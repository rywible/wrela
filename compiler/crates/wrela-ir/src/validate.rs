//! The IR's invariants. Every producer (the lowering, the derive pass) must keep them, and every
//! consumer (the back ends, the interpreter) may assume them. A failure is a compiler bug.
//!
//! What's checked: types are well formed and acyclic, and stored `GpuData` layouts are right;
//! names are unique; in each function, every value is defined once, before its uses and in scope,
//! every expression's operands have the types it needs and its result has the declared type,
//! stores go to writable places, `Break` and `Continue` are inside a loop, a function that
//! returns a value can't fall off its end, and no value, local or result holds an atomic (a
//! parameter that does is `InOut`); entry points, pipelines, exports and imports fit their
//! functions; and code reachable from a GPU entry point records no commands, calls no host
//! functions, can't trap or recurse, and uses only types the GPU has.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::module::{
    Arg, AtomicOp, BinaryOp, Block, Builtin, EntryParam, EntryPoint, Expr, Function, Interpolation,
    Io, IoBinding, MathOp, Module, ParamMode, PipelineKind, Place, PlaceRoot, Projection, Record,
    ResourceKind, Stage, Stmt, UnaryOp,
};
use crate::types::{Scalar, Type, Types, VectorSize};
use crate::{FuncId, TypeId, ValueId};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ValidationError {
    /// The function the problem is in, if it's in one.
    pub function: Option<String>,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.function {
            Some(function) => write!(f, "in `{function}`: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// Checks every invariant of `module`, reporting all the problems found.
pub fn validate(module: &Module) -> Result<(), Vec<ValidationError>> {
    let mut v = Validator {
        module,
        types: &module.types,
        errors: Vec::new(),
    };
    v.check_types();
    // Everything after this walks types, which is only safe once they're known to be acyclic.
    if v.errors.is_empty() {
        let atomic = v.atomic_types();
        v.check_names();
        for function in &module.functions {
            let errors = FnCx::check(module, function, &atomic);
            v.errors
                .extend(errors.into_iter().map(|message| ValidationError {
                    function: Some(function.name.clone()),
                    message,
                }));
        }
        v.check_imports();
        v.check_entry_points(&atomic);
        v.check_pipelines();
        v.check_exports();
        v.check_gpu_code();
    }
    if v.errors.is_empty() {
        Ok(())
    } else {
        Err(v.errors)
    }
}

struct Validator<'m> {
    module: &'m Module,
    types: &'m Types,
    errors: Vec<ValidationError>,
}

/// A scalar or vector type's scalar and, for a vector, its size.
fn shape(ty: &Type) -> Option<(Scalar, Option<VectorSize>)> {
    match *ty {
        Type::Scalar(s) => Some((s, None)),
        Type::Vector { size, scalar } => Some((scalar, Some(size))),
        _ => None,
    }
}

fn builtin_type(builtin: Builtin) -> Type {
    match builtin {
        Builtin::VertexIndex | Builtin::InstanceIndex => Type::Scalar(Scalar::U32),
        Builtin::ClipPosition | Builtin::FragCoord => Type::Vector {
            size: VectorSize::Four,
            scalar: Scalar::F32,
        },
        Builtin::GlobalId | Builtin::LocalId | Builtin::WorkgroupId => Type::Vector {
            size: VectorSize::Three,
            scalar: Scalar::U32,
        },
    }
}

/// Whether `builtin` is an input (`true`) or output of `stage`.
fn builtin_allowed(builtin: Builtin, stage: Stage, input: bool) -> bool {
    match builtin {
        Builtin::VertexIndex | Builtin::InstanceIndex => input && stage == Stage::Vertex,
        Builtin::ClipPosition => !input && stage == Stage::Vertex,
        Builtin::FragCoord => input && stage == Stage::Fragment,
        Builtin::GlobalId | Builtin::LocalId | Builtin::WorkgroupId => {
            input && matches!(stage, Stage::Compute { .. })
        }
    }
}

fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Vertex => "vertex",
        Stage::Fragment => "fragment",
        Stage::Compute { .. } => "compute",
    }
}

