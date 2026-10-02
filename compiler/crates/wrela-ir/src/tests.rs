use crate::fixtures::first_light;
use crate::{
    Arg, BinaryOp, Builtin, Const, EntryParam, EntryPoint, Expr, FuncId, FunctionBuilder,
    GpuLayout, Io, IoBinding, LayoutError, Module, ParamMode, PipelineId, Place, Record,
    ResourceKind, Scalar, Stage, Stmt, StructField, Type, TypeId, Types, ValueId, VectorSize,
    validate,
};

fn errors(module: &Module) -> Vec<String> {
    match validate(module) {
        Ok(()) => Vec::new(),
        Err(errors) => errors.iter().map(ToString::to_string).collect(),
    }
}

/// Asserts that validating `module` fails with a message containing `needle`.
fn rejects(module: &Module, needle: &str) {
    let errors = errors(module);
    assert!(
        errors.iter().any(|e| e.contains(needle)),
        "expected an error containing {needle:?}, got {errors:#?}"
    );
}

#[test]
fn first_light_validates() {
    assert_eq!(errors(&first_light()), Vec::<String>::new());
}

/// The golden text of first light's IR. `WRELA_BLESS=1` rewrites it.
#[test]
fn first_light_prints() {
    let text = first_light().to_string();
    assert_eq!(first_light().to_string(), text, "printing is deterministic");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/first_light.ir");
    if std::env::var_os("WRELA_BLESS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text, expected,
        "the printed IR changed; if that's intended, rerun with WRELA_BLESS=1"
    );
}

#[test]
fn scene_has_the_uniform_layout() {
    let module = first_light();
    let inline = module.inline_uniform(PipelineId(0)).unwrap();
    assert_eq!((inline.group, inline.binding), (0, 0));
    let Some(Type::Struct(scene)) = module.types.get(inline.ty) else {
        panic!("the inline uniform is the Scene struct");
    };
    assert_eq!(scene.name, "Scene");
    assert_eq!(
        scene.gpu_layout,
        Some(GpuLayout {
            size: 16,
            align: 8,
            offsets: vec![0, 8]
        })
    );
}

#[test]
fn gpu_layouts_follow_wgsl_uniform_rules() {
    let mut t = Types::new();
    let f32 = t.scalar(Scalar::F32);
    let u32 = t.scalar(Scalar::U32);
    let vec2 = t.vector(VectorSize::Two, Scalar::F32);
    let vec3 = t.vector(VectorSize::Three, Scalar::F32);
    let vec4 = t.vector(VectorSize::Four, Scalar::F32);
    let layout = |t: &Types, fields: &[TypeId]| t.gpu_struct_layout(fields).unwrap();
    let field = |name: &str, ty| StructField {
        name: name.into(),
        ty,
    };

    // A vec3 is 12 bytes aligned to 16, so a scalar packs into its last four.
    assert_eq!(
        layout(&t, &[vec3, f32]),
        GpuLayout {
            size: 16,
            align: 16,
            offsets: vec![0, 12]
        }
    );
    assert_eq!(layout(&t, &[f32, vec3]).offsets, [0, 16]);
    assert_eq!(layout(&t, &[f32, u32, vec2]).size, 16);
    assert_eq!(layout(&t, &[f32]).size, 4);
    // Arrays: element strides round up to 16.
    let floats = t.intern(Type::Array {
        element: f32,
        len: 4,
    });
    assert_eq!(t.gpu_size_align(floats), Ok((64, 16)));
    assert_eq!(layout(&t, &[f32, floats, f32]).offsets, [0, 16, 80]);
    // A nested struct starts on 16 bytes and the next field at least 16 bytes later.
    let small = t.add_struct("Small", vec![field("a", f32)], true).unwrap();
    assert_eq!(t.gpu_size_align(small), Ok((4, 4)));
    assert_eq!(
        layout(&t, &[f32, small, f32]),
        GpuLayout {
            size: 48,
            align: 16,
            offsets: vec![0, 16, 32]
        }
    );
    // Matrices are columns of vectors.
    let mat = |t: &mut Types, c, r| {
        let m = t.intern(Type::Matrix {
            columns: VectorSize::from_count(c).unwrap(),
            rows: VectorSize::from_count(r).unwrap(),
        });
        t.gpu_size_align(m).unwrap()
    };
    assert_eq!(mat(&mut t, 2, 2), (16, 8));
    assert_eq!(mat(&mut t, 3, 3), (48, 16));
    assert_eq!(mat(&mut t, 4, 4), (64, 16));
    assert_eq!(mat(&mut t, 4, 2), (32, 8));
    assert_eq!(layout(&t, &[vec4, vec2]).size, 32);

    // What can't cross to the GPU.
    for scalar in [
        Scalar::Bool,
        Scalar::F64,
        Scalar::I64,
        Scalar::U64,
        Scalar::I8,
        Scalar::U16,
    ] {
        let ty = t.scalar(scalar);
        assert_eq!(
            t.gpu_struct_layout(&[ty]),
            Err(LayoutError::NotHostShareable(ty)),
            "{scalar:?}"
        );
    }
    let plain = t.add_struct("Plain", vec![field("a", f32)], false).unwrap();
    assert_eq!(
        t.gpu_struct_layout(&[plain]),
        Err(LayoutError::NotHostShareable(plain))
    );
    let empty = t.intern(Type::Array {
        element: f32,
        len: 0,
    });
    assert_eq!(
        t.gpu_struct_layout(&[empty]),
        Err(LayoutError::ZeroLength(empty))
    );
    assert_eq!(t.gpu_struct_layout(&[]), Err(LayoutError::NoFields));
    let huge = t.intern(Type::Array {
        element: vec4,
        len: u32::MAX,
    });
    assert_eq!(t.gpu_size_align(huge), Err(LayoutError::TooLarge));
}

