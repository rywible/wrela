//! Hand-built modules for tests, here and in the crates that consume the IR (the back ends, the
//! driver), so they don't wait on the front end and all test against the same module. Built with
//! the `fixtures` feature, or in this crate's own tests.

use crate::{
    BinaryOp, Builtin, Const, EntryParam, EntryPoint, Export, Expr, FunctionBuilder, Interpolation,
    Io, IoBinding, MathOp, Module, ParamMode, Pipeline, PipelineId, PipelineKind, Record, Resource,
    ResourceKind, Scalar, Stage, Stmt, StructField, TypeId, UnaryOp, ValueId, VectorSize,
};

/// The types first light needs.
struct T {
    unit: TypeId,
    u32: TypeId,
    f32: TypeId,
    vec2: TypeId,
    vec3: TypeId,
    vec4: TypeId,
    scene: TypeId,
}

#[expect(
    clippy::expect_used,
    reason = "a fixed struct of a vec2 and an f32 always has a GPU layout"
)]
fn types(module: &mut Module) -> T {
    let t = &mut module.types;
    let f32 = t.scalar(Scalar::F32);
    let vec2 = t.vector(VectorSize::Two, Scalar::F32);
    T {
        unit: t.unit(),
        u32: t.scalar(Scalar::U32),
        f32,
        vec2,
        vec3: t.vector(VectorSize::Three, Scalar::F32),
        vec4: t.vector(VectorSize::Four, Scalar::F32),
        scene: t
            .add_struct(
                "Scene",
                vec![
                    StructField {
                        name: "resolution".into(),
                        ty: vec2,
                    },
                    StructField {
                        name: "time".into(),
                        ty: f32,
                    },
                ],
                true,
            )
            .expect("two f32 fields always have a GPU layout"),
    }
}

/// A builder with shorthands for the arithmetic below.
struct B<'t> {
    b: FunctionBuilder,
    t: &'t T,
}

impl B<'_> {
    fn f(&mut self, x: f32) -> ValueId {
        self.b.value(self.t.f32, Expr::Const(Const::f32(x)))
    }
    fn u(&mut self, x: u32) -> ValueId {
        self.b.value(self.t.u32, Expr::Const(Const::U32(x)))
    }
    fn bin(&mut self, ty: TypeId, op: BinaryOp, a: ValueId, b: ValueId) -> ValueId {
        self.b.value(ty, Expr::Binary(op, a, b))
    }
    fn math(&mut self, ty: TypeId, op: MathOp, args: &[ValueId]) -> ValueId {
        self.b.value(ty, Expr::Math(op, args.to_vec()))
    }
    fn vec(&mut self, ty: TypeId, parts: &[ValueId]) -> ValueId {
        self.b.value(
            ty,
            Expr::Construct {
                ty,
                parts: parts.to_vec(),
            },
        )
    }
    fn extract(&mut self, value: ValueId, index: u32) -> ValueId {
        self.b.value(self.t.f32, Expr::Extract { value, index })
    }
}

/// `cover`: the full-screen triangle, with `VertexIndex` and `ClipPosition` erased to their
/// builtins' types.
fn cover(t: &T) -> crate::Function {
    let mut b = B {
        b: FunctionBuilder::new("cover", t.vec4),
        t,
    };
    let index = b.b.param("vertex", t.u32, ParamMode::In);
    let convert = |b: &mut B<'_>, value| b.b.value(t.f32, Expr::Convert { ty: t.f32, value });
    // let x = f32(vertex.index % 2) * 4.0 - 1.0, each constant where it first appears
    let i = b.b.value(t.u32, Expr::Load(index));
    let two = b.u(2);
    let n = b.bin(t.u32, BinaryOp::Rem, i, two);
    let n = convert(&mut b, n);
    let four = b.f(4.0);
    let n = b.bin(t.f32, BinaryOp::Mul, n, four);
    let one = b.f(1.0);
    let x = b.bin(t.f32, BinaryOp::Sub, n, one);
    // let y = f32(vertex.index / 2) * 4.0 - 1.0, reusing the load and the constants
    let n = b.bin(t.u32, BinaryOp::Div, i, two);
    let n = convert(&mut b, n);
    let n = b.bin(t.f32, BinaryOp::Mul, n, four);
    let y = b.bin(t.f32, BinaryOp::Sub, n, one);
    let zero = b.f(0.0);
    let position = b.vec(t.vec4, &[x, y, zero, one]);
    b.b.ret(Some(position));
    b.b.finish()
}

