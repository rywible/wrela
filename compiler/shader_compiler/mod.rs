pub mod layout;
pub mod lower;
pub mod types;
pub mod wgsl_codegen;

use std::collections::HashSet;

pub use lower::{lower_to_wgsl_v2, shader_program_fingerprint};
pub use types::{ShaderBindingV1, ShaderEntryPointV1, ShaderProgramIRV1, ShaderStageV1};

const WGSL_RESERVED_IDENTIFIERS: &[&str] = &[
    "fn", "var", "struct", "let", "return", "if", "else", "for", "while", "loop", "break",
    "continue", "switch", "case", "default", "discard", "const", "override", "type", "alias",
];

pub(crate) fn canonical_wgsl_binding_ident(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().max(1));
    for (index, ch) in raw.chars().enumerate() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_';
        if !valid {
            out.push('_');
            continue;
        }

        if index == 0 && ch.is_ascii_digit() {
            out.push('_');
            out.push(ch);
            continue;
        }

        out.push(ch);
    }

    if out.is_empty() {
        out.push('_');
    }

    if WGSL_RESERVED_IDENTIFIERS.contains(&out.as_str()) {
        return format!("{out}_binding");
    }

    out
}

fn is_valid_wgsl_identifier(raw: &str) -> bool {
    if raw.is_empty() {
        return false;
    }
    if WGSL_RESERVED_IDENTIFIERS.contains(&raw) {
        return false;
    }
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub fn validate_shader_program(program: &ShaderProgramIRV1) -> Result<(), String> {
    if program.schema_version != 1 {
        return Err(format!(
            "unsupported schema_version {}, expected 1",
            program.schema_version
        ));
    }

    if program.kind != "shader_program" {
        return Err(format!(
            "invalid kind '{}', expected 'shader_program'",
            program.kind
        ));
    }

    if program.program_id.trim().is_empty() {
        return Err("program_id must not be empty".to_string());
    }

    let mut binding_ids: HashSet<&str> = HashSet::with_capacity(program.bindings.len());
    let mut slots: HashSet<(u32, u32)> = HashSet::with_capacity(program.bindings.len());
    let mut emitted_binding_idents: HashSet<String> =
        HashSet::with_capacity(program.bindings.len());

    for binding in &program.bindings {
        if binding.id.trim().is_empty() {
            return Err("binding id must not be empty".to_string());
        }

        if !binding_ids.insert(binding.id.as_str()) {
            return Err(format!("duplicate binding id: {}", binding.id));
        }
        let canonical_ident = canonical_wgsl_binding_ident(binding.id.as_str());
        if !emitted_binding_idents.insert(canonical_ident.clone()) {
            return Err(format!(
                "binding ids collide after WGSL canonicalization: '{}'",
                canonical_ident
            ));
        }

        if !slots.insert((binding.group, binding.binding)) {
            return Err(format!(
                "duplicate binding slot: group {} binding {}",
                binding.group, binding.binding
            ));
        }

        if binding.resource_kind.trim().is_empty() {
            return Err(format!("binding '{}' has empty resource_kind", binding.id));
        }
    }

    let mut entry_ids: HashSet<&str> = HashSet::with_capacity(program.entry_points.len());
    let mut entry_fn_names: HashSet<&str> = HashSet::with_capacity(program.entry_points.len());
    let mut stage_presence: HashSet<ShaderStageV1> =
        HashSet::with_capacity(program.entry_points.len());

    for entry in &program.entry_points {
        if entry.id.trim().is_empty() {
            return Err("entry point id must not be empty".to_string());
        }

        if !entry_ids.insert(entry.id.as_str()) {
            return Err(format!("duplicate entry point id: {}", entry.id));
        }

        if entry.function_name.trim().is_empty() {
            return Err(format!(
                "entry point '{}' has empty function_name",
                entry.id
            ));
        }
        if !is_valid_wgsl_identifier(entry.function_name.as_str()) {
            return Err(format!(
                "entry point '{}' has invalid WGSL function_name '{}'",
                entry.id, entry.function_name
            ));
        }

        if !entry_fn_names.insert(entry.function_name.as_str()) {
            return Err(format!(
                "duplicate entry point function name: {}",
                entry.function_name
            ));
        }

        if !stage_presence.insert(entry.stage) {
            return Err(format!(
                "duplicate entry point stage declared: {:?}",
                entry.stage
            ));
        }
    }

    if program.entry_points.is_empty() {
        return Err("shader program must declare at least one entry point".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_shader_program;
    use crate::shader_compiler::types::{
        ShaderBindingV1, ShaderEntryPointV1, ShaderProgramIRV1, ShaderStageV1,
    };

    fn base_program() -> ShaderProgramIRV1 {
        ShaderProgramIRV1 {
            schema_version: 1,
            kind: "shader_program".to_string(),
            program_id: "prog_1".to_string(),
            bindings: vec![ShaderBindingV1 {
                id: "camera".to_string(),
                group: 0,
                binding: 0,
                resource_kind: "uniform_buffer".to_string(),
                stage: ShaderStageV1::Vertex,
            }],
            entry_points: vec![ShaderEntryPointV1 {
                id: "main_vert".to_string(),
                function_name: "main_vert".to_string(),
                stage: ShaderStageV1::Vertex,
            }],
        }
    }

    #[test]
    fn rejects_duplicate_binding_ids() {
        let mut program = base_program();
        program.bindings.push(ShaderBindingV1 {
            id: "camera".to_string(),
            group: 0,
            binding: 1,
            resource_kind: "uniform_buffer".to_string(),
            stage: ShaderStageV1::Fragment,
        });

        let err =
            validate_shader_program(&program).expect_err("expected duplicate binding id error");
        assert!(
            err.contains("duplicate binding id"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_duplicate_binding_slots() {
        let mut program = base_program();
        program.bindings.push(ShaderBindingV1 {
            id: "material".to_string(),
            group: 0,
            binding: 0,
            resource_kind: "uniform_buffer".to_string(),
            stage: ShaderStageV1::Fragment,
        });

        let err =
            validate_shader_program(&program).expect_err("expected duplicate binding slot error");
        assert!(
            err.contains("duplicate binding slot"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_duplicate_entry_point_stages() {
        let mut program = base_program();
        program.entry_points.push(ShaderEntryPointV1 {
            id: "main_vert_2".to_string(),
            function_name: "main_vert_2".to_string(),
            stage: ShaderStageV1::Vertex,
        });

        let err = validate_shader_program(&program)
            .expect_err("expected duplicate entry point stage error");
        assert!(
            err.contains("duplicate entry point stage"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_missing_entry_points() {
        let mut program = base_program();
        program.entry_points.clear();

        let err =
            validate_shader_program(&program).expect_err("expected missing entry point error");
        assert!(
            err.contains("at least one entry point"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_invalid_wgsl_function_name() {
        let mut program = base_program();
        program.entry_points[0].function_name = "main-frag".to_string();

        let err = validate_shader_program(&program)
            .expect_err("expected invalid wgsl function_name error");
        assert!(
            err.contains("invalid WGSL function_name"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_binding_ids_that_collide_after_canonicalization() {
        let mut program = base_program();
        program.bindings[0].id = "camera-view".to_string();
        program.bindings.push(ShaderBindingV1 {
            id: "camera_view".to_string(),
            group: 1,
            binding: 0,
            resource_kind: "uniform_buffer".to_string(),
            stage: ShaderStageV1::Fragment,
        });

        let err = validate_shader_program(&program)
            .expect_err("expected canonical binding id collision error");
        assert!(
            err.contains("collide after WGSL canonicalization"),
            "unexpected error: {err}"
        );
    }
}
