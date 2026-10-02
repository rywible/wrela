use super::*;

/// The manifest that `runtime/command-stream.md` shows, so the spec's example stays valid.
fn spec_example() -> String {
    let spec = include_str!("../../../command-stream.md");
    let start = spec
        .find("```json\n")
        .expect("the spec has a ```json block")
        + "```json\n".len();
    let end = start + spec[start..].find("```").expect("the block is closed");
    spec[start..end].to_string()
}

fn first_light() -> Manifest {
    Manifest::from_json(&spec_example()).unwrap()
}

fn compute() -> Pipeline {
    Pipeline::Compute(ComputePipeline {
        id: 1,
        module: "kernels/sample.wgsl".to_string(),
        compute: "sample".to_string(),
        workgroup_size: [64, 1, 1],
        bindings: vec![
            Binding {
                group: 0,
                binding: 0,
                kind: BindingKind::Uniform,
                visibility: vec![Stage::Compute],
                size: 32,
                stride: None,
            },
            Binding {
                group: 0,
                binding: 1,
                kind: BindingKind::StorageReadWrite,
                visibility: vec![Stage::Compute],
                size: 0,
                stride: Some(16),
            },
        ],
    })
}

/// Applies `edit` to the first-light manifest's JSON value and parses the result.
fn edited(edit: impl FnOnce(&mut serde_json::Value)) -> Result<Manifest, ManifestError> {
    let mut value: serde_json::Value = serde_json::from_str(&spec_example()).unwrap();
    edit(&mut value);
    Manifest::from_json(&value.to_string())
}

fn remove(object: &mut serde_json::Value, key: &str) {
    object.as_object_mut().unwrap().remove(key).unwrap();
}

/// Applies `edit` to the parsed first-light manifest and validates it.
fn validated(edit: impl FnOnce(&mut Manifest)) -> Result<(), ManifestError> {
    let mut manifest = first_light();
    edit(&mut manifest);
    manifest.validate()
}

fn render(manifest: &mut Manifest) -> &mut RenderPipeline {
    match &mut manifest.pipelines[0] {
        Pipeline::Render(p) => p,
        Pipeline::Compute(_) => panic!("the first pipeline is a render pipeline"),
    }
}

/// serde rejects a key given twice, and a number with a fraction or exponent or written `-0` (which
/// it reads as a float) where an integer goes; the browser runtime's manifest tests check the same
/// cases.
#[test]
fn json_that_serde_rejects_is_rejected() {
    let example = spec_example();
    let cases = [
        ("\"version\": 0,", "\"version\": 0, \"version\": 0,"),
        ("\"version\": 0,", "\"version\": 0.0,"),
        ("\"version\": 0,", "\"version\": 0e0,"),
        ("\"size\": 16", "\"size\": 1.6e1"),
        ("\"size\": 16", "\"size\": 16, \"size\": 16"),
        ("\"offset\": 8", "\"offset\": 8.0"),
        ("\"version\": 0,", "\"version\": -0,"),
        ("\"id\": 0", "\"id\": -0"),
    ];
    for (from, to) in cases {
        assert!(example.contains(from), "{from}");
        let error = Manifest::from_json(&example.replacen(from, to, 1)).unwrap_err();
        assert!(matches!(error, ManifestError::Json(_)), "{to}: {error}");
    }
    let escaped = example.replacen(
        "\"version\": 0,",
        "\"version\": 0, \"ver\\u0073ion\": 0,",
        1,
    );
    assert!(matches!(
        Manifest::from_json(&escaped),
        Err(ManifestError::Json(_))
    ));
}

#[test]
fn the_spec_example_parses_to_the_first_light_manifest() {
    let manifest = first_light();
    assert_eq!(manifest.version, 0);
    assert_eq!(manifest.wasm, "program.wasm");
    let Pipeline::Render(p) = &manifest.pipelines[0] else {
        panic!("expected a render pipeline");
    };
    assert_eq!(
        (p.id, p.vertex.as_str(), p.fragment.as_str()),
        (0, "cover", "shade")
    );
    assert_eq!(p.color_target, ColorFormat::Rgba8Unorm);
    assert_eq!(manifest.pipelines[0].inline_uniform_size(), Some(16));
    let scene = &manifest.layouts[0];
    assert_eq!(
        (scene.name.as_str(), scene.size, scene.align),
        ("Scene", 16, 8)
    );
    assert_eq!(scene.fields[1].ty, "f32");
}

