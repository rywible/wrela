//! The build manifest: what a host needs to set up a program's GPU side. It names the WASM module
//! and, for each pipeline, its WGSL module, entry points and bindings; it also describes every
//! `GpuData` layout. JSON, versioned with the rest of the contract. Unknown fields are errors, so
//! a host never half-reads a manifest it doesn't understand.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::VERSION;
use crate::stream::Command;

/// WebGPU's default limits, which every manifest must fit.
pub mod limits {
    pub const MAX_BIND_GROUPS: u32 = 4;
    pub const MAX_BINDINGS_PER_GROUP: u32 = 1000;
    pub const MAX_UNIFORM_BINDING_SIZE: u32 = 65536;
    pub const MAX_WORKGROUP_SIZE: [u32; 3] = [256, 256, 64];
    pub const MAX_WORKGROUP_INVOCATIONS: u32 = 256;
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The contract version, [`VERSION`].
    pub version: u16,
    /// The program's WASM module, relative to the manifest.
    pub wasm: String,
    pub pipelines: Vec<Pipeline>,
    /// Every `GpuData` type's layout, for tools; hosts don't need them to run a program.
    pub layouts: Vec<Layout>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pipeline {
    Render(RenderPipeline),
    Compute(ComputePipeline),
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderPipeline {
    /// What `DRAW` names it by. Unique among the manifest's pipelines.
    pub id: u32,
    /// The WGSL module holding both entry points, relative to the manifest.
    pub module: String,
    pub vertex: String,
    pub fragment: String,
    pub color_target: ColorFormat,
    pub bindings: Vec<Binding>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputePipeline {
    pub id: u32,
    pub module: String,
    pub compute: String,
    pub workgroup_size: [u32; 3],
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum ColorFormat {
    #[serde(rename = "rgba8unorm")]
    Rgba8Unorm,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub group: u32,
    pub binding: u32,
    pub kind: BindingKind,
    /// The stages whose entry points use the binding.
    pub visibility: Vec<Stage>,
    /// The minimum binding size in bytes. With a `stride`, what comes before the runtime-sized
    /// array (0 for a bare array).
    pub size: u32,
    /// The element stride of a runtime-sized array at the end of a storage binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stride: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKind {
    Uniform,
    StorageRead,
    StorageReadWrite,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Vertex,
    Fragment,
    Compute,
}

/// A `GpuData` type's layout: the same in WASM memory and on the GPU.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub name: String,
    pub size: u32,
    pub align: u32,
    pub fields: Vec<FieldLayout>,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldLayout {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    /// The field's type as written in wrela, such as `vec2` or `[f32; 4]`.
    #[serde(rename = "type")]
    pub ty: String,
}

impl Pipeline {
    pub fn id(&self) -> u32 {
        match self {
            Pipeline::Render(p) => p.id,
            Pipeline::Compute(p) => p.id,
        }
    }

    pub fn module(&self) -> &str {
        match self {
            Pipeline::Render(p) => &p.module,
            Pipeline::Compute(p) => &p.module,
        }
    }

    pub fn bindings(&self) -> &[Binding] {
        match self {
            Pipeline::Render(p) => &p.bindings,
            Pipeline::Compute(p) => &p.bindings,
        }
    }

    /// The size of the uniform at group 0, binding 0, whose bytes `DRAW` carries inline; `None`
    /// when the pipeline has no binding there.
    pub fn inline_uniform_size(&self) -> Option<u32> {
        self.bindings()
            .iter()
            .find(|b| b.group == 0 && b.binding == 0 && b.kind == BindingKind::Uniform)
            .map(|b| b.size)
    }
}

impl Manifest {
    /// Parses and validates a manifest.
    pub fn from_json(text: &str) -> Result<Manifest, ManifestError> {
        let manifest: Manifest =
            serde_json::from_str(text).map_err(|e| ManifestError::Json(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// The manifest as pretty-printed JSON with a final newline. Deterministic: the same manifest
    /// always gives the same bytes.
    pub fn to_json(&self) -> String {
        #[expect(
            clippy::expect_used,
            reason = "these types have only string keys and finite numbers, which always serialize"
        )]
        let mut text = serde_json::to_string_pretty(self).expect("a manifest serializes");
        text.push('\n');
        text
    }

    pub fn pipeline(&self, id: u32) -> Option<&Pipeline> {
        self.pipelines.iter().find(|p| p.id() == id)
    }

    /// Checks everything JSON parsing doesn't: the version, safe relative paths, unique ids and
    /// bindings, entry point names, WebGPU's default limits, and layouts that add up.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.version != VERSION {
            return Err(ManifestError::UnsupportedVersion {
                found: self.version,
            });
        }
        check_path(&self.wasm, ".wasm")?;
        let mut ids = BTreeSet::new();
        for pipeline in &self.pipelines {
            let id = pipeline.id();
            if !ids.insert(id) {
                return Err(ManifestError::DuplicatePipeline { id });
            }
            check_path(pipeline.module(), ".wgsl")?;
            let problem = |problem: String| ManifestError::Pipeline { id, problem };
            match pipeline {
                Pipeline::Render(p) => {
                    check_entry_point(&p.vertex).map_err(problem)?;
                    check_entry_point(&p.fragment).map_err(problem)?;
                }
                Pipeline::Compute(p) => {
                    check_entry_point(&p.compute).map_err(problem)?;
                    check_workgroup_size(p.workgroup_size).map_err(problem)?;
                }
            }
            check_bindings(pipeline)?;
        }
        let mut names = BTreeSet::new();
        for layout in &self.layouts {
            if !names.insert(layout.name.as_str()) {
                return Err(ManifestError::DuplicateLayout {
                    name: layout.name.clone(),
                });
            }
            check_layout(layout).map_err(|problem| ManifestError::Layout {
                name: layout.name.clone(),
                problem,
            })?;
        }
        Ok(())
    }

    /// Checks a decoded command against this manifest: a `DRAW` must name a render pipeline and
    /// carry exactly the bytes of its inline uniform.
    pub fn check_command(&self, command: &Command) -> Result<(), CommandError> {
        let Command::Draw {
            pipeline, uniforms, ..
        } = command
        else {
            return Ok(());
        };
        let id = *pipeline;
        match self.pipeline(id) {
            None => Err(CommandError::UnknownPipeline { id }),
            Some(Pipeline::Compute(_)) => Err(CommandError::NotRender { id }),
            Some(p @ Pipeline::Render(_)) => {
                let expected = p.inline_uniform_size().unwrap_or(0);
                if usize::try_from(expected).ok() == Some(uniforms.len()) {
                    Ok(())
                } else {
                    Err(CommandError::UniformSize {
                        id,
                        expected,
                        found: uniforms.len(),
                    })
                }
            }
        }
    }
}

/// A path the manifest names: relative, `/`-separated, inside the manifest's directory, with
/// only letters, digits, `_`, `.` and `-` in its parts. A browser resolves the path as a URL, so
/// `%`, `?`, `#` or a space would make it fetch a different file than a native host opens
/// (`%2e%2e/x.wasm` is the parent directory's `x.wasm` to a browser).
fn check_path(path: &str, extension: &str) -> Result<(), ManifestError> {
    let reason = if path.contains('\\') || path.contains(':') {
        Some("use `/` and no drive or scheme")
    } else if path
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        Some("it must be relative, with no empty, `.` or `..` parts")
    } else if !path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '/'))
    {
        Some("its parts have only letters, digits, `_`, `.` and `-`")
    } else if !path.ends_with(extension) || path.len() == extension.len() {
        Some(if extension == ".wasm" {
            "a WASM module's name ends in `.wasm`"
        } else {
            "a WGSL module's name ends in `.wgsl`"
        })
    } else {
        None
    };
    match reason {
        Some(reason) => Err(ManifestError::Path {
            path: path.to_string(),
            reason,
        }),
        None => Ok(()),
    }
}

