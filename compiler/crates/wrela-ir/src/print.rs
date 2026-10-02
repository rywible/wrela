//! The text form of a module, for golden tests and debugging. It's deterministic: everything is
//! printed in id order, and floats print as the shortest text that reads back to the same bits.
//! Values are `%N`, parameters `pN` and locals `lN`; declarations give their names.

use std::fmt::{self, Write};

use crate::module::{
    Arg, BinaryOp, Block, Const, EntryParam, EntryPoint, Expr, Function, Interpolation,
    Interpretation, Io, IoBinding, Module, ParamMode, PipelineKind, Place, PlaceRoot, Projection,
    Record, ResourceKind, Stage, Stmt, UnaryOp,
};
use crate::types::Type;
use crate::{FuncId, ValueId};

impl fmt::Display for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut p = Printer {
            module: self,
            out: f,
            indent: 0,
        };
        p.module()
    }
}

struct Printer<'a, 'f, 'w> {
    module: &'a Module,
    out: &'f mut fmt::Formatter<'w>,
    indent: usize,
}

struct V(ValueId);

impl fmt::Display for V {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%{}", self.0.0)
    }
}

impl Printer<'_, '_, '_> {
    fn line(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
        for _ in 0..self.indent {
            self.out.write_str("    ")?;
        }
        self.out.write_fmt(args)?;
        self.out.write_char('\n')
    }

    fn ty(&self, id: crate::TypeId) -> String {
        self.module.types.name(id).to_string()
    }

    fn func_name(&self, id: FuncId) -> String {
        match self.module.function(id) {
            Some(f) => f.name.clone(),
            None => format!("?f{}", id.0),
        }
    }

    fn module(&mut self) -> fmt::Result {
        let mut first = true;
        let mut gap = |p: &mut Self| -> fmt::Result {
            if !std::mem::take(&mut first) {
                p.out.write_char('\n')?;
            }
            Ok(())
        };
        for (_, ty) in self.module.types.iter() {
            match ty {
                Type::Struct(s) => {
                    gap(self)?;
                    let layout = match &s.gpu_layout {
                        Some(l) => format!(" gpu(size {}, align {})", l.size, l.align),
                        None => String::new(),
                    };
                    self.line(format_args!("struct {}{layout} {{", s.name))?;
                    for (i, field) in s.fields.iter().enumerate() {
                        let offset = s
                            .gpu_layout
                            .as_ref()
                            .and_then(|l| l.offsets.get(i))
                            .map(|o| format!(" @ {o}"))
                            .unwrap_or_default();
                        let ty = self.ty(field.ty);
                        self.line(format_args!("    {}: {ty}{offset}", field.name))?;
                    }
                    self.line(format_args!("}}"))?;
                }
                Type::Enum(e) => {
                    gap(self)?;
                    self.line(format_args!("enum {} {{", e.name))?;
                    for v in &e.variants {
                        let fields: Vec<String> = v.fields.iter().map(|&t| self.ty(t)).collect();
                        if fields.is_empty() {
                            self.line(format_args!("    {}", v.name))?;
                        } else {
                            self.line(format_args!("    {}({})", v.name, fields.join(", ")))?;
                        }
                    }
                    self.line(format_args!("}}"))?;
                }
                _ => {}
            }
        }
        for (i, import) in self.module.imports.iter().enumerate() {
            gap(self)?;
            let params: Vec<String> = import.params.iter().map(|&t| self.ty(t)).collect();
            let ret = self.ty(import.ret);
            self.line(format_args!(
                "import {i}: {}.{}({}) -> {ret}",
                import.module,
                import.name,
                params.join(", ")
            ))?;
        }
        for function in &self.module.functions {
            gap(self)?;
            self.function(function)?;
        }
        if !self.module.entry_points.is_empty() {
            gap(self)?;
        }
        for (i, entry) in self.module.entry_points.iter().enumerate() {
            self.entry_point(i, entry)?;
        }
        if !(self.module.pipelines.is_empty() && self.module.exports.is_empty()) {
            gap(self)?;
        }
        for (i, pipeline) in self.module.pipelines.iter().enumerate() {
            let kind = match pipeline.kind {
                PipelineKind::Render { vertex, fragment } => {
                    format!(
                        "render(vertex entry {}, fragment entry {})",
                        vertex.0, fragment.0
                    )
                }
                PipelineKind::Compute { entry } => format!("compute(entry {})", entry.0),
            };
            self.line(format_args!("pipeline {i}: {kind}"))?;
        }
        for export in &self.module.exports {
            let name = self.func_name(export.function);
            self.line(format_args!("export {:?} = {name}", export.name))?;
        }
        Ok(())
    }

    fn function(&mut self, function: &Function) -> fmt::Result {
        let params: Vec<String> = function
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mode = match p.mode {
                    ParamMode::In => "",
                    ParamMode::InOut => "inout ",
                };
                format!("p{i} {}: {mode}{}", p.name, self.ty(p.ty))
            })
            .collect();
        let ret = self.ty(function.ret);
        self.line(format_args!(
            "fn {}({}) -> {ret} {{",
            function.name,
            params.join(", ")
        ))?;
        self.indent += 1;
        for (i, local) in function.locals.iter().enumerate() {
            let ty = self.ty(local.ty);
            self.line(format_args!("local l{i} {}: {ty}", local.name))?;
        }
        self.stmts(function, &function.body)?;
        self.indent -= 1;
        self.line(format_args!("}}"))
    }

    fn stmts(&mut self, function: &Function, block: &Block) -> fmt::Result {
        for stmt in &block.0 {
            self.stmt(function, stmt)?;
        }
        Ok(())
    }

    fn nested(&mut self, function: &Function, block: &Block) -> fmt::Result {
        self.indent += 1;
        self.stmts(function, block)?;
        self.indent -= 1;
        Ok(())
    }

    fn stmt(&mut self, function: &Function, stmt: &Stmt) -> fmt::Result {
        match stmt {
            Stmt::Let(v, expr) => {
                let ty = match function.values.get(v.index()) {
                    Some(&t) => self.ty(t),
                    None => "?".to_string(),
                };
                let expr = self.expr(expr);
                self.line(format_args!("{}: {ty} = {expr}", V(*v)))
            }
            Stmt::Store(place, v) => {
                let place = place_text(place);
                self.line(format_args!("store {place}, {}", V(*v)))
            }
            Stmt::If {
                cond,
                then,
                otherwise,
            } => {
                self.line(format_args!("if {} {{", V(*cond)))?;
                self.nested(function, then)?;
                if !otherwise.0.is_empty() {
                    self.line(format_args!("}} else {{"))?;
                    self.nested(function, otherwise)?;
                }
                self.line(format_args!("}}"))
            }
            Stmt::Loop { body, continuing } => {
                self.line(format_args!("loop {{"))?;
                self.nested(function, body)?;
                if !continuing.0.is_empty() {
                    self.line(format_args!("}} continuing {{"))?;
                    self.nested(function, continuing)?;
                }
                self.line(format_args!("}}"))
            }
            Stmt::Break => self.line(format_args!("break")),
            Stmt::Continue => self.line(format_args!("continue")),
            Stmt::Return(None) => self.line(format_args!("return")),
            Stmt::Return(Some(v)) => self.line(format_args!("return {}", V(*v))),
            Stmt::Record(Record::BeginScreenPass { clear }) => self.line(format_args!(
                "record begin_screen_pass(clear {})",
                V(*clear)
            )),
            Stmt::Record(Record::Draw {
                pipeline,
                vertex_count,
                instance_count,
                uniforms,
            }) => {
                let uniforms = match uniforms {
                    Some(u) => format!(", uniforms {}", V(*u)),
                    None => String::new(),
                };
                self.line(format_args!(
                    "record draw(pipeline {}, vertices {}, instances {}{uniforms})",
                    pipeline.0,
                    V(*vertex_count),
                    V(*instance_count)
                ))
            }
            Stmt::Record(Record::Present) => self.line(format_args!("record present")),
            Stmt::Trap => self.line(format_args!("trap")),
        }
    }

    fn expr(&self, expr: &Expr) -> String {
        let list = |values: &[ValueId]| -> String {
            values
                .iter()
                .map(|&v| V(v).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        match expr {
            Expr::Const(c) => format!("const {}", const_text(*c)),
            Expr::Unary(op, v) => {
                let op = match op {
                    UnaryOp::Neg => "neg",
                    UnaryOp::Not => "not",
                    UnaryOp::BitNot => "bit_not",
                };
                format!("{op} {}", V(*v))
            }
            Expr::Binary(op, a, b) => format!("{} {}, {}", binary_name(*op), V(*a), V(*b)),
            Expr::Load(place) => format!("load {}", place_text(place)),
            Expr::Construct { ty, parts } => format!("construct {}({})", self.ty(*ty), list(parts)),
            Expr::Extract { value, index } => format!("extract {}.{index}", V(*value)),
            Expr::Index { value, index } => format!("index {}[{}]", V(*value), V(*index)),
            Expr::Swizzle { value, components } => {
                let letters: String = components
                    .iter()
                    .map(|&c| match c {
                        0 => 'x',
                        1 => 'y',
                        2 => 'z',
                        3 => 'w',
                        _ => '?',
                    })
                    .collect();
                format!("swizzle {}.{letters}", V(*value))
            }
            Expr::Splat { ty, value } => format!("splat {}({})", self.ty(*ty), V(*value)),
            Expr::Convert { ty, value } => format!("convert {}({})", self.ty(*ty), V(*value)),
            Expr::Bitcast { ty, value } => format!("bitcast {}({})", self.ty(*ty), V(*value)),
            Expr::Math(op, args) => format!("math {}({})", op.name(), list(args)),
            Expr::Select {
                cond,
                accept,
                reject,
            } => format!("select {}, {}, {}", V(*cond), V(*accept), V(*reject)),
            Expr::Call { func, args } => {
                let args: Vec<String> = args
                    .iter()
                    .map(|a| match a {
                        Arg::Value(v) => V(*v).to_string(),
                        Arg::Place(p) => format!("&{}", place_text(p)),
                    })
                    .collect();
                format!("call {}({})", self.func_name(*func), args.join(", "))
            }
            Expr::CallImport { import, args } => match self.module.import(*import) {
                Some(i) => format!("call_import {}.{}({})", i.module, i.name, list(args)),
                None => format!("call_import ?i{}({})", import.0, list(args)),
            },
            Expr::Variant {
                ty,
                variant,
                fields,
            } => format!("variant {}.{variant}({})", self.ty(*ty), list(fields)),
            Expr::Discriminant(v) => format!("discriminant {}", V(*v)),
            Expr::VariantField {
                value,
                variant,
                index,
            } => format!("variant_field {}.{variant}.{index}", V(*value)),
            Expr::Len(place) => format!("len {}", place_text(place)),
            Expr::Atomic { op, place, args } => {
                let op = format!("{op:?}").to_lowercase();
                if args.is_empty() {
                    format!("atomic {op} {}", place_text(place))
                } else {
                    format!("atomic {op} {}, {}", place_text(place), list(args))
                }
            }
            Expr::Derived {
                interpretation,
                func,
                args,
            } => {
                let kind = match interpretation {
                    Interpretation::Gradient => "gradient",
                    Interpretation::Interval => "interval",
                };
                format!("derived {kind} {}({})", self.func_name(*func), list(args))
            }
        }
    }

    fn entry_point(&mut self, index: usize, entry: &EntryPoint) -> fmt::Result {
        let stage = match entry.stage {
            Stage::Vertex => "vertex".to_string(),
            Stage::Fragment => "fragment".to_string(),
            Stage::Compute {
                workgroup_size: [x, y, z],
            } => format!("compute({x}, {y}, {z})"),
        };
        let params: Vec<String> = entry
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let source = match p {
                    EntryParam::Io(io) => io_text(io),
                    EntryParam::Resource {
                        resource,
                        member: None,
                    } => format!("resource {resource}"),
                    EntryParam::Resource {
                        resource,
                        member: Some(m),
                    } => format!("resource {resource}.{m}"),
                };
                format!("p{i}: {source}")
            })
            .collect();
        let result = match &entry.result {
            Some(io) => format!(" -> {}", io_text(io)),
            None => String::new(),
        };
        let name = self.func_name(entry.function);
        self.line(format_args!(
            "entry {index}: {stage} {name}({}){result}",
            params.join(", ")
        ))?;
        for (i, r) in entry.resources.iter().enumerate() {
            let kind = match r.kind {
                ResourceKind::Uniform => "uniform",
                ResourceKind::StorageRead => "storage_read",
                ResourceKind::StorageReadWrite => "storage_read_write",
            };
            let ty = self.ty(r.ty);
            self.line(format_args!(
                "    resource {i}: group {}, binding {}, {kind} {ty}",
                r.group, r.binding
            ))?;
        }
        Ok(())
    }
}