#[test]
fn types_intern_and_print() {
    let mut t = Types::new();
    let f32 = t.scalar(Scalar::F32);
    assert_eq!(t.scalar(Scalar::F32), f32);
    let u32 = t.scalar(Scalar::U32);
    let tuple = t.intern(Type::Tuple(vec![f32, u32]));
    let one = t.intern(Type::Tuple(vec![f32]));
    let slice = t.intern(Type::Slice { element: u32 });
    let atomic = t.intern(Type::Atomic(Scalar::U32));
    let names: Vec<String> = [tuple, one, slice, atomic]
        .iter()
        .map(|&id| t.name(id).to_string())
        .collect();
    assert_eq!(names, ["(f32, u32)", "(f32,)", "[u32]", "atomic<u32>"]);
    assert_eq!(t.name(TypeId(99)).to_string(), "?99");
    assert_eq!(t.len(), 6);
}

/// Structured control flow: `var s = 0; var i = 0; while i < n { s += i; i += 1 }; s`, with an
/// `InOut` parameter and a call.
#[test]
fn loops_locals_and_calls_validate_and_print() {
    let mut module = Module::new();
    let u32 = module.types.scalar(Scalar::U32);
    let bool_ = module.types.scalar(Scalar::Bool);
    let unit = module.types.unit();

    let mut b = FunctionBuilder::new("bump", unit);
    let target = b.param("target", u32, ParamMode::InOut);
    let old = b.value(u32, Expr::Load(target.clone()));
    let one = b.value(u32, Expr::Const(Const::U32(1)));
    let new = b.value(u32, Expr::Binary(BinaryOp::Add, old, one));
    b.store(target, new);
    let bump = module.add_function(b.finish());

    let mut b = FunctionBuilder::new("sum_below", u32);
    let n = b.param("n", u32, ParamMode::In);
    let s = b.local("s", u32);
    let i = b.local("i", u32);
    let zero = b.value(u32, Expr::Const(Const::U32(0)));
    b.store(Place::local(s), zero);
    b.store(Place::local(i), zero);
    b.loop_with(
        |b| {
            let iv = b.value(u32, Expr::Load(Place::local(i)));
            let nv = b.value(u32, Expr::Load(n.clone()));
            let done = b.value(bool_, Expr::Binary(BinaryOp::Ge, iv, nv));
            b.if_else(done, |b| b.emit(Stmt::Break), |_| {});
            let sv = b.value(u32, Expr::Load(Place::local(s)));
            let sum = b.value(u32, Expr::Binary(BinaryOp::Add, sv, iv));
            b.store(Place::local(s), sum);
        },
        |b| {
            b.value(
                unit,
                Expr::Call {
                    func: bump,
                    args: vec![Arg::Place(Place::local(i))],
                },
            );
        },
    );
    let total = b.value(u32, Expr::Load(Place::local(s)));
    b.ret(Some(total));
    module.add_function(b.finish());

    assert_eq!(errors(&module), Vec::<String>::new());
    let expected = "\
fn bump(p0 target: inout u32) -> () {
    %0: u32 = load p0
    %1: u32 = const u32 1
    %2: u32 = add %0, %1
    store p0, %2
}

