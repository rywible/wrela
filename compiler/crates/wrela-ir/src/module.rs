//! The IR's data structures: a [`Module`] of [`Function`]s in structured form, plus the GPU entry
//! points and pipelines built from some of them and the CPU exports and host imports.
//!
//! The shape follows from the two targets. WGSL and WASM both want structured control flow, so
//! there's no CFG: `If` and `Loop` nest, as in naga. Each value is defined once by a [`Stmt::Let`]
//! and is visible to the statements after it in its block and in blocks nested there, so values
//! stay SSA without phi nodes; what flows out of a branch or around a loop goes through a local.
//! Every type is concrete: generics are monomorphized, closures inlined, and functions that return
//! projections inlined, before the IR. So there are no references here, only values and places.

use crate::{
    EntryId, FuncId, ImportId, LocalId, PipelineId, TypeId, ValueId,
    types::{Scalar, Types},
};

/// One program: everything the back ends and the interpreter need.
#[derive(Clone, Debug, Default)]
pub struct Module {
    pub types: Types,
    /// Indexed by [`FuncId`]. Names are unique (the lowering mangles instances, as in
    /// `lerp<vec3>`), so printed IR can refer to functions by name.
    pub functions: Vec<Function>,
    /// Host functions, imported by WASM. Indexed by [`ImportId`].
    pub imports: Vec<Import>,
    /// GPU entry points. Indexed by [`EntryId`].
    pub entry_points: Vec<EntryPoint>,
    /// Indexed by [`PipelineId`], which is also the pipeline's id in the manifest and in `DRAW`.
    pub pipelines: Vec<Pipeline>,
    /// CPU functions the WASM module exports, in export order.
    pub exports: Vec<Export>,
}

fn next_id(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

impl Module {
    pub fn new() -> Self {
        Module::default()
    }

    pub fn add_function(&mut self, function: Function) -> FuncId {
        let id = FuncId(next_id(self.functions.len()));
        self.functions.push(function);
        id
    }

    pub fn add_import(&mut self, import: Import) -> ImportId {
        let id = ImportId(next_id(self.imports.len()));
        self.imports.push(import);
        id
    }

    pub fn add_entry_point(&mut self, entry: EntryPoint) -> EntryId {
        let id = EntryId(next_id(self.entry_points.len()));
        self.entry_points.push(entry);
        id
    }

    pub fn add_pipeline(&mut self, pipeline: Pipeline) -> PipelineId {
        let id = PipelineId(next_id(self.pipelines.len()));
        self.pipelines.push(pipeline);
        id
    }

    pub fn add_export(&mut self, export: Export) {
        self.exports.push(export);
    }

    pub fn function(&self, id: FuncId) -> Option<&Function> {
        self.functions.get(id.index())
    }

    pub fn import(&self, id: ImportId) -> Option<&Import> {
        self.imports.get(id.index())
    }

    pub fn entry_point(&self, id: EntryId) -> Option<&EntryPoint> {
        self.entry_points.get(id.index())
    }

    pub fn pipeline(&self, id: PipelineId) -> Option<&Pipeline> {
        self.pipelines.get(id.index())
    }

    /// The resource whose bytes a `DRAW` of `pipeline` carries inline: the uniform at group 0,
    /// binding 0 of either of its entry points, if one has it.
    pub fn inline_uniform(&self, pipeline: PipelineId) -> Option<&Resource> {
        let PipelineKind::Render { vertex, fragment } = self.pipeline(pipeline)?.kind else {
            return None;
        };
        [vertex, fragment]
            .into_iter()
            .filter_map(|e| self.entry_point(e))
            .flat_map(|e| &e.resources)
            .find(|r| r.group == 0 && r.binding == 0 && r.kind == ResourceKind::Uniform)
    }
}

/// A function: parameters, locals, values and a structured body.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: TypeId,
    /// Mutable storage: `var` bindings, and the temporaries that carry values out of branches
    /// and around loops. Indexed by [`LocalId`].
    pub locals: Vec<Local>,
    /// The type of every value, indexed by [`ValueId`]. Each is defined by exactly one
    /// [`Stmt::Let`] in `body`.
    pub values: Vec<TypeId>,
    pub body: Block,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Param {
    pub name: String,
    pub ty: TypeId,
    pub mode: ParamMode,
}

/// How a parameter is passed. wrela's `borrow` and `take` are both `In`: the difference is
/// ownership, which the checker has already enforced. `mut` is `InOut`: the callee can store
/// through it. On the CPU that's a pointer; on the GPU, copy-in copy-out or inlining, which
/// exclusivity makes sound.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ParamMode {
    In,
    InOut,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Local {
    pub name: String,
    pub ty: TypeId,
}

/// A sequence of statements; also a scope for the values defined in it.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Block(pub Vec<Stmt>);

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Stmt {
    /// Defines a value. The value's type is `Function::values[value]`.
    Let(ValueId, Expr),
    /// Writes a value into a place rooted at a local or an `InOut` parameter.
    Store(Place, ValueId),
    /// `cond` is a `bool`.
    If {
        cond: ValueId,
        then: Block,
        otherwise: Block,
    },
    /// Repeats `body` until a `Break`. `continuing` runs after each pass through `body`, including
    /// one ended by `Continue`, and sees only the values visible at the loop itself; it can't
    /// `Break`, `Continue` or `Return`. This is WGSL's loop, which `while` and `for` lower to.
    Loop { body: Block, continuing: Block },
    /// Leaves the innermost loop.
    Break,
    /// Goes to the innermost loop's `continuing` block.
    Continue,
    /// `None` exactly when the function returns `()`.
    Return(Option<ValueId>),
    /// Records a GPU command; CPU code only.
    Record(Record),
    /// Stops the program: a failed runtime check, such as a stale handle. CPU code only.
    Trap,
}

/// Somewhere a value lives: a local or a parameter, then a path into it.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Place {
    pub root: PlaceRoot,
    pub projections: Vec<Projection>,
}