/// An entry point name: a WGSL identifier (ASCII, not `_`, not starting with `__`).
fn check_entry_point(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && name != "_"
        && !name.starts_with("__");
    if valid {
        Ok(())
    } else {
        Err(format!("`{name}` isn't a valid entry point name"))
    }
}

fn check_workgroup_size(size: [u32; 3]) -> Result<(), String> {
    let within = size
        .iter()
        .zip(limits::MAX_WORKGROUP_SIZE)
        .all(|(&n, max)| (1..=max).contains(&n));
    let invocations = size
        .iter()
        .try_fold(1u32, |product, &n| product.checked_mul(n));
    match invocations {
        Some(n) if within && n <= limits::MAX_WORKGROUP_INVOCATIONS => Ok(()),
        _ => Err(format!(
            "workgroup size {size:?} is outside WebGPU's default limits: each of x and y in \
             1..=256, z in 1..=64, and at most 256 invocations"
        )),
    }
}

fn check_bindings(pipeline: &Pipeline) -> Result<(), ManifestError> {
    let id = pipeline.id();
    let mut seen = BTreeSet::new();
    for b in pipeline.bindings() {
        let problem = |problem: &str| ManifestError::Binding {
            pipeline: id,
            group: b.group,
            binding: b.binding,
            problem: problem.to_string(),
        };
        if !seen.insert((b.group, b.binding)) {
            return Err(problem("it appears twice"));
        }
        if b.group >= limits::MAX_BIND_GROUPS || b.binding >= limits::MAX_BINDINGS_PER_GROUP {
            return Err(problem(
                "WebGPU's default limits allow groups 0..=3 and bindings 0..=999",
            ));
        }
        let mut stages = BTreeSet::new();
        if b.visibility.is_empty() || !b.visibility.iter().all(|s| stages.insert(*s)) {
            return Err(problem(
                "its visibility must list each stage once, and not be empty",
            ));
        }
        let allowed: &[Stage] = match pipeline {
            Pipeline::Render(_) => &[Stage::Vertex, Stage::Fragment],
            Pipeline::Compute(_) => &[Stage::Compute],
        };
        if !b.visibility.iter().all(|s| allowed.contains(s)) {
            return Err(problem(
                "it's visible to a stage this pipeline doesn't have",
            ));
        }
        if b.kind == BindingKind::StorageReadWrite && b.visibility.contains(&Stage::Vertex) {
            return Err(problem(
                "WebGPU forbids writable storage in a vertex shader",
            ));
        }
        if !b.size.is_multiple_of(4) || b.stride.is_some_and(|s| s == 0 || !s.is_multiple_of(4)) {
            return Err(problem("sizes and strides are positive multiples of 4"));
        }
        match (b.kind, b.stride) {
            (BindingKind::Uniform, Some(_)) => {
                return Err(problem("a uniform can't hold a runtime-sized array"));
            }
            (BindingKind::Uniform, None)
                if b.size == 0 || b.size > limits::MAX_UNIFORM_BINDING_SIZE =>
            {
                return Err(problem("a uniform's size must be 1 to 65536 bytes"));
            }
            (_, None) if b.size == 0 => return Err(problem("a fixed-size binding can't be empty")),
            _ => {}
        }
        // Version 0's DRAW binds one thing: its inline uniform, at group 0, binding 0.
        if matches!(pipeline, Pipeline::Render(_))
            && !(b.group == 0 && b.binding == 0 && b.kind == BindingKind::Uniform)
        {
            return Err(problem(
                "a version 0 render pipeline takes only a uniform at group 0, binding 0",
            ));
        }
    }
    Ok(())
}