impl Validator<'_> {
    fn error(&mut self, message: String) {
        self.errors.push(ValidationError {
            function: None,
            message,
        });
    }

    fn name(&self, ty: TypeId) -> String {
        self.types.name(ty).to_string()
    }

    fn check_types(&mut self) {
        let mut nominal: BTreeMap<&str, TypeId> = BTreeMap::new();
        for (id, ty) in self.types.iter() {
            // Each type refers only to earlier ones, so the arena has no cycles.
            let refs: Vec<TypeId> = match ty {
                Type::Array { element, .. } | Type::Slice { element } => vec![*element],
                Type::Tuple(elements) => elements.clone(),
                Type::Struct(s) => s.fields.iter().map(|f| f.ty).collect(),
                Type::Enum(e) => e.variants.iter().flat_map(|v| v.fields.clone()).collect(),
                _ => Vec::new(),
            };
            for r in refs {
                match self.types.get(r) {
                    _ if r >= id => self.error(format!(
                        "type {id:?} refers to {r:?}, which isn't an earlier type"
                    )),
                    Some(Type::Slice { .. }) => self.error(format!(
                        "type {id:?} contains a slice; only parameters and resources have one"
                    )),
                    _ => {}
                }
            }
            match ty {
                Type::Vector { scalar, .. }
                    if !matches!(
                        scalar,
                        Scalar::Bool | Scalar::I32 | Scalar::U32 | Scalar::F32
                    ) =>
                {
                    self.error(format!("type {id:?}: a vector of {}", scalar.name()));
                }
                Type::Atomic(scalar) if !matches!(scalar, Scalar::I32 | Scalar::U32) => {
                    self.error(format!("type {id:?}: an atomic {}", scalar.name()));
                }
                Type::Tuple(elements) if elements.is_empty() => {
                    self.error(format!("type {id:?}: an empty tuple; use `()`"));
                }
                Type::Struct(s) => {
                    if nominal.insert(s.name.as_str(), id).is_some() || s.name.is_empty() {
                        self.error(format!("struct name `{}` is empty or not unique", s.name));
                    }
                    let mut names = BTreeSet::new();
                    if !s.fields.iter().all(|f| names.insert(&f.name)) {
                        self.error(format!("struct `{}` has two fields of one name", s.name));
                    }
                    if let Some(layout) = &s.gpu_layout {
                        let fields: Vec<TypeId> = s.fields.iter().map(|f| f.ty).collect();
                        match self.types.gpu_struct_layout(&fields) {
                            Ok(computed) if computed == *layout => {}
                            Ok(computed) => self.error(format!(
                                "struct `{}` stores the GPU layout {layout:?}, but its fields give \
                                 {computed:?}",
                                s.name
                            )),
                            Err(e) => {
                                self.error(format!(
                                    "struct `{}` has a GPU layout, but {e}",
                                    s.name
                                ));
                            }
                        }
                    }
                }
                Type::Enum(e) => {
                    if nominal.insert(e.name.as_str(), id).is_some() || e.name.is_empty() {
                        self.error(format!("enum name `{}` is empty or not unique", e.name));
                    }
                    let mut names = BTreeSet::new();
                    if e.variants.is_empty() || !e.variants.iter().all(|v| names.insert(&v.name)) {
                        self.error(format!(
                            "enum `{}` needs variants, each with its own name",
                            e.name
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    fn check_names(&mut self) {
        let mut names = BTreeSet::new();
        for f in &self.module.functions {
            if f.name.is_empty() || !names.insert(f.name.as_str()) {
                self.error(format!("function name `{}` is empty or not unique", f.name));
            }
        }
    }

    fn is_scalar(&self, ty: TypeId) -> bool {
        matches!(self.types.get(ty), Some(Type::Scalar(_)))
    }

    fn is_unit(&self, ty: TypeId) -> bool {
        matches!(self.types.get(ty), Some(Type::Unit))
    }

    fn check_imports(&mut self) {
        let mut seen = BTreeSet::new();
        for import in &self.module.imports {
            let key = (import.module.as_str(), import.name.as_str());
            if import.module.is_empty() || import.name.is_empty() || !seen.insert(key) {
                self.error(format!(
                    "import `{}.{}` is unnamed or imported twice",
                    import.module, import.name
                ));
            }
            let scalars = import.params.iter().all(|&t| self.is_scalar(t));
            if !scalars || !(self.is_unit(import.ret) || self.is_scalar(import.ret)) {
                self.error(format!(
                    "import `{}.{}` must take and return scalars",
                    import.module, import.name
                ));
            }
        }
    }

    fn check_exports(&mut self) {
        let mut names = BTreeSet::new();
        for export in &self.module.exports {
            if export.name.is_empty() || !names.insert(export.name.as_str()) {
                self.error(format!(
                    "export `{}` is unnamed or exported twice",
                    export.name
                ));
            }
            let Some(f) = self.module.function(export.function) else {
                self.error(format!("export `{}` names a missing function", export.name));
                continue;
            };
            let scalars = f
                .params
                .iter()
                .all(|p| p.mode == ParamMode::In && self.is_scalar(p.ty));
            if !scalars || !(self.is_unit(f.ret) || self.is_scalar(f.ret)) {
                self.error(format!(
                    "export `{}`: an exported function takes `In` scalars and returns `()` or a \
                     scalar",
                    export.name
                ));
            }
        }
    }

    /// Checks one stage input or output binding against the type it carries.
    fn check_io_binding(
        &mut self,
        what: &str,
        stage: Stage,
        binding: IoBinding,
        ty: TypeId,
        input: bool,
        locations: &mut BTreeSet<u32>,
    ) {
        let Some(actual) = self.types.get(ty) else {
            return self.error(format!("{what}: type {ty:?} doesn't exist"));
        };
        match binding {
            IoBinding::Builtin(builtin) => {
                if !builtin_allowed(builtin, stage, input) {
                    let direction = if input { "an input" } else { "an output" };
                    self.error(format!(
                        "{what}: {} isn't {direction} of a {} entry point",
                        builtin.name(),
                        stage_name(stage)
                    ));
                }
                if *actual != builtin_type(builtin) {
                    self.error(format!(
                        "{what}: {} has the wrong type {}",
                        builtin.name(),
                        self.name(ty)
                    ));
                }
            }
            IoBinding::Location {
                location,
                interpolation,
            } => {
                let inter_stage =
                    (stage == Stage::Vertex && !input) || (stage == Stage::Fragment && input);
                let fragment_out = stage == Stage::Fragment && !input;
                if !(inter_stage || fragment_out) {
                    self.error(format!(
                        "{what}: a {} entry point has no location {}",
                        stage_name(stage),
                        if input { "inputs" } else { "outputs" }
                    ));
                }
                match shape(actual) {
                    Some((s, _)) if matches!(s, Scalar::I32 | Scalar::U32 | Scalar::F32) => {
                        if inter_stage && s != Scalar::F32 && interpolation != Interpolation::Flat {
                            self.error(format!(
                                "{what}: location {location} holds integers, so it must be flat"
                            ));
                        }
                    }
                    _ => self.error(format!(
                        "{what}: location {location} must hold an i32, u32 or f32 scalar or \
                         vector, not {}",
                        self.name(ty)
                    )),
                }
                if !locations.insert(location) {
                    self.error(format!("{what}: location {location} is used twice"));
                }
            }
        }
    }

    fn check_io(&mut self, what: &str, stage: Stage, io: &Io, ty: TypeId, input: bool) {
        let mut locations = BTreeSet::new();
        self.check_io_into(what, stage, io, ty, input, &mut locations);
    }

    fn check_io_into(
        &mut self,
        what: &str,
        stage: Stage,
        io: &Io,
        ty: TypeId,
        input: bool,
        locations: &mut BTreeSet<u32>,
    ) {
        match io {
            Io::Binding(b) => self.check_io_binding(what, stage, *b, ty, input, locations),
            Io::Members(bindings) => match self.types.get(ty) {
                Some(Type::Struct(s)) if s.fields.len() == bindings.len() => {
                    for (field, b) in s.fields.iter().zip(bindings) {
                        self.check_io_binding(what, stage, *b, field.ty, input, locations);
                    }
                }
                _ => self.error(format!(
                    "{what}: {} isn't a struct with one field per binding",
                    self.name(ty)
                )),
            },
        }
    }

    /// The bindings an `Io` makes, with the type each carries.
    fn io_bindings(&self, io: &Io, ty: TypeId) -> Vec<(IoBinding, TypeId)> {
        match io {
            Io::Binding(b) => vec![(*b, ty)],
            Io::Members(bindings) => match self.types.get(ty) {
                Some(Type::Struct(s)) => s
                    .fields
                    .iter()
                    .zip(bindings)
                    .map(|(f, b)| (*b, f.ty))
                    .collect(),
                _ => Vec::new(),
            },
        }
    }

    /// `atomic` is [`Validator::atomic_types`].
    fn check_entry_points(&mut self, atomic: &[bool]) {
        let module = self.module;
        for (index, entry) in module.entry_points.iter().enumerate() {
            let Some(f) = module.function(entry.function) else {
                self.error(format!("entry point {index} names a missing function"));
                continue;
            };
            let what = format!("entry point `{}`", f.name);
            self.check_entry_point(&what, entry, f, atomic);
        }
    }

    /// `atomic` is [`Validator::atomic_types`].
    fn check_entry_point(&mut self, what: &str, entry: &EntryPoint, f: &Function, atomic: &[bool]) {
        let stage = entry.stage;
        if entry.params.len() != f.params.len() {
            self.error(format!(
                "{what}: {} parameter bindings for {} parameters",
                entry.params.len(),
                f.params.len()
            ));
        }
        let mut resources = BTreeSet::new();
        for r in &entry.resources {
            if !resources.insert((r.group, r.binding)) {
                self.error(format!(
                    "{what}: group {} binding {} is bound twice",
                    r.group, r.binding
                ));
            }
            // A uniform holds GPU data; storage may also hold a runtime-sized slice of it. Only
            // writable storage may hold atomics, at any depth (WGSL allows them nowhere else).
            let fits = match (r.kind, self.types.get(r.ty)) {
                (ResourceKind::Uniform, Some(Type::Slice { .. })) => false,
                (ResourceKind::Uniform | ResourceKind::StorageRead, _)
                    if atomic.get(r.ty.index()).copied().unwrap_or(false) =>
                {
                    false
                }
                (_, Some(Type::Slice { element })) => self.types.gpu_size_align(*element).is_ok(),
                _ => self.types.gpu_size_align(r.ty).is_ok(),
            };
            if !fits {
                self.error(format!(
                    "{what}: group {} binding {} can't hold {}",
                    r.group,
                    r.binding,
                    self.name(r.ty)
                ));
            }
        }
        let mut locations = BTreeSet::new();
        for (i, (param, binding)) in f.params.iter().zip(&entry.params).enumerate() {
            let what = format!("{what}, parameter {i}");
            match binding {
                EntryParam::Io(io) => {
                    if param.mode != ParamMode::In {
                        self.error(format!("{what}: a stage input is `In`"));
                    }
                    self.check_io_into(&what, stage, io, param.ty, true, &mut locations);
                }
                EntryParam::Resource { resource, member } => {
                    let Some(r) = entry.resources.get(*resource as usize) else {
                        self.error(format!("{what}: resource {resource} doesn't exist"));
                        continue;
                    };
                    let expected = match member {
                        None => Some(r.ty),
                        Some(m) => match self.types.get(r.ty) {
                            Some(Type::Struct(s)) => s.fields.get(*m as usize).map(|f| f.ty),
                            _ => None,
                        },
                    };
                    if expected != Some(param.ty) {
                        self.error(format!(
                            "{what}: has type {}, which isn't what its resource holds",
                            self.name(param.ty)
                        ));
                    }
                    let mode = if r.kind == ResourceKind::StorageReadWrite {
                        ParamMode::InOut
                    } else {
                        ParamMode::In
                    };
                    if param.mode != mode {
                        self.error(format!("{what}: should be {mode:?}, for its resource"));
                    }
                }
            }
        }
        match (stage, &entry.result) {
            (Stage::Compute { workgroup_size }, result) => {
                if result.is_some() || !self.is_unit(f.ret) {
                    self.error(format!("{what}: a compute entry point returns `()`"));
                }
                if workgroup_size.contains(&0) {
                    self.error(format!("{what}: a workgroup size of 0"));
                }
            }
            (_, None) => self.error(format!(
                "{what}: a {} entry point needs a result binding",
                stage_name(stage)
            )),
            (_, Some(io)) => {
                self.check_io(what, stage, io, f.ret, false);
                let clip = self
                    .io_bindings(io, f.ret)
                    .iter()
                    .filter(|(b, _)| *b == IoBinding::Builtin(Builtin::ClipPosition))
                    .count();
                if stage == Stage::Vertex && clip != 1 {
                    self.error(format!(
                        "{what}: a vertex entry point outputs one clip_position"
                    ));
                }
            }
        }
    }

    fn check_pipelines(&mut self) {
        let module = self.module;
        for (index, pipeline) in module.pipelines.iter().enumerate() {
            let what = format!("pipeline {index}");
            let entry = |id: crate::EntryId| module.entry_point(id);
            match pipeline.kind {
                PipelineKind::Compute { entry: e } => match entry(e) {
                    Some(e) if matches!(e.stage, Stage::Compute { .. }) => {}
                    _ => self.error(format!("{what}: its entry point isn't a compute one")),
                },
                PipelineKind::Render { vertex, fragment } => {
                    let (Some(v), Some(fr)) = (entry(vertex), entry(fragment)) else {
                        self.error(format!("{what}: names a missing entry point"));
                        continue;
                    };
                    if v.stage != Stage::Vertex || fr.stage != Stage::Fragment {
                        self.error(format!("{what}: needs a vertex and a fragment entry point"));
                        continue;
                    }
                    self.check_interface(&what, v, fr);
                    for a in &v.resources {
                        for b in &fr.resources {
                            if (a.group, a.binding) == (b.group, b.binding) && a != b {
                                self.error(format!(
                                    "{what}: its stages disagree about group {} binding {}",
                                    a.group, a.binding
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    /// The fragment stage's location inputs must be vertex outputs of the same type and
    /// interpolation.
    fn check_interface(&mut self, what: &str, vertex: &EntryPoint, fragment: &EntryPoint) {
        let locations = |pairs: Vec<(IoBinding, TypeId)>| -> BTreeMap<u32, (TypeId, IoBinding)> {
            pairs
                .into_iter()
                .filter_map(|(b, ty)| match b {
                    IoBinding::Location { location, .. } => Some((location, (ty, b))),
                    IoBinding::Builtin(_) => None,
                })
                .collect()
        };
        let Some(vf) = self.module.function(vertex.function) else {
            return;
        };
        let Some(ff) = self.module.function(fragment.function) else {
            return;
        };
        let outputs = match &vertex.result {
            Some(io) => locations(self.io_bindings(io, vf.ret)),
            None => BTreeMap::new(),
        };
        let mut inputs = Vec::new();
        for (param, binding) in ff.params.iter().zip(&fragment.params) {
            if let EntryParam::Io(io) = binding {
                inputs.extend(self.io_bindings(io, param.ty));
            }
        }
        for (location, input) in locations(inputs) {
            if outputs.get(&location) != Some(&input) {
                self.error(format!(
                    "{what}: fragment location {location} doesn't match a vertex output of the \
                     same type and interpolation"
                ));
            }
        }
    }

    /// Functions reachable from GPU entry points, through calls and derived interpretations.
    fn gpu_functions(&self) -> BTreeSet<FuncId> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<FuncId> = self
            .module
            .entry_points
            .iter()
            .map(|e| e.function)
            .collect();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(f) = self.module.function(id) {
                stack.extend(callees(f));
            }
        }
        seen
    }

    /// Which types, by id, the GPU has: nothing inside them is a scalar it lacks.
    fn gpu_types(&self) -> Vec<bool> {
        per_type(self.types, |ty, gpu| match ty {
            Type::Unit | Type::Matrix { .. } | Type::Atomic(_) => true,
            Type::Scalar(s) | Type::Vector { scalar: s, .. } => s.is_gpu(),
            Type::Array { element, .. } | Type::Slice { element } => gpu(*element),
            Type::Tuple(elements) => elements.iter().all(|&e| gpu(e)),
            Type::Struct(s) => s.fields.iter().all(|f| gpu(f.ty)),
            Type::Enum(e) => e.variants.iter().all(|v| v.fields.iter().all(|&t| gpu(t))),
        })
    }

    /// Which types, by id, hold an atomic at any depth.
    fn atomic_types(&self) -> Vec<bool> {
        per_type(self.types, |ty, atomic| match ty {
            Type::Atomic(_) => true,
            Type::Unit | Type::Scalar(_) | Type::Vector { .. } | Type::Matrix { .. } => false,
            Type::Array { element, .. } | Type::Slice { element } => atomic(*element),
            Type::Tuple(elements) => elements.iter().any(|&e| atomic(e)),
            Type::Struct(s) => s.fields.iter().any(|f| atomic(f.ty)),
            Type::Enum(e) => e
                .variants
                .iter()
                .any(|v| v.fields.iter().any(|&t| atomic(t))),
        })
    }

    fn check_gpu_code(&mut self) {
        let gpu = self.gpu_functions();
        let gpu_types = self.gpu_types();
        for &id in &gpu {
            let Some(f) = self.module.function(id) else {
                continue;
            };
            let mut problems = BTreeSet::new();
            visit(&f.body, &mut |stmt| match stmt {
                Stmt::Record(_) => {
                    problems.insert("records a GPU command");
                }
                Stmt::Trap => {
                    problems.insert("can trap");
                }
                Stmt::Let(_, Expr::CallImport { .. }) => {
                    problems.insert("calls a host function");
                }
                _ => {}
            });
            let types = f
                .params
                .iter()
                .map(|p| p.ty)
                .chain(f.locals.iter().map(|l| l.ty))
                .chain(f.values.iter().copied())
                .chain([f.ret]);
            let on_gpu = |t: TypeId| gpu_types.get(t.index()).copied().unwrap_or(false);
            if let Some(ty) = types.into_iter().find(|&t| !on_gpu(t)) {
                let message = format!(
                    "is GPU code but uses {}, which the GPU doesn't have",
                    self.name(ty)
                );
                self.errors.push(ValidationError {
                    function: Some(f.name.clone()),
                    message,
                });
            }
            for problem in problems {
                self.errors.push(ValidationError {
                    function: Some(f.name.clone()),
                    message: format!("is GPU code but {problem}"),
                });
            }
        }
        // GPU code can't recurse. Peel off functions that no remaining GPU function calls
        // (Kahn's algorithm, so a deep call chain can't overflow the stack); whatever is left is
        // on a cycle or called from one.
        let calls: BTreeMap<FuncId, Vec<FuncId>> = gpu
            .iter()
            .map(|&id| {
                (
                    id,
                    self.module.function(id).map(callees).unwrap_or_default(),
                )
            })
            .collect();
        let mut callers: BTreeMap<FuncId, usize> = gpu.iter().map(|&id| (id, 0)).collect();
        for callee in calls.values().flatten() {
            if let Some(n) = callers.get_mut(callee) {
                *n += 1;
            }
        }
        let mut ready: Vec<FuncId> = callers
            .iter()
            .filter(|&(_, &n)| n == 0)
            .map(|(&id, _)| id)
            .collect();
        while let Some(id) = ready.pop() {
            for callee in calls.get(&id).into_iter().flatten() {
                if let Some(n) = callers.get_mut(callee) {
                    *n -= 1;
                    if *n == 0 {
                        ready.push(*callee);
                    }
                }
            }
        }
        let left: BTreeSet<FuncId> = callers
            .iter()
            .filter(|&(_, &n)| n > 0)
            .map(|(&id, _)| id)
            .collect();
        // Every function left has a caller that's left too: walk back along callers until one
        // repeats, and name that one, which is on the cycle.
        let mut caller_of: BTreeMap<FuncId, FuncId> = BTreeMap::new();
        for &id in &left {
            for callee in calls.get(&id).into_iter().flatten() {
                if left.contains(callee) {
                    caller_of.entry(*callee).or_insert(id);
                }
            }
        }
        let mut walked = BTreeSet::new();
        let mut at = left.first().copied();
        while let Some(id) = at {
            if !walked.insert(id) {
                self.errors.push(ValidationError {
                    function: self.module.function(id).map(|f| f.name.clone()),
                    message: "is GPU code but recurses".to_string(),
                });
                break;
            }
            at = caller_of.get(&id).copied();
        }
    }
}

/// A property of every type, by id, from `rule`, which sees the type and the property of the
/// types it refers to. Types refer only to earlier ones (`check_types`), so one pass in id order
/// does it: no recursion to overflow the stack on a deep type, and a type shared many times
/// inside another is still looked at once.
fn per_type(types: &Types, rule: impl Fn(&Type, &dyn Fn(TypeId) -> bool) -> bool) -> Vec<bool> {
    let mut table = Vec::with_capacity(types.len());
    for (_, ty) in types.iter() {
        let value = rule(ty, &|id: TypeId| {
            table.get(id.index()).copied().unwrap_or(false)
        });
        table.push(value);
    }
    table
}

/// Calls every statement in `block`, nested blocks included, in order.
fn visit(block: &Block, f: &mut impl FnMut(&Stmt)) {
    for stmt in &block.0 {
        f(stmt);
        match stmt {
            Stmt::If {
                then, otherwise, ..
            } => {
                visit(then, f);
                visit(otherwise, f);
            }
            Stmt::Loop { body, continuing } => {
                visit(body, f);
                visit(continuing, f);
            }
            _ => {}
        }
    }
}

/// The functions `f` calls, directly or through a derived interpretation.
fn callees(f: &Function) -> Vec<FuncId> {
    let mut out = Vec::new();
    visit(&f.body, &mut |stmt| {
        if let Stmt::Let(_, Expr::Call { func, .. } | Expr::Derived { func, .. }) = stmt {
            out.push(*func);
        }
    });
    out
}

/// Whether control can't flow past the end of `block`.
fn diverges(block: &Block) -> bool {
    block.0.iter().any(|stmt| match stmt {
        Stmt::Return(_) | Stmt::Trap | Stmt::Break | Stmt::Continue => true,
        Stmt::If {
            then, otherwise, ..
        } => diverges(then) && diverges(otherwise),
        Stmt::Loop { body, .. } => !breaks(body),
        _ => false,
    })
}

/// Whether `block` has a `Break` that leaves the loop whose body it is.
fn breaks(block: &Block) -> bool {
    block.0.iter().any(|stmt| match stmt {
        Stmt::Break => true,
        Stmt::If {
            then, otherwise, ..
        } => breaks(then) || breaks(otherwise),
        _ => false,
    })
}

/// An expression's type: an existing id, a structural type that may not be in the arena (its id,
/// if it has one, is the only id it can match), or anything (a derived interpretation's result).
#[derive(Clone, PartialEq, Debug)]
enum Ty {
    Id(TypeId),
    New(Type),
    Any,
}

/// The checks inside one function.
struct FnCx<'a> {
    module: &'a Module,
    types: &'a Types,
    f: &'a Function,
    defined: Vec<bool>,
    in_scope: Vec<bool>,
    scope: Vec<ValueId>,
    loops: u32,
    in_continuing: bool,
    errors: Vec<String>,
}

impl<'a> FnCx<'a> {
    /// `atomic` is [`Validator::atomic_types`].
    fn check(module: &'a Module, f: &'a Function, atomic: &[bool]) -> Vec<String> {
        let mut cx = FnCx {
            module,
            types: &module.types,
            f,
            defined: vec![false; f.values.len()],
            in_scope: vec![false; f.values.len()],
            scope: Vec::new(),
            loops: 0,
            in_continuing: false,
            errors: Vec::new(),
        };
        for (i, p) in f.params.iter().enumerate() {
            if cx.types.get(p.ty).is_none() {
                cx.errors.push(format!("parameter {i} has no type"));
            }
        }
        for (i, l) in f.locals.iter().enumerate() {
            if !matches!(cx.types.get(l.ty), Some(t) if !matches!(t, Type::Slice { .. })) {
                cx.errors
                    .push(format!("local l{i} needs a type with a size"));
            }
        }
        let tys = f.values.iter().chain([&f.ret]);
        if tys.into_iter().any(|&t| cx.types.get(t).is_none()) {
            cx.errors
                .push("a value or the result has a type that doesn't exist".to_string());
        }
        // An atomic is reached only through a place, with `Expr::Atomic`: WGSL has no atomic
        // values or function-scope atomics, and the CPU needs neither. A parameter is a place,
        // but one holding an atomic must be writable, as atomics are only in writable storage.
        let holds_atomic = |t: TypeId| atomic.get(t.index()).copied().unwrap_or(false);
        let only_places = "atomics are reached only through places";
        for (i, p) in f.params.iter().enumerate() {
            if holds_atomic(p.ty) && p.mode != ParamMode::InOut {
                cx.errors.push(format!(
                    "parameter {i} holds an atomic, so it must be `InOut`"
                ));
            }
        }
        for (i, l) in f.locals.iter().enumerate() {
            if holds_atomic(l.ty) {
                cx.errors
                    .push(format!("local l{i} holds an atomic; {only_places}"));
            }
        }
        for (i, &t) in f.values.iter().enumerate() {
            if holds_atomic(t) {
                cx.errors
                    .push(format!("%{i} holds an atomic; {only_places}"));
            }
        }
        if holds_atomic(f.ret) {
            cx.errors
                .push(format!("its result holds an atomic; {only_places}"));
        }
        cx.block(&f.body);
        if !cx.is(f.ret, |t| matches!(t, Type::Unit)) && !diverges(&f.body) {
            let message = format!(
                "can reach its end without returning a {}",
                cx.types.name(f.ret)
            );
            cx.errors.push(message);
        }
        for (i, defined) in cx.defined.iter().enumerate() {
            if !defined {
                cx.errors
                    .push(format!("%{i} is declared but never defined"));
            }
        }
        cx.errors
    }

    fn err<T>(&mut self, message: String) -> Option<T> {
        self.errors.push(message);
        None
    }

    fn is(&self, ty: TypeId, f: impl FnOnce(&Type) -> bool) -> bool {
        self.types.get(ty).is_some_and(f)
    }

    fn describe(&self, ty: &Ty) -> String {
        match ty {
            Ty::Id(id) => self.types.name(*id).to_string(),
            Ty::Any => "any type".to_string(),
            Ty::New(t) => match self.types.lookup(t) {
                Some(id) => self.types.name(id).to_string(),
                None => match t {
                    Type::Unit => "()".to_string(),
                    Type::Scalar(s) => s.name().to_string(),
                    Type::Vector { size, scalar } => {
                        format!("vec{}<{}>", size.count(), scalar.name())
                    }
                    Type::Matrix { columns, rows } => {
                        format!("mat{}x{}<f32>", columns.count(), rows.count())
                    }
                    other => format!("{other:?}"),
                },
            },
        }
    }

    fn matches(&self, declared: TypeId, ty: &Ty) -> bool {
        match ty {
            Ty::Id(id) => *id == declared,
            Ty::New(t) => self.types.get(declared) == Some(t),
            Ty::Any => true,
        }
    }

    fn resolve(&self, ty: &Ty) -> Option<Type> {
        match ty {
            Ty::Id(id) => self.types.get(*id).cloned(),
            Ty::New(t) => Some(t.clone()),
            Ty::Any => None,
        }
    }

    /// The type of value `v`, if it's in scope here.
    fn value(&mut self, v: ValueId) -> Option<TypeId> {
        let Some(&ty) = self.f.values.get(v.index()) else {
            return self.err(format!("%{} doesn't exist", v.0));
        };
        if !self.in_scope[v.index()] {
            return self.err(format!(
                "%{} is used before it's defined, or outside the block that defines it",
                v.0
            ));
        }
        Some(ty)
    }

    /// The type of value `v`, resolved.
    fn value_type(&mut self, v: ValueId) -> Option<Type> {
        let ty = self.value(v)?;
        self.types.get(ty).cloned()
    }

    fn value_shape(&mut self, v: ValueId) -> Option<(Scalar, Option<VectorSize>)> {
        let ty = self.value_type(v)?;
        match shape(&ty) {
            Some(s) => Some(s),
            None => self.err(format!("%{} must be a scalar or vector", v.0)),
        }
    }

    fn expect(&mut self, v: ValueId, expected: &Type, what: &str) {
        if let Some(ty) = self.value(v)
            && self.types.get(ty) != Some(expected)
        {
            let expected = self.describe(&Ty::New(expected.clone()));
            self.errors.push(format!(
                "{what} must be a {expected}, but %{} is a {}",
                v.0,
                self.types.name(ty)
            ));
        }
    }

    fn expect_index(&mut self, v: ValueId) {
        if let Some(ty) = self.value_type(v)
            && !matches!(ty, Type::Scalar(Scalar::I32 | Scalar::U32))
        {
            self.errors
                .push(format!("index %{} must be an i32 or u32", v.0));
        }
    }

    fn block(&mut self, block: &Block) {
        let mark = self.scope.len();
        for stmt in &block.0 {
            self.stmt(stmt);
        }
        for v in self.scope.split_off(mark) {
            self.in_scope[v.index()] = false;
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(v, expr) => {
                let ty = self.expr(expr);
                let Some(&declared) = self.f.values.get(v.index()) else {
                    return self.errors.push(format!("%{} has no declared type", v.0));
                };
                if self.defined[v.index()] {
                    return self.errors.push(format!("%{} is defined twice", v.0));
                }
                if let Some(ty) = ty
                    && !self.matches(declared, &ty)
                {
                    let message = format!(
                        "%{} is declared {} but its expression is a {}",
                        v.0,
                        self.types.name(declared),
                        self.describe(&ty)
                    );
                    self.errors.push(message);
                }
                self.defined[v.index()] = true;
                self.in_scope[v.index()] = true;
                self.scope.push(*v);
            }
            Stmt::Store(place, v) => {
                let place_ty = self.place(place, true);
                let value_ty = self.value(*v);
                if let Some(ty) = &place_ty
                    && let Some(Type::Atomic(_) | Type::Slice { .. }) = self.resolve(ty)
                {
                    return self
                        .errors
                        .push("a store can't write an atomic or a whole slice".to_string());
                }
                if let (Some(pt), Some(vt)) = (place_ty, value_ty)
                    && !self.matches(vt, &pt)
                {
                    let message = format!(
                        "can't store %{}, a {}, into a place of type {}",
                        v.0,
                        self.types.name(vt),
                        self.describe(&pt)
                    );
                    self.errors.push(message);
                }
            }
            Stmt::If {
                cond,
                then,
                otherwise,
            } => {
                self.expect(*cond, &Type::Scalar(Scalar::Bool), "an `if` condition");
                self.block(then);
                self.block(otherwise);
            }
            Stmt::Loop { body, continuing } => {
                self.loops += 1;
                self.block(body);
                self.loops -= 1;
                let saved = (self.loops, self.in_continuing);
                (self.loops, self.in_continuing) = (0, true);
                self.block(continuing);
                (self.loops, self.in_continuing) = saved;
            }
            Stmt::Break | Stmt::Continue => {
                if self.loops == 0 {
                    let message = if self.in_continuing {
                        "a loop's `continuing` block can't break or continue"
                    } else {
                        "`break` or `continue` outside a loop"
                    };
                    self.errors.push(message.to_string());
                }
            }
            Stmt::Return(value) => {
                if self.in_continuing {
                    self.errors
                        .push("a loop's `continuing` block can't return".to_string());
                }
                let ret = self.f.ret;
                let unit = self.is(ret, |t| matches!(t, Type::Unit));
                match value {
                    None if !unit => self.errors.push(format!(
                        "returns nothing from a function that returns {}",
                        self.types.name(ret)
                    )),
                    Some(_) if unit => self
                        .errors
                        .push("returns a value from a function that returns ()".to_string()),
                    Some(v) => {
                        if let Some(ty) = self.value(*v)
                            && ty != ret
                        {
                            let message = format!(
                                "returns a {} from a function that returns {}",
                                self.types.name(ty),
                                self.types.name(ret)
                            );
                            self.errors.push(message);
                        }
                    }
                    None => {}
                }
            }
            Stmt::Record(record) => self.record(record),
            Stmt::Trap => {}
        }
    }

    fn record(&mut self, record: &Record) {
        let u32_ty = Type::Scalar(Scalar::U32);
        match record {
            Record::BeginScreenPass { clear } => {
                let vec4 = Type::Vector {
                    size: VectorSize::Four,
                    scalar: Scalar::F32,
                };
                self.expect(*clear, &vec4, "a clear colour");
            }
            Record::Draw {
                pipeline,
                vertex_count,
                instance_count,
                uniforms,
            } => {
                self.expect(*vertex_count, &u32_ty, "a vertex count");
                self.expect(*instance_count, &u32_ty, "an instance count");
                match self.module.pipeline(*pipeline).map(|p| p.kind) {
                    Some(PipelineKind::Render { .. }) => {}
                    _ => {
                        return self.errors.push(format!(
                            "draws with pipeline {}, which isn't a render pipeline",
                            pipeline.0
                        ));
                    }
                }
                let expected = self.module.inline_uniform(*pipeline).map(|r| r.ty);
                match (expected, uniforms) {
                    (None, None) => {}
                    (Some(ty), Some(v)) => {
                        if let Some(actual) = self.value(*v)
                            && actual != ty
                        {
                            let message = format!(
                                "draws pipeline {} with uniforms of type {}, not {}",
                                pipeline.0,
                                self.types.name(actual),
                                self.types.name(ty)
                            );
                            self.errors.push(message);
                        }
                    }
                    (Some(_), None) => self
                        .errors
                        .push(format!("pipeline {} needs uniforms", pipeline.0)),
                    (None, Some(_)) => self
                        .errors
                        .push(format!("pipeline {} takes no uniforms", pipeline.0)),
                }
            }
            Record::Present => {}
        }
    }

    /// A place's type; with `write`, the place must be writable.
    fn place(&mut self, place: &Place, write: bool) -> Option<Ty> {
        let root = match place.root {
            PlaceRoot::Local(l) => match self.f.locals.get(l.index()) {
                Some(local) => local.ty,
                None => return self.err(format!("local l{} doesn't exist", l.0)),
            },
            PlaceRoot::Param(i) => match self.f.params.get(i as usize) {
                Some(p) if write && p.mode != ParamMode::InOut => {
                    return self.err(format!(
                        "writes parameter p{i} (`{}`), which isn't `InOut`",
                        p.name
                    ));
                }
                Some(p) => p.ty,
                None => return self.err(format!("parameter p{i} doesn't exist")),
            },
        };
        let mut current = Ty::Id(root);
        for projection in &place.projections {
            let ty = self.resolve(&current)?;
            let next = match *projection {
                Projection::Field(i) => match &ty {
                    Type::Struct(s) => s.fields.get(i as usize).map(|f| Ty::Id(f.ty)),
                    Type::Tuple(elements) => elements.get(i as usize).map(|&e| Ty::Id(e)),
                    Type::Vector { size, scalar } if i < size.count() => {
                        Some(Ty::New(Type::Scalar(*scalar)))
                    }
                    Type::Matrix { columns, rows } if i < columns.count() => {
                        Some(Ty::New(Type::Vector {
                            size: *rows,
                            scalar: Scalar::F32,
                        }))
                    }
                    _ => None,
                },
                Projection::Index(v) => {
                    self.expect_index(v);
                    match &ty {
                        Type::Array { element, .. } | Type::Slice { element } => {
                            Some(Ty::Id(*element))
                        }
                        Type::Vector { scalar, .. } => Some(Ty::New(Type::Scalar(*scalar))),
                        Type::Matrix { rows, .. } => Some(Ty::New(Type::Vector {
                            size: *rows,
                            scalar: Scalar::F32,
                        })),
                        _ => None,
                    }
                }
            };
            match next {
                Some(next) => current = next,
                None => {
                    let message = format!(
                        "{projection:?} doesn't apply to a {}",
                        self.describe(&current)
                    );
                    return self.err(message);
                }
            }
        }
        Some(current)
    }

    fn expr(&mut self, expr: &Expr) -> Option<Ty> {
        match expr {
            Expr::Const(c) => Some(Ty::New(Type::Scalar(c.scalar()))),
            Expr::Unary(op, v) => {
                let (scalar, _) = self.value_shape(*v)?;
                let ok = match op {
                    UnaryOp::Neg => scalar.is_signed(),
                    UnaryOp::Not => scalar == Scalar::Bool,
                    UnaryOp::BitNot => scalar.is_int(),
                };
                if !ok {
                    return self.err(format!("{op:?} doesn't apply to %{}", v.0));
                }
                Some(Ty::Id(self.value(*v)?))
            }
            Expr::Binary(op, a, b) => self.binary(*op, *a, *b),
            Expr::Load(place) => {
                let ty = self.place(place, false)?;
                if let Some(Type::Atomic(_) | Type::Slice { .. }) = self.resolve(&ty) {
                    return self.err("a load can't read an atomic or a whole slice".to_string());
                }
                Some(ty)
            }
            Expr::Construct { ty, parts } => self.construct(*ty, parts),
            Expr::Extract { value, index } => {
                let ty = self.value_type(*value)?;
                let i = *index as usize;
                let result = match &ty {
                    Type::Struct(s) => s.fields.get(i).map(|f| Ty::Id(f.ty)),
                    Type::Tuple(elements) => elements.get(i).map(|&e| Ty::Id(e)),
                    Type::Vector { size, scalar } if *index < size.count() => {
                        Some(Ty::New(Type::Scalar(*scalar)))
                    }
                    Type::Matrix { columns, rows } if *index < columns.count() => {
                        Some(Ty::New(Type::Vector {
                            size: *rows,
                            scalar: Scalar::F32,
                        }))
                    }
                    Type::Array { element, len } if *index < *len => Some(Ty::Id(*element)),
                    _ => None,
                };
                match result {
                    Some(t) => Some(t),
                    None => self.err(format!("%{} has no element {index}", value.0)),
                }
            }
            Expr::Index { value, index } => {
                self.expect_index(*index);
                match self.value_type(*value)? {
                    Type::Array { element, .. } => Some(Ty::Id(element)),
                    Type::Vector { scalar, .. } => Some(Ty::New(Type::Scalar(scalar))),
                    Type::Matrix { rows, .. } => Some(Ty::New(Type::Vector {
                        size: rows,
                        scalar: Scalar::F32,
                    })),
                    _ => self.err(format!("%{} can't be indexed", value.0)),
                }
            }
            Expr::Swizzle { value, components } => {
                let (scalar, size) = self.value_shape(*value)?;
                let Some(size) = size else {
                    return self.err(format!("%{} isn't a vector", value.0));
                };
                let count = u32::try_from(components.len()).unwrap_or(0);
                let result = VectorSize::from_count(count);
                match result {
                    Some(result) if components.iter().all(|&c| u32::from(c) < size.count()) => {
                        Some(Ty::New(Type::Vector {
                            size: result,
                            scalar,
                        }))
                    }
                    _ => self.err(format!(
                        "swizzle {components:?} of %{} needs 2 to 4 components that exist",
                        value.0
                    )),
                }
            }
            Expr::Splat { ty, value } => match self.types.get(*ty) {
                Some(Type::Vector { scalar, .. }) => {
                    let scalar = Type::Scalar(*scalar);
                    self.expect(*value, &scalar, "a splatted value");
                    Some(Ty::Id(*ty))
                }
                _ => self.err(format!("splat to {}, not a vector", self.types.name(*ty))),
            },
            Expr::Convert { ty, value } => {
                let (_, from) = self.value_shape(*value)?;
                match self.types.get(*ty).and_then(shape) {
                    Some((_, to)) if to == from => Some(Ty::Id(*ty)),
                    _ => self.err(format!(
                        "can't convert %{} to {}",
                        value.0,
                        self.types.name(*ty)
                    )),
                }
            }
            Expr::Bitcast { ty, value } => {
                let width = |(s, n): (Scalar, Option<VectorSize>)| {
                    s.bits().map(|b| b * n.map_or(1, VectorSize::count))
                };
                let from = self.value_shape(*value).and_then(width);
                let to = self.types.get(*ty).and_then(shape).and_then(width);
                match (from, to) {
                    (Some(a), Some(b)) if a == b => Some(Ty::Id(*ty)),
                    _ => self.err(format!(
                        "can't bitcast %{} to {}: the widths differ",
                        value.0,
                        self.types.name(*ty)
                    )),
                }
            }
            Expr::Math(op, args) => self.math(*op, args),
            Expr::Select {
                cond,
                accept,
                reject,
            } => {
                let a = self.value(*accept)?;
                let b = self.value(*reject)?;
                if a != b {
                    return self.err("`select` needs two values of one type".to_string());
                }
                let (scalar, size) = self.value_shape(*cond)?;
                let fits = scalar == Scalar::Bool
                    && (size.is_none() || self.is(a, |t| shape(t).is_some_and(|(_, n)| n == size)));
                if !fits {
                    return self.err(format!(
                        "`select` condition %{} must be a bool, or a bool vector as wide as the \
                         values",
                        cond.0
                    ));
                }
                Some(Ty::Id(a))
            }
            Expr::Call { func, args } => self.call(*func, args),
            Expr::CallImport { import, args } => {
                let Some(import) = self.module.import(*import) else {
                    return self.err(format!("import {} doesn't exist", import.0));
                };
                if args.len() != import.params.len() {
                    return self.err(format!(
                        "`{}.{}` takes {} arguments",
                        import.module,
                        import.name,
                        import.params.len()
                    ));
                }
                for (arg, &param) in args.iter().zip(&import.params) {
                    if let Some(ty) = self.value(*arg)
                        && ty != param
                    {
                        self.errors.push(format!(
                            "argument %{} to an import has the wrong type",
                            arg.0
                        ));
                    }
                }
                Some(Ty::Id(import.ret))
            }
            Expr::Variant {
                ty,
                variant,
                fields,
            } => {
                let Some(Type::Enum(e)) = self.types.get(*ty) else {
                    return self.err(format!("{} isn't an enum", self.types.name(*ty)));
                };
                let Some(v) = e.variants.get(*variant as usize) else {
                    return self.err(format!("{} has no variant {variant}", e.name));
                };
                self.expect_values(fields, &v.fields, "a variant field");
                Some(Ty::Id(*ty))
            }
            Expr::Discriminant(v) => match self.value_type(*v)? {
                Type::Enum(_) => Some(Ty::New(Type::Scalar(Scalar::U32))),
                _ => self.err(format!("%{} isn't an enum", v.0)),
            },
            Expr::VariantField {
                value,
                variant,
                index,
            } => {
                let Type::Enum(e) = self.value_type(*value)? else {
                    return self.err(format!("%{} isn't an enum", value.0));
                };
                let field = e
                    .variants
                    .get(*variant as usize)
                    .and_then(|v| v.fields.get(*index as usize));
                match field {
                    Some(&ty) => Some(Ty::Id(ty)),
                    None => self.err(format!("{} has no variant field {variant}.{index}", e.name)),
                }
            }
            Expr::Len(place) => {
                let ty = self.place(place, false)?;
                match self.resolve(&ty) {
                    Some(Type::Slice { .. } | Type::Array { .. }) => {
                        Some(Ty::New(Type::Scalar(Scalar::U32)))
                    }
                    _ => self.err("`len` of something that isn't a slice or array".to_string()),
                }
            }
            Expr::Atomic { op, place, args } => self.atomic(*op, place, args),
            Expr::Derived { func, args, .. } => {
                let Some(callee) = self.module.function(*func) else {
                    return self.err(format!("function {} doesn't exist", func.0));
                };
                if args.len() != callee.params.len() {
                    return self.err(format!(
                        "a derived interpretation of `{}` needs {} arguments",
                        callee.name,
                        callee.params.len()
                    ));
                }
                for arg in args {
                    self.value(*arg);
                }
                Some(Ty::Any)
            }
        }
    }

    /// Checks that each value has the matching type.
    fn expect_values(&mut self, values: &[ValueId], types: &[TypeId], what: &str) {
        if values.len() != types.len() {
            return self.errors.push(format!(
                "{} values for {} {what}s",
                values.len(),
                types.len()
            ));
        }
        for (&v, &ty) in values.iter().zip(types) {
            if let Some(actual) = self.value(v)
                && actual != ty
            {
                let message = format!(
                    "{what} must be a {}, but %{} is a {}",
                    self.types.name(ty),
                    v.0,
                    self.types.name(actual)
                );
                self.errors.push(message);
            }
        }
    }

    fn binary(&mut self, op: BinaryOp, a: ValueId, b: ValueId) -> Option<Ty> {
        let ta = self.value(a)?;
        let tb = self.value(b)?;
        let (Some(x), Some(y)) = (self.types.get(ta).cloned(), self.types.get(tb).cloned()) else {
            return None;
        };
        let bool_shape = |n: Option<VectorSize>| match n {
            None => Type::Scalar(Scalar::Bool),
            Some(size) => Type::Vector {
                size,
                scalar: Scalar::Bool,
            },
        };
        let result = match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => {
                match (shape(&x), shape(&y)) {
                    (Some((s, n)), Some((t, m))) if s == t && s.is_numeric() => {
                        if n == m || m.is_none() {
                            Some(Ty::Id(ta))
                        } else if n.is_none() {
                            Some(Ty::Id(tb))
                        } else {
                            None
                        }
                    }
                    _ => matrix_arith(op, &x, &y, ta, tb),
                }
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => match shape(&x) {
                Some((s, _))
                    if ta == tb
                        && (s.is_int() || (s == Scalar::Bool && op != BinaryOp::BitXor)) =>
                {
                    Some(Ty::Id(ta))
                }
                _ => None,
            },
            BinaryOp::Shl | BinaryOp::Shr => match (shape(&x), shape(&y)) {
                (Some((s, n)), Some((Scalar::U32, m))) if s.is_int() && n == m => Some(Ty::Id(ta)),
                _ => None,
            },
            BinaryOp::Eq | BinaryOp::Ne => match shape(&x) {
                Some((_, n)) if ta == tb => Some(Ty::New(bool_shape(n))),
                _ => None,
            },
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => match shape(&x) {
                Some((s, n)) if ta == tb && s.is_numeric() => Some(Ty::New(bool_shape(n))),
                _ => None,
            },
            BinaryOp::LogicalAnd | BinaryOp::LogicalOr => match shape(&x) {
                Some((Scalar::Bool, _)) if ta == tb => Some(Ty::Id(ta)),
                _ => None,
            },
        };
        match result {
            Some(t) => Some(t),
            None => self.err(format!(
                "{op:?} doesn't apply to a {} and a {}",
                self.types.name(ta),
                self.types.name(tb)
            )),
        }
    }

    fn construct(&mut self, ty: TypeId, parts: &[ValueId]) -> Option<Ty> {
        let Some(target) = self.types.get(ty).cloned() else {
            return self.err(format!("construct of type {ty:?}, which doesn't exist"));
        };
        match &target {
            Type::Struct(s) => {
                let fields: Vec<TypeId> = s.fields.iter().map(|f| f.ty).collect();
                self.expect_values(parts, &fields, "a field");
            }
            Type::Tuple(elements) => self.expect_values(parts, elements, "an element"),
            Type::Array { element, len } => {
                // Check the count first: the length may be huge in a malformed module.
                if usize::try_from(*len).ok() != Some(parts.len()) {
                    return self.err(format!(
                        "a {} needs {len} elements, not {}",
                        self.types.name(ty),
                        parts.len()
                    ));
                }
                let elements = vec![*element; parts.len()];
                self.expect_values(parts, &elements, "an element");
            }
            Type::Matrix { columns, rows } => {
                let column = Type::Vector {
                    size: *rows,
                    scalar: Scalar::F32,
                };
                if parts.len() != columns.count() as usize {
                    return self.err(format!(
                        "a {} needs {} columns",
                        self.types.name(ty),
                        columns.count()
                    ));
                }
                for &p in parts {
                    self.expect(p, &column, "a matrix column");
                }
            }
            Type::Vector { size, scalar } => {
                let mut count = 0;
                for &p in parts {
                    match self.value_shape(p) {
                        Some((s, n)) if s == *scalar => count += n.map_or(1, VectorSize::count),
                        Some(_) => {
                            return self.err(format!("%{} doesn't hold {}s", p.0, scalar.name()));
                        }
                        None => return None,
                    }
                }
                if parts.len() < 2 || count != size.count() {
                    return self.err(format!(
                        "a {} is built from at least two parts with {} components in all, not {count}",
                        self.types.name(ty),
                        size.count()
                    ));
                }
            }
            _ => return self.err(format!("can't construct a {}", self.types.name(ty))),
        }
        Some(Ty::Id(ty))
    }

    fn call(&mut self, func: FuncId, args: &[Arg]) -> Option<Ty> {
        let Some(callee) = self.module.function(func) else {
            return self.err(format!("function {} doesn't exist", func.0));
        };
        if args.len() != callee.params.len() {
            return self.err(format!(
                "`{}` takes {} arguments, not {}",
                callee.name,
                callee.params.len(),
                args.len()
            ));
        }
        for (i, (arg, param)) in args.iter().zip(&callee.params).enumerate() {
            let param_ty = self.types.get(param.ty).cloned();
            let slice = match &param_ty {
                Some(Type::Slice { element }) => Some(*element),
                _ => None,
            };
            let fits = match (param.mode, arg, slice) {
                (ParamMode::In, Arg::Value(v), None) => self.value(*v).map(|t| t == param.ty),
                (ParamMode::In, Arg::Place(p), Some(element)) => self
                    .place(p, false)
                    .map(|t| self.passes_as_slice(&t, element)),
                (ParamMode::InOut, Arg::Place(p), slice) => {
                    self.place(p, true).map(|t| match slice {
                        Some(element) => self.passes_as_slice(&t, element),
                        None => self.matches(param.ty, &t),
                    })
                }
                _ => {
                    self.errors.push(format!(
                        "argument {i} of `{}` must be a {}",
                        callee.name,
                        if param.mode == ParamMode::InOut || slice.is_some() {
                            "place"
                        } else {
                            "value"
                        }
                    ));
                    continue;
                }
            };
            if fits == Some(false) {
                self.errors.push(format!(
                    "argument {i} of `{}` isn't a {}",
                    callee.name,
                    self.types.name(param.ty)
                ));
            }
        }
        Some(Ty::Id(callee.ret))
    }

    /// Whether a place of type `ty` can be passed as a slice of `element`: a slice or an array of
    /// it.
    fn passes_as_slice(&self, ty: &Ty, element: TypeId) -> bool {
        matches!(
            self.resolve(ty),
            Some(Type::Slice { element: e } | Type::Array { element: e, .. }) if e == element
        )
    }

    fn math(&mut self, op: MathOp, args: &[ValueId]) -> Option<Ty> {
        use MathOp::{
            Abs, Acos, Asin, Atan, Atan2, Ceil, Clamp, Cos, Cosh, Cross, Determinant, Distance,
            Dot, Dpdx, Dpdy, Exp, Exp2, Floor, Fract, Fwidth, InverseSqrt, Length, Log, Log2, Max,
            Min, Mix, Normalize, Pow, Reflect, Round, Sign, Sin, Sinh, SmoothStep, Sqrt, Step, Tan,
            Tanh, Transpose, Trunc,
        };
        let arity = match op {
            Clamp | Mix | SmoothStep => 3,
            Min | Max | Pow | Atan2 | Step | Distance | Dot | Cross | Reflect => 2,
            _ => 1,
        };
        if args.len() != arity {
            return self.err(format!("{} takes {arity} arguments", op.name()));
        }
        let first = self.value(args[0])?;
        let first_ty = self.types.get(first)?.clone();
        let rest: Option<Vec<TypeId>> = args[1..].iter().map(|&a| self.value(a)).collect();
        let rest = rest?;
        let same = rest.iter().all(|&t| t == first);
        let float = shape(&first_ty).is_some_and(|(s, _)| s.is_float());
        let numeric = shape(&first_ty).is_some_and(|(s, _)| s.is_numeric());
        let scalar_of = |t: &Type| shape(t).map(|(s, _)| Ty::New(Type::Scalar(s)));
        let result = match op {
            Floor | Ceil | Round | Trunc | Fract | Sqrt | InverseSqrt | Exp | Exp2 | Log | Log2
            | Sin | Cos | Tan | Asin | Acos | Atan | Sinh | Cosh | Tanh | Dpdx | Dpdy | Fwidth
                if float =>
            {
                Some(Ty::Id(first))
            }
            Abs if numeric => Some(Ty::Id(first)),
            Sign if shape(&first_ty).is_some_and(|(s, _)| s.is_signed()) => Some(Ty::Id(first)),
            Min | Max | Clamp if numeric && same => Some(Ty::Id(first)),
            Pow | Atan2 | Step | SmoothStep if float && same => Some(Ty::Id(first)),
            Mix if float => {
                let blend_scalar = rest.get(1).is_some_and(|&t| {
                    shape(&first_ty).is_some_and(|(s, _)| self.is(t, |b| *b == Type::Scalar(s)))
                });
                (rest.first() == Some(&first) && (rest.get(1) == Some(&first) || blend_scalar))
                    .then_some(Ty::Id(first))
            }
            Length if float => scalar_of(&first_ty),
            Distance if float && same => scalar_of(&first_ty),
            Dot if numeric && same && matches!(first_ty, Type::Vector { .. }) => {
                scalar_of(&first_ty)
            }
            Cross
                if same
                    && first_ty
                        == (Type::Vector {
                            size: VectorSize::Three,
                            scalar: Scalar::F32,
                        }) =>
            {
                Some(Ty::Id(first))
            }
            Normalize | Reflect if float && same && matches!(first_ty, Type::Vector { .. }) => {
                Some(Ty::Id(first))
            }
            Transpose => match first_ty {
                Type::Matrix { columns, rows } => Some(Ty::New(Type::Matrix {
                    columns: rows,
                    rows: columns,
                })),
                _ => None,
            },
            Determinant => match first_ty {
                Type::Matrix { columns, rows } if columns == rows => {
                    Some(Ty::New(Type::Scalar(Scalar::F32)))
                }
                _ => None,
            },
            _ => None,
        };
        match result {
            Some(t) => Some(t),
            None => self.err(format!(
                "{} doesn't apply to these arguments (the first is a {})",
                op.name(),
                self.types.name(first)
            )),
        }
    }

    fn atomic(&mut self, op: AtomicOp, place: &Place, args: &[ValueId]) -> Option<Ty> {
        let ty = self.place(place, op != AtomicOp::Load)?;
        let Some(Type::Atomic(scalar)) = self.resolve(&ty) else {
            return self.err("an atomic operation on a place that isn't atomic".to_string());
        };
        let operands = match op {
            AtomicOp::Load => 0,
            AtomicOp::CompareExchange => 2,
            _ => 1,
        };
        if args.len() != operands {
            return self.err(format!("atomic {op:?} takes {operands} operands"));
        }
        for &a in args {
            self.expect(a, &Type::Scalar(scalar), "an atomic operand");
        }
        match op {
            AtomicOp::Store => Some(Ty::New(Type::Unit)),
            AtomicOp::CompareExchange => {
                let old = self.types.lookup(&Type::Scalar(scalar));
                let exchanged = self.types.lookup(&Type::Scalar(Scalar::Bool));
                match (old, exchanged) {
                    (Some(old), Some(exchanged)) => {
                        Some(Ty::New(Type::Tuple(vec![old, exchanged])))
                    }
                    _ => self.err(
                        "compare-exchange gives a tuple whose types aren't in the module"
                            .to_string(),
                    ),
                }
            }
            _ => Some(Ty::New(Type::Scalar(scalar))),
        }
    }
}

/// Matrix arithmetic, as WGSL has it: `+` and `-` of equal matrices, and `*` of a matrix by a
/// matrix, a vector or an `f32`.
fn matrix_arith(op: BinaryOp, x: &Type, y: &Type, ta: TypeId, tb: TypeId) -> Option<Ty> {
    let f32 = Type::Scalar(Scalar::F32);
    match (op, x, y) {
        (BinaryOp::Add | BinaryOp::Sub, Type::Matrix { .. }, _) if ta == tb => Some(Ty::Id(ta)),
        (BinaryOp::Mul, Type::Matrix { .. }, s) if *s == f32 => Some(Ty::Id(ta)),
        (BinaryOp::Mul, s, Type::Matrix { .. }) if *s == f32 => Some(Ty::Id(tb)),
        (
            BinaryOp::Mul,
            Type::Matrix { columns, rows },
            Type::Vector {
                size,
                scalar: Scalar::F32,
            },
        ) if columns == size => Some(Ty::New(Type::Vector {
            size: *rows,
            scalar: Scalar::F32,
        })),
        (
            BinaryOp::Mul,
            Type::Vector {
                size,
                scalar: Scalar::F32,
            },
            Type::Matrix { columns, rows },
        ) if rows == size => Some(Ty::New(Type::Vector {
            size: *columns,
            scalar: Scalar::F32,
        })),
        (
            BinaryOp::Mul,
            Type::Matrix {
                columns: k,
                rows: r,
            },
            Type::Matrix {
                columns: c,
                rows: k2,
            },
        ) if k == k2 => Some(Ty::New(Type::Matrix {
            columns: *c,
            rows: *r,
        })),
        _ => None,
    }
}