impl Place {
    pub fn local(local: LocalId) -> Place {
        Place {
            root: PlaceRoot::Local(local),
            projections: Vec::new(),
        }
    }

    pub fn param(index: u32) -> Place {
        Place {
            root: PlaceRoot::Param(index),
            projections: Vec::new(),
        }
    }

    pub fn field(mut self, index: u32) -> Place {
        self.projections.push(Projection::Field(index));
        self
    }

    pub fn index(mut self, index: ValueId) -> Place {
        self.projections.push(Projection::Index(index));
        self
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PlaceRoot {
    Local(LocalId),
    /// The parameter at this position.
    Param(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Projection {
    /// A struct field or tuple element by position; also a vector's component or a matrix's
    /// column, by a constant index.
    Field(u32),
    /// An element of an array, slice, vector or matrix, by an `i32` or `u32` value. Out of
    /// bounds traps on the CPU and is clamped on the GPU (language.md §11). WGSL rejects a
    /// constant index out of bounds outright, so the GPU back end computes one that may be at
    /// run time; the checker reports the ones it knows (E0316).
    Index(ValueId),
}

/// An expression, always the right-hand side of a [`Stmt::Let`]. Its operands are earlier values.
/// Everything is pure except `Call`, `CallImport` and `Atomic`, which may write places and talk to
/// the host. Arithmetic follows language.md §11 on the CPU (integer overflow and division by zero
/// trap) and WGSL on the GPU.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Expr {
    Const(Const),
    Unary(UnaryOp, ValueId),
    Binary(BinaryOp, ValueId, ValueId),
    /// Reads a place. Reading a parameter, including an entry point's builtin or resource, is a
    /// `Load` of `Place::param(i)`.
    Load(Place),
    /// Builds a value of `ty` from `parts`: a struct's fields or a tuple's or array's elements,
    /// in order; a matrix's columns; or a vector's components, as scalars and vectors of its
    /// scalar type whose sizes add up to its size, at least two parts (`vec4(xy, z, 1.0)`).
    Construct {
        ty: TypeId,
        parts: Vec<ValueId>,
    },
    /// A struct field, tuple element, vector component, matrix column or array element, by a
    /// constant position.
    Extract {
        value: ValueId,
        index: u32,
    },
    /// An array, vector or matrix element by an `i32` or `u32` value.
    Index {
        value: ValueId,
        index: ValueId,
    },
    /// Two to four components of a vector, by position (0 = x): `v.zyx` is `[2, 1, 0]`.
    Swizzle {
        value: ValueId,
        components: Vec<u8>,
    },
    /// A vector of `ty` with every component equal to the scalar `value`.
    Splat {
        ty: TypeId,
        value: ValueId,
    },
    /// A numeric conversion, componentwise for vectors: `f32(i)`, `vec3<f32>(v)`. Float to
    /// integer out of range traps on the CPU.
    Convert {
        ty: TypeId,
        value: ValueId,
    },
    /// The same bits as another type of the same width.
    Bitcast {
        ty: TypeId,
        value: ValueId,
    },
    /// A math intrinsic. On the GPU it's the WGSL builtin; on the CPU a WASM instruction or the
    /// stdlib's wrela body; the derived interpretations have a rule for each.
    Math(MathOp, Vec<ValueId>),
    /// `accept` if `cond` else `reject`, both already evaluated. With a vector of `bool` as
    /// `cond`, componentwise.
    Select {
        cond: ValueId,
        accept: ValueId,
        reject: ValueId,
    },
    Call {
        func: FuncId,
        args: Vec<Arg>,
    },
    /// A host function. Its parameters and result are scalars.
    CallImport {
        import: ImportId,
        args: Vec<ValueId>,
    },
    /// An enum value: `variant` of `ty`, with its fields.
    Variant {
        ty: TypeId,
        variant: u32,
        fields: Vec<ValueId>,
    },
    /// An enum value's variant, as a `u32`.
    Discriminant(ValueId),
    /// A field of an enum value's variant. The value must be that variant; lowering checks the
    /// discriminant first, and the interpreter traps if it's wrong.
    VariantField {
        value: ValueId,
        variant: u32,
        index: u32,
    },
    /// The length of a slice or array place, as a `u32`.
    Len(Place),
    /// An atomic operation on an atomic place.
    Atomic {
        op: AtomicOp,
        place: Place,
        args: Vec<ValueId>,
    },
    /// A derived interpretation of `func` (language.md §13). The derive pass replaces it with a
    /// `Call` of a generated function, so back ends never see it. Its type is the declared one.
    Derived {
        interpretation: Interpretation,
        func: FuncId,
        args: Vec<ValueId>,
    },
}

/// A scalar constant. Floats are kept as bits, so constants compare, hash and print exactly.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Const {
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32 { bits: u32 },
    F64 { bits: u64 },
}

impl Const {
    pub fn f32(value: f32) -> Const {
        Const::F32 {
            bits: value.to_bits(),
        }
    }

    pub fn f64(value: f64) -> Const {
        Const::F64 {
            bits: value.to_bits(),
        }
    }

    pub fn scalar(self) -> Scalar {
        match self {
            Const::Bool(_) => Scalar::Bool,
            Const::I8(_) => Scalar::I8,
            Const::U8(_) => Scalar::U8,
            Const::I16(_) => Scalar::I16,
            Const::U16(_) => Scalar::U16,
            Const::I32(_) => Scalar::I32,
            Const::U32(_) => Scalar::U32,
            Const::I64(_) => Scalar::I64,
            Const::U64(_) => Scalar::U64,
            Const::F32 { .. } => Scalar::F32,
            Const::F64 { .. } => Scalar::F64,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UnaryOp {
    /// Of a signed integer or float.
    Neg,
    /// Of a `bool`.
    Not,
    /// Of an integer.
    BitNot,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum BinaryOp {
    Add,
    Sub,
    /// Also matrix × vector, vector × matrix and matrix × matrix, as in WGSL.
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    /// The shift amount is a `u32` (componentwise for vectors).
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    /// `&&` and `||` on values that are both already evaluated; short-circuiting is an `If`.
    LogicalAnd,
    LogicalOr,
}

/// The math intrinsics (brief §5). Each is an IR primitive so every back end and interpretation
/// can treat it specially; see [`MathOp::name`] for their names.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MathOp {
    Abs,
    Sign,
    Min,
    Max,
    Clamp,
    Floor,
    Ceil,
    Round,
    Trunc,
    Fract,
    Sqrt,
    InverseSqrt,
    Exp,
    Exp2,
    Log,
    Log2,
    Pow,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Sinh,
    Cosh,
    Tanh,
    Mix,
    Step,
    SmoothStep,
    Length,
    Distance,
    Dot,
    Cross,
    Normalize,
    Reflect,
    Transpose,
    Determinant,
    /// Fragment-only screen-space derivatives.
    Dpdx,
    Dpdy,
    Fwidth,
}

impl MathOp {
    /// The intrinsic's name, as wrela's stdlib and WGSL spell it.
    pub fn name(self) -> &'static str {
        match self {
            MathOp::Abs => "abs",
            MathOp::Sign => "sign",
            MathOp::Min => "min",
            MathOp::Max => "max",
            MathOp::Clamp => "clamp",
            MathOp::Floor => "floor",
            MathOp::Ceil => "ceil",
            MathOp::Round => "round",
            MathOp::Trunc => "trunc",
            MathOp::Fract => "fract",
            MathOp::Sqrt => "sqrt",
            MathOp::InverseSqrt => "inverse_sqrt",
            MathOp::Exp => "exp",
            MathOp::Exp2 => "exp2",
            MathOp::Log => "log",
            MathOp::Log2 => "log2",
            MathOp::Pow => "pow",
            MathOp::Sin => "sin",
            MathOp::Cos => "cos",
            MathOp::Tan => "tan",
            MathOp::Asin => "asin",
            MathOp::Acos => "acos",
            MathOp::Atan => "atan",
            MathOp::Atan2 => "atan2",
            MathOp::Sinh => "sinh",
            MathOp::Cosh => "cosh",
            MathOp::Tanh => "tanh",
            MathOp::Mix => "mix",
            MathOp::Step => "step",
            MathOp::SmoothStep => "smoothstep",
            MathOp::Length => "length",
            MathOp::Distance => "distance",
            MathOp::Dot => "dot",
            MathOp::Cross => "cross",
            MathOp::Normalize => "normalize",
            MathOp::Reflect => "reflect",
            MathOp::Transpose => "transpose",
            MathOp::Determinant => "determinant",
            MathOp::Dpdx => "dpdx",
            MathOp::Dpdy => "dpdy",
            MathOp::Fwidth => "fwidth",
        }
    }
}

/// An argument. `In` parameters take a value, except a slice (`[T]`), which is passed by place;
/// `InOut` parameters take a place rooted at a local or an `InOut` parameter.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Arg {
    Value(ValueId),
    Place(Place),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AtomicOp {
    /// No operands; gives the value.
    Load,
    /// One operand; gives `()`.
    Store,
    /// One operand each; give the old value.
    Add,
    Sub,
    Max,
    Min,
    And,
    Or,
    Xor,
    Exchange,
    /// Operands `expected` and `new`; gives `(old value, whether it exchanged)`.
    CompareExchange,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Interpretation {
    /// Forward-mode derivative.
    Gradient,
    /// A conservative range over a box.
    Interval,
}

/// A GPU command recorded by CPU code: what `draw` and the screen-pass built-ins compile to. The
/// WASM back end encodes each as `runtime/command-stream.md` says and submits it; the interpreter
/// logs it. Later versions add buffers and dispatches.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Record {
    /// `clear` is a `vec4<f32>`.
    BeginScreenPass {
        clear: ValueId,
    },
    /// `vertex_count` and `instance_count` are `u32`s. `uniforms` is the value of the pipeline's
    /// inline uniform ([`Module::inline_uniform`]), present exactly when the pipeline has one.
    Draw {
        pipeline: PipelineId,
        vertex_count: ValueId,
        instance_count: ValueId,
        uniforms: Option<ValueId>,
    },
    Present,
}

/// A GPU entry point: a function, its stage, and how its parameters and result meet the
/// pipeline. The function itself is ordinary IR: its builtins and resources are parameters, so
/// the interpreter can run it by passing arguments.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct EntryPoint {
    pub function: FuncId,
    pub stage: Stage,
    /// One per parameter of `function`, in order.
    pub params: Vec<EntryParam>,
    /// How the result leaves the stage. `None` for a compute entry point, which returns `()`.
    pub result: Option<Io>,
    /// The resources the entry point's parameters come from.
    pub resources: Vec<Resource>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Stage {
    Vertex,
    Fragment,
    Compute { workgroup_size: [u32; 3] },
}

/// Where an entry point's parameter comes from.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EntryParam {
    /// A stage input: a builtin, an interpolated location, or a struct of them.
    Io(Io),
    /// Resource `resource` of the entry point; with `member`, that field of it. Several
    /// by-value `GpuData` parameters share one uniform this way, each as a member. A parameter
    /// from a `StorageReadWrite` resource is `InOut`; any other is `In`.
    Resource { resource: u32, member: Option<u32> },
}

/// One binding, or a struct whose fields each have one (as naga has it). wrela's builtin types
/// (`VertexIndex`, `FragCoord`, `ClipPosition`, …) may be erased to the builtin's own type with
/// `Binding`, or kept as structs with `Members`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Io {
    Binding(IoBinding),
    Members(Vec<IoBinding>),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IoBinding {
    Builtin(Builtin),
    /// A user value between stages: a numeric scalar or vector. Integers must be `Flat`.
    Location {
        location: u32,
        interpolation: Interpolation,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Interpolation {
    /// WGSL's default: perspective-correct, at the pixel centre.
    Perspective,
    /// Not interpolated: wrela's `Flat<T>`.
    Flat,
}

/// The GPU builtins, named after wrela's typed builtins (language.md §12).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Builtin {
    /// Vertex input, `u32`.
    VertexIndex,
    /// Vertex input, `u32`.
    InstanceIndex,
    /// Vertex output, `vec4<f32>`: WGSL's `position`.
    ClipPosition,
    /// Fragment input, `vec4<f32>`: WGSL's `position`, in framebuffer pixels.
    FragCoord,
    /// Compute input, `vec3<u32>`.
    GlobalId,
    /// Compute input, `vec3<u32>`.
    LocalId,
    /// Compute input, `vec3<u32>`.
    WorkgroupId,
}

impl Builtin {
    pub fn name(self) -> &'static str {
        match self {
            Builtin::VertexIndex => "vertex_index",
            Builtin::InstanceIndex => "instance_index",
            Builtin::ClipPosition => "clip_position",
            Builtin::FragCoord => "frag_coord",
            Builtin::GlobalId => "global_id",
            Builtin::LocalId => "local_id",
            Builtin::WorkgroupId => "workgroup_id",
        }
    }
}

/// A buffer an entry point binds.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Resource {
    pub group: u32,
    pub binding: u32,
    pub kind: ResourceKind,
    /// A `GpuData` type, or, in storage, a slice of one.
    pub ty: TypeId,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ResourceKind {
    Uniform,
    StorageRead,
    StorageReadWrite,
}

/// What the manifest lists, and what `DRAW` names by its [`PipelineId`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pipeline {
    pub kind: PipelineKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PipelineKind {
    Render { vertex: EntryId, fragment: EntryId },
    Compute { entry: EntryId },
}

/// A CPU function exported from the WASM module under `name`. Its parameters are `In` scalars and
/// it returns `()` or a scalar (runtime contract version 0).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Export {
    pub name: String,
    pub function: FuncId,
}

/// A host function: `name` from WASM import module `module`. Scalars in and out.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Import {
    pub module: String,
    pub name: String,
    pub params: Vec<TypeId>,
    pub ret: TypeId,
}