fn binary_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Mul => "mul",
        BinaryOp::Div => "div",
        BinaryOp::Rem => "rem",
        BinaryOp::BitAnd => "bit_and",
        BinaryOp::BitOr => "bit_or",
        BinaryOp::BitXor => "bit_xor",
        BinaryOp::Shl => "shl",
        BinaryOp::Shr => "shr",
        BinaryOp::Eq => "eq",
        BinaryOp::Ne => "ne",
        BinaryOp::Lt => "lt",
        BinaryOp::Le => "le",
        BinaryOp::Gt => "gt",
        BinaryOp::Ge => "ge",
        BinaryOp::LogicalAnd => "and",
        BinaryOp::LogicalOr => "or",
    }
}

fn place_text(place: &Place) -> String {
    let mut text = match place.root {
        PlaceRoot::Local(l) => format!("l{}", l.0),
        PlaceRoot::Param(i) => format!("p{i}"),
    };
    for projection in &place.projections {
        match projection {
            Projection::Field(i) => {
                let _ = write!(text, ".{i}");
            }
            Projection::Index(v) => {
                let _ = write!(text, "[{}]", V(*v));
            }
        }
    }
    text
}

fn io_text(io: &Io) -> String {
    let binding = |b: &IoBinding| match *b {
        IoBinding::Builtin(builtin) => format!("builtin {}", builtin.name()),
        IoBinding::Location {
            location,
            interpolation: Interpolation::Perspective,
        } => format!("location {location}"),
        IoBinding::Location {
            location,
            interpolation: Interpolation::Flat,
        } => format!("location {location} flat"),
    };
    match io {
        Io::Binding(b) => binding(b),
        Io::Members(bindings) => {
            let members: Vec<String> = bindings.iter().map(binding).collect();
            format!("{{{}}}", members.join(", "))
        }
    }
}

/// A constant as `<type> <value>`. Floats use Rust's shortest round-trip form; a NaN shows its
/// bits, since NaNs differ only there.
fn const_text(c: Const) -> String {
    match c {
        Const::Bool(b) => format!("bool {b}"),
        Const::I8(n) => format!("i8 {n}"),
        Const::U8(n) => format!("u8 {n}"),
        Const::I16(n) => format!("i16 {n}"),
        Const::U16(n) => format!("u16 {n}"),
        Const::I32(n) => format!("i32 {n}"),
        Const::U32(n) => format!("u32 {n}"),
        Const::I64(n) => format!("i64 {n}"),
        Const::U64(n) => format!("u64 {n}"),
        Const::F32 { bits } => {
            let x = f32::from_bits(bits);
            if x.is_nan() {
                format!("f32 nan({bits:#010x})")
            } else {
                format!("f32 {x:?}")
            }
        }
        Const::F64 { bits } => {
            let x = f64::from_bits(bits);
            if x.is_nan() {
                format!("f64 nan({bits:#018x})")
            } else {
                format!("f64 {x:?}")
            }
        }
    }
}