fn sum_below(p0 n: u32) -> u32 {
    local l0 s: u32
    local l1 i: u32
    %0: u32 = const u32 0
    store l0, %0
    store l1, %0
    loop {
        %1: u32 = load l1
        %2: u32 = load p0
        %3: bool = ge %1, %2
        if %3 {
            break
        }
        %4: u32 = load l0
        %5: u32 = add %4, %1
        store l0, %5
    } continuing {
        %6: () = call bump(&l1)
    }
    %7: u32 = load l0
    return %7
}
";
    assert_eq!(module.to_string(), expected);
}

/// Replaces the body of function `func` in a valid first-light module.
fn with_body(func: u32, edit: impl FnOnce(&mut crate::Function)) -> Module {
    let mut module = first_light();
    edit(&mut module.functions[func as usize]);
    module
}

#[test]
fn values_must_be_defined_before_use_and_in_scope() {
    // A use before the definition: `cover`'s `%4 = rem %0, %1` moved before `%0 = load p0`.
    let module = with_body(0, |f| f.body.0.swap(0, 4));
    rejects(&module, "%0 is used before it's defined");
    // A value defined in an `if` arm, used after it.
    let mut module = Module::new();
    let u32 = module.types.scalar(Scalar::U32);
    let bool_ = module.types.scalar(Scalar::Bool);
    let mut b = FunctionBuilder::new("leak", u32);
    let yes = b.value(bool_, Expr::Const(Const::Bool(true)));
    let mut inner = ValueId(0);
    b.if_else(
        yes,
        |b| inner = b.value(u32, Expr::Const(Const::U32(1))),
        |_| {},
    );
    b.ret(Some(inner));
    module.add_function(b.finish());
    rejects(
        &module,
        "%1 is used before it's defined, or outside the block",
    );
    // Defined twice.
    let module = with_body(0, |f| {
        let first = f.body.0[0].clone();
        f.body.0.insert(1, first);
    });
    rejects(&module, "%0 is defined twice");
    // Declared, never defined.
    let module = with_body(0, |f| f.values.push(f.ret));
    rejects(&module, "is declared but never defined");
}

#[test]
fn types_must_line_up() {
    // `cover`'s `%1: u32 = const u32 2` with an f32 constant.
    let module = with_body(0, |f| {
        f.body.0[1] = Stmt::Let(ValueId(1), Expr::Const(Const::f32(2.0)));
    });
    rejects(&module, "%1 is declared u32 but its expression is a f32");
    // ...and declared f32 too (the type of `%3`), so `%2 = rem %0, %1` mixes a u32 and an f32.
    let module = with_body(0, |f| {
        f.body.0[1] = Stmt::Let(ValueId(1), Expr::Const(Const::f32(2.0)));
        f.values[1] = f.values[3];
    });
    rejects(&module, "Rem doesn't apply to a u32 and a f32");
    // Returning the wrong type.
    let module = with_body(0, |f| {
        let last = f.body.0.len() - 1;
        f.body.0[last] = Stmt::Return(Some(ValueId(0)));
    });
    rejects(
        &module,
        "returns a u32 from a function that returns vec4<f32>",
    );
    // A function that can fall off its end.
    let module = with_body(0, |f| {
        f.body.0.pop();
    });
    rejects(&module, "can reach its end without returning a vec4<f32>");
    // A vector built from the wrong number of components.
    let module = with_body(0, |f| {
        let last = f.body.0.len() - 2;
        if let Stmt::Let(_, Expr::Construct { parts, .. }) = &mut f.body.0[last] {
            parts.pop();
        }
    });
    rejects(
        &module,
        "at least two parts with 4 components in all, not 3",
    );
}