fn check_layout(layout: &Layout) -> Result<(), String> {
    if layout.name.is_empty() {
        return Err("a layout needs a name".to_string());
    }
    if !layout.align.is_power_of_two() || layout.align < 4 {
        return Err(format!(
            "alignment {} isn't a power of two of at least 4",
            layout.align
        ));
    }
    if layout.size == 0 || !layout.size.is_multiple_of(layout.align) {
        return Err(format!(
            "size {} isn't a positive multiple of the alignment {}",
            layout.size, layout.align
        ));
    }
    let mut end = 0u32;
    let mut names = BTreeSet::new();
    for field in &layout.fields {
        if field.name.is_empty() || field.ty.is_empty() {
            return Err("every field needs a name and a type".to_string());
        }
        if !names.insert(field.name.as_str()) {
            return Err(format!("field `{}` appears twice", field.name));
        }
        if field.size == 0 || !field.offset.is_multiple_of(4) || !field.size.is_multiple_of(4) {
            return Err(format!(
                "field `{}`: offsets and sizes are multiples of 4, and sizes positive",
                field.name
            ));
        }
        if field.offset < end {
            return Err(format!(
                "field `{}` at offset {} overlaps the field before it or is out of order",
                field.name, field.offset
            ));
        }
        end = match field.offset.checked_add(field.size) {
            Some(end) if end <= layout.size => end,
            _ => {
                return Err(format!(
                    "field `{}` runs past the end of the {}-byte layout",
                    field.name, layout.size
                ));
            }
        };
    }
    Ok(())
}