/// `shade`: the sphere's signed distance, coloured. The same steps as
/// `examples/first-light/main.wrela`, as a lowering would emit them.
fn shade(t: &T) -> crate::Function {
    use BinaryOp::{Add, Div, Mul, Sub};
    let mut b = B {
        b: FunctionBuilder::new("shade", t.vec4),
        t,
    };
    let pixel = b.b.param("pixel", t.vec4, ParamMode::In);
    let scene = b.b.param("scene", t.scene, ParamMode::In);
    // let uv = (pixel.position.xy * 2.0 - scene.resolution) / scene.resolution.y
    let position = b.b.value(t.vec4, Expr::Load(pixel));
    let xy = b.b.value(
        t.vec2,
        Expr::Swizzle {
            value: position,
            components: vec![0, 1],
        },
    );
    let two = b.f(2.0);
    let scaled = b.bin(t.vec2, Mul, xy, two);
    let resolution = b.b.value(t.vec2, Expr::Load(scene.clone().field(0)));
    let centred = b.bin(t.vec2, Sub, scaled, resolution);
    let height = b.extract(resolution, 1);
    let uv = b.bin(t.vec2, Div, centred, height);
    // let t = scene.time
    let time = b.b.value(t.f32, Expr::Load(scene.field(1)));
    // let p = vec3(uv.x, -uv.y, 0.25 * sin(t * 0.7))
    let ux = b.extract(uv, 0);
    let uy = b.extract(uv, 1);
    let down = b.b.value(t.f32, Expr::Unary(UnaryOp::Neg, uy));
    let quarter = b.f(0.25);
    let rate = b.f(0.7);
    let phase = b.bin(t.f32, Mul, time, rate);
    let wave = b.math(t.f32, MathOp::Sin, &[phase]);
    let z = b.bin(t.f32, Mul, quarter, wave);
    let p = b.vec(t.vec3, &[ux, down, z]);
    // let centre = vec3(0.35 * cos(t), 0.2 * sin(t * 1.3), 0.0)
    let a = b.f(0.35);
    let cos = b.math(t.f32, MathOp::Cos, &[time]);
    let cx = b.bin(t.f32, Mul, a, cos);
    let a = b.f(0.2);
    let rate = b.f(1.3);
    let phase = b.bin(t.f32, Mul, time, rate);
    let sin = b.math(t.f32, MathOp::Sin, &[phase]);
    let cy = b.bin(t.f32, Mul, a, sin);
    let zero = b.f(0.0);
    let centre = b.vec(t.vec3, &[cx, cy, zero]);
    // let radius = 0.45 + 0.05 * sin(t * 2.0)
    let r0 = b.f(0.45);
    let a = b.f(0.05);
    let phase = b.bin(t.f32, Mul, time, two);
    let sin = b.math(t.f32, MathOp::Sin, &[phase]);
    let breathe = b.bin(t.f32, Mul, a, sin);
    let radius = b.bin(t.f32, Add, r0, breathe);
    // let d = length(p - centre) - radius
    let offset = b.bin(t.vec3, Sub, p, centre);
    let len = b.math(t.f32, MathOp::Length, &[offset]);
    let d = b.bin(t.f32, Sub, len, radius);
    // let base = mix(vec3(1.0, 0.55, 0.25), vec3(0.25, 0.45, 0.85), step(0.0, d))
    let one = b.f(1.0);
    let g = b.f(0.55);
    let warm = b.vec(t.vec3, &[one, g, quarter]);
    let bl = b.f(0.85);
    let cool = b.vec(t.vec3, &[quarter, r0, bl]);
    let outside = b.math(t.f32, MathOp::Step, &[zero, d]);
    let base = b.math(t.vec3, MathOp::Mix, &[warm, cool, outside]);
    // let bands = 0.5 + 0.5 * cos(d * 62.831853)
    let half = b.f(0.5);
    let freq = b.f(62.831_853);
    let phase = b.bin(t.f32, Mul, d, freq);
    let cos = b.math(t.f32, MathOp::Cos, &[phase]);
    let wave = b.bin(t.f32, Mul, half, cos);
    let bands = b.bin(t.f32, Add, half, wave);
    // let fade = clamp(1.0 - abs(d) * 1.5, 0.0, 1.0)
    let distance = b.math(t.f32, MathOp::Abs, &[d]);
    let k = b.f(1.5);
    let falloff = b.bin(t.f32, Mul, distance, k);
    let near = b.bin(t.f32, Sub, one, falloff);
    let fade = b.math(t.f32, MathOp::Clamp, &[near, zero, one]);
    // let shaded = base * (0.75 + 0.25 * bands * fade)
    let floor = b.f(0.75);
    let band = b.bin(t.f32, Mul, quarter, bands);
    let band = b.bin(t.f32, Mul, band, fade);
    let light = b.bin(t.f32, Add, floor, band);
    let shaded = b.bin(t.vec3, Mul, base, light);
    // let rim = 1.0 - smoothstep(0.0, 0.015, abs(d))
    let width = b.f(0.015);
    let edge = b.math(t.f32, MathOp::SmoothStep, &[zero, width, distance]);
    let rim = b.bin(t.f32, Sub, one, edge);
    // vec4(mix(shaded, vec3(1.0), rim), 1.0)
    let white = b.b.value(
        t.vec3,
        Expr::Splat {
            ty: t.vec3,
            value: one,
        },
    );
    let colour = b.math(t.vec3, MathOp::Mix, &[shaded, white, rim]);
    let out = b.vec(t.vec4, &[colour, one]);
    b.b.ret(Some(out));
    b.b.finish()
}