#[test]
fn stores_need_writable_places_and_loops_need_their_rules() {
    // `frame` writing its `In` parameter.
    let module = with_body(2, |f| {
        f.body.0.push(Stmt::Store(Place::param(0), ValueId(5)));
    });
    rejects(&module, "writes parameter p0 (`time`), which isn't `InOut`");
    let module = with_body(2, |f| f.body.0.push(Stmt::Break));
    rejects(&module, "`break` or `continue` outside a loop");
    let module = with_body(2, |f| {
        f.body.0.push(Stmt::Loop {
            body: crate::Block(vec![Stmt::Break]),
            continuing: crate::Block(vec![Stmt::Continue]),
        });
    });
    rejects(&module, "`continuing` block can't break or continue");
}

#[test]
fn calls_are_checked_against_their_callee() {
    let mut module = first_light();
    let f32 = module.types.scalar(Scalar::F32);
    let mut b = FunctionBuilder::new("caller", f32);
    let x = b.value(f32, Expr::Const(Const::f32(1.0)));
    // `frame` takes three arguments and returns ().
    let r = b.value(
        f32,
        Expr::Call {
            func: FuncId(2),
            args: vec![Arg::Value(x)],
        },
    );
    b.ret(Some(r));
    module.add_function(b.finish());
    rejects(&module, "`frame` takes 3 arguments, not 1");
}

#[test]
fn draws_must_match_their_pipeline() {
    // Uniforms of the wrong type: the clear colour instead of the scene.
    let module = with_body(2, |f| {
        for stmt in &mut f.body.0 {
            if let Stmt::Record(Record::Draw { uniforms, .. }) = stmt {
                *uniforms = Some(ValueId(9));
            }
        }
    });
    rejects(
        &module,
        "draws pipeline 0 with uniforms of type vec4<f32>, not Scene",
    );
    let module = with_body(2, |f| {
        for stmt in &mut f.body.0 {
            if let Stmt::Record(Record::Draw { uniforms, .. }) = stmt {
                *uniforms = None;
            }
        }
    });
    rejects(&module, "pipeline 0 needs uniforms");
    let module = with_body(2, |f| {
        f.body.0.push(Stmt::Record(Record::Draw {
            pipeline: PipelineId(7),
            vertex_count: ValueId(11),
            instance_count: ValueId(11),
            uniforms: None,
        }));
    });
    rejects(&module, "pipeline 7, which isn't a render pipeline");
}

#[test]
fn entry_points_must_fit_their_functions_and_stages() {
    let mut module = first_light();
    module.entry_points[0].params[0] =
        EntryParam::Io(Io::Binding(IoBinding::Builtin(Builtin::FragCoord)));
    rejects(&module, "frag_coord isn't an input of a vertex entry point");
    rejects(&module, "frag_coord has the wrong type u32");

    let mut module = first_light();
    module.entry_points[0].result = Some(Io::Binding(IoBinding::Location {
        location: 0,
        interpolation: crate::Interpolation::Perspective,
    }));
    rejects(&module, "a vertex entry point outputs one clip_position");

    let mut module = first_light();
    module.entry_points[1].resources[0].kind = ResourceKind::StorageReadWrite;
    rejects(&module, "should be InOut, for its resource");

    // A fragment input with no matching vertex output.
    let mut module = first_light();
    let vec4 = module.types.vector(VectorSize::Four, Scalar::F32);
    module.functions[1].params[0].ty = vec4;
    module.entry_points[1].params[0] = EntryParam::Io(Io::Binding(IoBinding::Location {
        location: 3,
        interpolation: crate::Interpolation::Perspective,
    }));
    rejects(&module, "fragment location 3 doesn't match a vertex output");
}