/// Why a manifest was rejected.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ManifestError {
    /// Not JSON, or not this shape: a missing, unknown or mistyped field.
    Json(String),
    UnsupportedVersion {
        found: u16,
    },
    /// A file path that isn't a safe relative path with the right extension.
    Path {
        path: String,
        reason: &'static str,
    },
    DuplicatePipeline {
        id: u32,
    },
    Pipeline {
        id: u32,
        problem: String,
    },
    Binding {
        pipeline: u32,
        group: u32,
        binding: u32,
        problem: String,
    },
    DuplicateLayout {
        name: String,
    },
    Layout {
        name: String,
        problem: String,
    },
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Json(message) => write!(f, "the manifest isn't valid: {message}"),
            ManifestError::UnsupportedVersion { found } => write!(
                f,
                "manifest version {found} isn't supported; this host reads version {VERSION}"
            ),
            ManifestError::Path { path, reason } => {
                write!(f, "the manifest names the file `{path}`: {reason}")
            }
            ManifestError::DuplicatePipeline { id } => {
                write!(f, "two pipelines have the id {id}")
            }
            ManifestError::Pipeline { id, problem } => write!(f, "pipeline {id}: {problem}"),
            ManifestError::Binding {
                pipeline,
                group,
                binding,
                problem,
            } => write!(
                f,
                "pipeline {pipeline}, group {group}, binding {binding}: {problem}"
            ),
            ManifestError::DuplicateLayout { name } => {
                write!(f, "two layouts are named `{name}`")
            }
            ManifestError::Layout { name, problem } => write!(f, "layout `{name}`: {problem}"),
        }
    }
}

impl Error for ManifestError {}

/// Why a decoded command doesn't fit the manifest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommandError {
    UnknownPipeline {
        id: u32,
    },
    NotRender {
        id: u32,
    },
    /// The uniform bytes don't match the size of the pipeline's inline uniform.
    UniformSize {
        id: u32,
        expected: u32,
        found: usize,
    },
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandError::UnknownPipeline { id } => {
                write!(
                    f,
                    "a DRAW names pipeline {id}, which the manifest doesn't have"
                )
            }
            CommandError::NotRender { id } => {
                write!(f, "a DRAW names pipeline {id}, which is a compute pipeline")
            }
            CommandError::UniformSize {
                id,
                expected,
                found,
            } => write!(
                f,
                "a DRAW with pipeline {id} carries {found} bytes of uniforms; the pipeline takes {expected}"
            ),
        }
    }
}

impl Error for CommandError {}

#[cfg(test)]
mod tests;