#[test]
fn json_round_trips() {
    let manifest = first_light();
    let text = manifest.to_json();
    assert_eq!(Manifest::from_json(&text).unwrap(), manifest);
    assert_eq!(manifest.to_json(), text, "deterministic");
    assert!(text.ends_with("}\n"));
    let mut both = manifest.clone();
    both.pipelines.push(compute());
    let text = both.to_json();
    assert_eq!(Manifest::from_json(&text).unwrap(), both);
    assert!(text.contains("\"stride\": 16"));
    assert_eq!(
        text.matches("\"stride\"").count(),
        1,
        "absent strides aren't written"
    );
}

#[test]
fn unknown_fields_are_rejected_at_every_level() {
    let cases: [fn(&mut serde_json::Value); 5] = [
        |v| v["extra"] = 1.into(),
        |v| v["pipelines"][0]["blend"] = "add".into(),
        |v| v["pipelines"][0]["bindings"][0]["dynamic"] = true.into(),
        |v| v["layouts"][0]["packed"] = true.into(),
        |v| v["layouts"][0]["fields"][0]["array"] = 2.into(),
    ];
    for edit in cases {
        let err = edited(edit).unwrap_err();
        assert!(
            matches!(&err, ManifestError::Json(m) if m.contains("unknown field")),
            "{err}"
        );
    }
}

#[test]
fn missing_fields_and_unknown_kinds_are_rejected() {
    let cases: [fn(&mut serde_json::Value); 6] = [
        |v| remove(v, "wasm"),
        |v| remove(&mut v["pipelines"][0], "fragment"),
        |v| remove(&mut v["pipelines"][0], "kind"),
        |v| v["pipelines"][0]["kind"] = "mesh".into(),
        |v| v["pipelines"][0]["color_target"] = "bgra8unorm".into(),
        |v| v["pipelines"][0]["bindings"][0]["kind"] = "storage".into(),
    ];
    for edit in cases {
        assert!(matches!(edited(edit), Err(ManifestError::Json(_))));
    }
    assert!(matches!(
        Manifest::from_json("not json"),
        Err(ManifestError::Json(_))
    ));
    assert!(matches!(
        Manifest::from_json(""),
        Err(ManifestError::Json(_))
    ));
}

#[test]
fn another_version_is_rejected() {
    assert_eq!(
        edited(|v| v["version"] = 1.into()),
        Err(ManifestError::UnsupportedVersion { found: 1 })
    );
}

#[test]
fn paths_stay_inside_the_manifest_directory() {
    for bad in [
        "",
        ".wasm",
        "/abs/first-light.wasm",
        "../first-light.wasm",
        "out/../first-light.wasm",
        "./first-light.wasm",
        "out//first-light.wasm",
        "out\\first-light.wasm",
        "C:first-light.wasm",
        "https://example.com/x.wasm",
        "first-light.wgsl",
        "first-light",
        // A browser resolves these as URLs, to other files than a native host opens.
        "%2e%2e/program.wasm",
        "a%20b.wasm",
        "a#.wasm",
        "x?.wasm",
        "a b.wasm",
        "café.wasm",
    ] {
        let result = validated(|m| m.wasm = bad.to_string());
        assert!(
            matches!(&result, Err(ManifestError::Path { path, .. }) if path == bad),
            "{bad:?}: {result:?}"
        );
    }
    assert_eq!(validated(|m| m.wasm = "out/a.b.wasm".to_string()), Ok(()));
    assert_eq!(
        validated(|m| render(m).module = "kernels/sample-1_v2.wgsl".to_string()),
        Ok(())
    );
    assert_eq!(
        validated(|m| m.wasm = "a#.wasm".to_string())
            .unwrap_err()
            .to_string(),
        "the manifest names the file `a#.wasm`: its parts have only letters, digits, `_`, `.` \
         and `-`"
    );
    let result = validated(|m| render(m).module = "shaders/x.wasm".to_string());
    assert!(matches!(result, Err(ManifestError::Path { .. })));
}