#[test]
fn gpu_code_records_nothing_and_uses_only_gpu_types() {
    let module = with_body(1, |f| {
        f.body.0.insert(0, Stmt::Record(Record::Present));
    });
    rejects(&module, "in `shade`: is GPU code but records a GPU command");
    let mut module = first_light();
    let f64 = module.types.scalar(Scalar::F64);
    module.functions[0].locals.push(crate::Local {
        name: "wide".into(),
        ty: f64,
    });
    rejects(&module, "in `cover`: is GPU code but uses f64");
}

#[test]
fn gpu_code_cannot_recurse() {
    // A compute kernel calls `leaf`, which is fine, and `ping`, which calls `pong`, which calls
    // `ping`.
    let mut module = first_light();
    let unit = module.types.unit();
    let call = |b: &mut FunctionBuilder, func: u32| {
        b.value(
            unit,
            Expr::Call {
                func: FuncId(func),
                args: vec![],
            },
        );
    };
    let (leaf, ping, pong) = (3, 4, 5);
    module.add_function(FunctionBuilder::new("leaf", unit).finish());
    let mut b = FunctionBuilder::new("ping", unit);
    call(&mut b, pong);
    call(&mut b, leaf);
    module.add_function(b.finish());
    let mut b = FunctionBuilder::new("pong", unit);
    call(&mut b, ping);
    module.add_function(b.finish());
    let mut b = FunctionBuilder::new("kernel", unit);
    call(&mut b, leaf);
    call(&mut b, ping);
    let kernel = module.add_function(b.finish());
    module.add_entry_point(EntryPoint {
        function: kernel,
        stage: Stage::Compute {
            workgroup_size: [64, 1, 1],
        },
        params: vec![],
        result: None,
        resources: vec![],
    });
    let errors = errors(&module);
    assert_eq!(errors, ["in `ping`: is GPU code but recurses"]);
}

#[test]
fn stored_gpu_layouts_must_be_right() {
    let mut module = first_light();
    let f32 = module.types.scalar(Scalar::F32);
    // Interning a struct whose stored layout is wrong.
    module.types.intern(Type::Struct(crate::StructType {
        name: "Wrong".into(),
        fields: vec![StructField {
            name: "a".into(),
            ty: f32,
        }],
        gpu_layout: Some(GpuLayout {
            size: 16,
            align: 16,
            offsets: vec![0],
        }),
    }));
    rejects(&module, "struct `Wrong` stores the GPU layout");
}

#[test]
fn a_huge_array_is_checked_without_allocating_it() {
    let mut module = Module::new();
    let f32 = module.types.scalar(Scalar::F32);
    let huge = module.types.intern(Type::Array {
        element: f32,
        len: u32::MAX,
    });
    let mut b = FunctionBuilder::new("huge", huge);
    let x = b.value(f32, Expr::Const(Const::f32(1.0)));
    let array = b.value(
        huge,
        Expr::Construct {
            ty: huge,
            parts: vec![x],
        },
    );
    b.ret(Some(array));
    module.add_function(b.finish());
    rejects(
        &module,
        "a [f32; 4294967295] needs 4294967295 elements, not 1",
    );
}

#[test]
fn names_are_unique() {
    let mut module = first_light();
    let copy = module.functions[0].clone();
    module.add_function(copy);
    rejects(&module, "function name `cover` is empty or not unique");
}

#[test]
fn float_constants_print_exactly() {
    let mut module = Module::new();
    let f32 = module.types.scalar(Scalar::F32);
    let unit = module.types.unit();
    let mut b = FunctionBuilder::new("consts", unit);
    for x in [0.1f32, -0.0, 1e-9, 62.831_853, f32::INFINITY] {
        b.value(f32, Expr::Const(Const::f32(x)));
    }
    b.value(f32, Expr::Const(Const::F32 { bits: 0x7fc0_0001 }));
    module.add_function(b.finish());
    let text = module.to_string();
    for line in [
        "%0: f32 = const f32 0.1",
        "%1: f32 = const f32 -0.0",
        "%2: f32 = const f32 1e-9",
        "%3: f32 = const f32 62.831852",
        "%4: f32 = const f32 inf",
        "%5: f32 = const f32 nan(0x7fc00001)",
    ] {
        assert!(text.contains(line), "{line:?} not in\n{text}");
    }
}