/// `frame`: records the full-screen draw with pipeline 0.
fn frame(t: &T) -> crate::Function {
    let mut b = B {
        b: FunctionBuilder::new("frame", t.unit),
        t,
    };
    let time = b.b.param("time", t.f32, ParamMode::In);
    let width = b.b.param("width", t.u32, ParamMode::In);
    let height = b.b.param("height", t.u32, ParamMode::In);
    let w = b.b.value(t.u32, Expr::Load(width));
    let w = b.b.value(
        t.f32,
        Expr::Convert {
            ty: t.f32,
            value: w,
        },
    );
    let h = b.b.value(t.u32, Expr::Load(height));
    let h = b.b.value(
        t.f32,
        Expr::Convert {
            ty: t.f32,
            value: h,
        },
    );
    let resolution = b.vec(t.vec2, &[w, h]);
    let time = b.b.value(t.f32, Expr::Load(time));
    let scene = b.vec(t.scene, &[resolution, time]);
    let zero = b.f(0.0);
    let one = b.f(1.0);
    let clear = b.vec(t.vec4, &[zero, zero, zero, one]);
    b.b.emit(Stmt::Record(Record::BeginScreenPass { clear }));
    let vertices = b.u(3);
    let instances = b.u(1);
    b.b.emit(Stmt::Record(Record::Draw {
        pipeline: PipelineId(0),
        vertex_count: vertices,
        instance_count: instances,
        uniforms: Some(scene),
    }));
    b.b.emit(Stmt::Record(Record::Present));
    b.b.finish()
}

/// First light (`examples/first-light/main.wrela`), as S1's lowering is expected to produce it:
/// `cover` and `shade` as pipeline 0's entry points, and `frame` exported. Its printed form is
/// `src/tests/first_light.ir`.
pub fn first_light() -> Module {
    let mut module = Module::new();
    let t = types(&mut module);
    let cover = module.add_function(cover(&t));
    let shade = module.add_function(shade(&t));
    let frame = module.add_function(frame(&t));
    let vertex = module.add_entry_point(EntryPoint {
        function: cover,
        stage: Stage::Vertex,
        params: vec![EntryParam::Io(Io::Binding(IoBinding::Builtin(
            Builtin::VertexIndex,
        )))],
        result: Some(Io::Binding(IoBinding::Builtin(Builtin::ClipPosition))),
        resources: vec![],
    });
    let fragment = module.add_entry_point(EntryPoint {
        function: shade,
        stage: Stage::Fragment,
        params: vec![
            EntryParam::Io(Io::Binding(IoBinding::Builtin(Builtin::FragCoord))),
            EntryParam::Resource {
                resource: 0,
                member: None,
            },
        ],
        result: Some(Io::Binding(IoBinding::Location {
            location: 0,
            interpolation: Interpolation::Perspective,
        })),
        resources: vec![Resource {
            group: 0,
            binding: 0,
            kind: ResourceKind::Uniform,
            ty: t.scene,
        }],
    });
    module.add_pipeline(Pipeline {
        kind: PipelineKind::Render { vertex, fragment },
    });
    module.add_export(Export {
        name: "frame".into(),
        function: frame,
    });
    module
}