#[test]
fn pipeline_ids_are_unique() {
    let result = validated(|m| {
        let copy = m.pipelines[0].clone();
        m.pipelines.push(copy);
    });
    assert_eq!(result, Err(ManifestError::DuplicatePipeline { id: 0 }));
    assert_eq!(validated(|m| m.pipelines.push(compute())), Ok(()));
}

#[test]
fn entry_points_are_wgsl_identifiers() {
    for bad in ["", "_", "__x", "1st", "shade-fs", "café", "a b"] {
        let result = validated(|m| render(m).fragment = bad.to_string());
        assert!(
            matches!(&result, Err(ManifestError::Pipeline { id: 0, .. })),
            "{bad:?}: {result:?}"
        );
    }
    for good in ["shade", "_shade", "shade_f32", "S2"] {
        assert_eq!(validated(|m| render(m).vertex = good.to_string()), Ok(()));
    }
}

#[test]
fn workgroup_sizes_fit_webgpus_default_limits() {
    let with_size = |size: [u32; 3]| {
        validated(|m| {
            let mut p = compute();
            if let Pipeline::Compute(c) = &mut p {
                c.workgroup_size = size;
            }
            m.pipelines.push(p);
        })
    };
    for good in [[1, 1, 1], [256, 1, 1], [16, 16, 1], [1, 4, 64], [8, 8, 4]] {
        assert_eq!(with_size(good), Ok(()), "{good:?}");
    }
    for bad in [
        [0, 1, 1],
        [1, 0, 1],
        [257, 1, 1],
        [1, 1, 65],
        [16, 16, 2],
        [u32::MAX, u32::MAX, 1],
    ] {
        assert!(
            matches!(with_size(bad), Err(ManifestError::Pipeline { id: 1, .. })),
            "{bad:?}"
        );
    }
}

#[test]
fn bindings_follow_webgpu_and_version_0() {
    let binding = |edit: fn(&mut Binding)| validated(|m| edit(&mut render(m).bindings[0]));
    let compute_binding = |edit: fn(&mut Binding)| {
        validated(|m| {
            let mut p = compute();
            if let Pipeline::Compute(c) = &mut p {
                edit(&mut c.bindings[1]);
            }
            m.pipelines.push(p);
        })
    };
    let rejected = |result: Result<(), ManifestError>, needle: &str| match result {
        Err(ManifestError::Binding { problem, .. }) => {
            assert!(problem.contains(needle), "{problem:?} lacks {needle:?}");
        }
        other => panic!("expected a binding error about {needle:?}, got {other:?}"),
    };

    rejected(
        validated(|m| {
            let b = render(m).bindings[0].clone();
            render(m).bindings.push(b);
        }),
        "twice",
    );
    rejected(binding(|b| b.group = 4), "limits");
    rejected(binding(|b| b.binding = 1000), "limits");
    rejected(binding(|b| b.visibility.clear()), "visibility");
    rejected(
        binding(|b| b.visibility = vec![Stage::Fragment, Stage::Fragment]),
        "visibility",
    );
    rejected(binding(|b| b.visibility = vec![Stage::Compute]), "stage");
    rejected(binding(|b| b.size = 6), "multiples of 4");
    rejected(binding(|b| b.size = 0), "1 to 65536");
    rejected(binding(|b| b.size = 65540), "1 to 65536");
    rejected(binding(|b| b.stride = Some(16)), "runtime-sized");
    rejected(
        binding(|b| b.binding = 1),
        "only a uniform at group 0, binding 0",
    );
    rejected(
        binding(|b| b.kind = BindingKind::StorageRead),
        "only a uniform at group 0, binding 0",
    );
    rejected(
        binding(|b| {
            b.kind = BindingKind::StorageReadWrite;
            b.visibility = vec![Stage::Vertex, Stage::Fragment];
        }),
        "vertex",
    );
    assert_eq!(
        binding(|b| b.visibility = vec![Stage::Vertex, Stage::Fragment]),
        Ok(())
    );
    assert_eq!(binding(|b| b.size = 65536), Ok(()));
    assert_eq!(validated(|m| render(m).bindings.clear()), Ok(()));

    rejected(compute_binding(|b| b.stride = Some(0)), "multiples of 4");
    rejected(compute_binding(|b| b.stride = Some(6)), "multiples of 4");
    rejected(compute_binding(|b| b.stride = None), "empty");
    rejected(
        compute_binding(|b| b.visibility = vec![Stage::Fragment]),
        "stage",
    );
    assert_eq!(
        compute_binding(|b| {
            b.stride = None;
            b.size = 1024;
        }),
        Ok(())
    );
    assert_eq!(compute_binding(|b| b.size = 16), Ok(()));
}

#[test]
fn layouts_add_up() {
    let layout = |edit: fn(&mut Layout)| validated(|m| edit(&mut m.layouts[0]));
    let rejected = |result: Result<(), ManifestError>, needle: &str| match result {
        Err(ManifestError::Layout { problem, .. }) => {
            assert!(problem.contains(needle), "{problem:?} lacks {needle:?}");
        }
        other => panic!("expected a layout error about {needle:?}, got {other:?}"),
    };
    rejected(layout(|l| l.name.clear()), "name");
    rejected(layout(|l| l.align = 6), "power of two");
    rejected(layout(|l| l.align = 2), "power of two");
    rejected(layout(|l| l.size = 12), "multiple of the alignment");
    rejected(layout(|l| l.size = 0), "multiple of the alignment");
    rejected(
        layout(|l| l.fields[1].name = "resolution".to_string()),
        "twice",
    );
    rejected(layout(|l| l.fields[1].ty.clear()), "type");
    rejected(layout(|l| l.fields[1].offset = 4), "overlaps");
    rejected(layout(|l| l.fields.swap(0, 1)), "overlaps");
    rejected(layout(|l| l.fields[1].offset = 6), "multiples of 4");
    rejected(layout(|l| l.fields[1].size = 0), "multiples of 4");
    rejected(layout(|l| l.fields[1].offset = 16), "past the end");
    rejected(
        layout(|l| l.fields[1].offset = u32::MAX - 3),
        "past the end",
    );
    assert_eq!(layout(|l| l.fields.clear()), Ok(()));
    assert_eq!(
        validated(|m| {
            let copy = m.layouts[0].clone();
            m.layouts.push(copy);
        }),
        Err(ManifestError::DuplicateLayout {
            name: "Scene".to_string()
        })
    );
}

#[test]
fn draws_are_checked_against_their_pipeline() {
    let mut manifest = first_light();
    manifest.pipelines.push(compute());
    let draw = |pipeline: u32, len: usize| Command::Draw {
        pipeline,
        vertex_count: 3,
        instance_count: 1,
        uniforms: vec![0; len],
    };
    assert_eq!(manifest.check_command(&draw(0, 16)), Ok(()));
    assert_eq!(
        manifest.check_command(&draw(0, 12)),
        Err(CommandError::UniformSize {
            id: 0,
            expected: 16,
            found: 12
        })
    );
    assert_eq!(
        manifest.check_command(&draw(0, 0)),
        Err(CommandError::UniformSize {
            id: 0,
            expected: 16,
            found: 0
        })
    );
    assert_eq!(
        manifest.check_command(&draw(9, 16)),
        Err(CommandError::UnknownPipeline { id: 9 })
    );
    assert_eq!(
        manifest.check_command(&draw(1, 32)),
        Err(CommandError::NotRender { id: 1 })
    );
    // A pipeline with no inline uniform takes no bytes.
    render(&mut manifest).bindings.clear();
    assert_eq!(manifest.check_command(&draw(0, 0)), Ok(()));
    assert!(manifest.check_command(&draw(0, 16)).is_err());
    assert_eq!(manifest.check_command(&Command::Present), Ok(()));
    assert_eq!(
        manifest.check_command(&Command::BeginScreenPass { clear: [0.0; 4] }),
        Ok(())
    );
}

#[test]
fn errors_read_as_messages() {
    let messages = [
        ManifestError::UnsupportedVersion { found: 2 }.to_string(),
        ManifestError::DuplicatePipeline { id: 3 }.to_string(),
        CommandError::UniformSize {
            id: 0,
            expected: 16,
            found: 12,
        }
        .to_string(),
        validated(|m| m.wasm = "../x.wasm".to_string())
            .unwrap_err()
            .to_string(),
    ];
    for message in messages {
        assert!(!message.ends_with('.'), "{message}");
        assert!(!message.starts_with(char::is_uppercase), "{message}");
    }
}
