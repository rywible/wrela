use super::{expect_block_intro, parse_block, parse_param_list, types};
use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn func_def(p: &mut Parser) {
    let m = p.start();
    parse_func_attributes(p);
    parse_func_signature_and_block(p, SyntaxKind::FnKw, false);
    m.complete(p, SyntaxKind::FuncDef);
}

pub fn system_def(p: &mut Parser) {
    let m = p.start();
    parse_func_attributes(p);
    parse_func_signature_and_block(p, SyntaxKind::SystemKw, true);
    m.complete(p, SyntaxKind::SystemDef);
}

pub fn view_def(p: &mut Parser) {
    let m = p.start();
    parse_func_attributes(p);
    parse_func_signature_and_block(p, SyntaxKind::ViewKw, false);
    m.complete(p, SyntaxKind::ViewDef);
}

pub fn material_def(p: &mut Parser) {
    let m = p.start();
    parse_func_attributes(p);
    parse_material_declaration(p);
    m.complete(p, SyntaxKind::MaterialDef);
}

pub fn anim_def(p: &mut Parser) {
    let m = p.start();
    parse_func_attributes(p);
    parse_func_signature_and_block(p, SyntaxKind::AnimKw, false);
    m.complete(p, SyntaxKind::AnimDef);
}

pub fn gpu_func_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(SyntaxKind::GpuKw, "expected 'gpu' before gpu function");
    parse_func_signature_and_block(p, SyntaxKind::FnKw, false);
    m.complete(p, SyntaxKind::GpuFuncDef);
}

pub fn attributed_func_or_check_def(p: &mut Parser) -> bool {
    if !p.at(SyntaxKind::At) {
        return false;
    }
    let m = p.start();
    parse_func_attributes(p);
    if p.at(SyntaxKind::FnKw)
        || p.at(SyntaxKind::SystemKw)
        || p.at(SyntaxKind::ViewKw)
        || p.at(SyntaxKind::MaterialKw)
        || p.at(SyntaxKind::AnimKw)
        || p.at(SyntaxKind::GpuKw)
    {
        let (head, node_kind, allow_metadata) = if p.at(SyntaxKind::GpuKw) {
            p.bump();
            (SyntaxKind::FnKw, SyntaxKind::GpuFuncDef, false)
        } else if p.at(SyntaxKind::SystemKw) {
            (SyntaxKind::SystemKw, SyntaxKind::SystemDef, true)
        } else if p.at(SyntaxKind::ViewKw) {
            (SyntaxKind::ViewKw, SyntaxKind::ViewDef, false)
        } else if p.at(SyntaxKind::MaterialKw) {
            parse_material_declaration(p);
            m.complete(p, SyntaxKind::MaterialDef);
            return true;
        } else if p.at(SyntaxKind::AnimKw) {
            (SyntaxKind::AnimKw, SyntaxKind::AnimDef, false)
        } else {
            (SyntaxKind::FnKw, SyntaxKind::FuncDef, false)
        };
        parse_func_signature_and_block(p, head, allow_metadata);
        m.complete(p, node_kind);
        return true;
    }
    if p.at(SyntaxKind::CheckKw) {
        p.error_with_message_no_bump("`check` was removed; declare `fn ... -> Boolean` instead");
        p.bump();
        p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Eof]);
        if p.at(SyntaxKind::Newline) {
            p.bump();
        }
        m.complete(p, SyntaxKind::Error);
        return true;
    }
    p.error_with_message_no_bump("expected `fn` after attributes");
    m.complete(p, SyntaxKind::Error);
    true
}

pub fn shader_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::ShaderKw,
        "expected 'shader' to start a shader definition",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected shader name after 'shader'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after shader name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after shader parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after shader signature");
    parse_block(p);
    m.complete(p, SyntaxKind::ShaderDef);
}

fn parse_func_attributes(p: &mut Parser) {
    while p.at(SyntaxKind::At) {
        let m = p.start();
        p.bump();
        let attr_name = if p.at(SyntaxKind::Ident) || p.at(SyntaxKind::ShaderKw) {
            Some(p.peek_text().to_string())
        } else {
            None
        };
        if p.at(SyntaxKind::ShaderKw) {
            p.bump(); // treat `shader` keyword as attribute name
        } else {
            p.expect_with_message(SyntaxKind::Ident, "expected attribute name after '@'");
        }
        if let Some(name) = attr_name
            && let Some(message) = legacy_render_annotation_message(name.as_str())
        {
            p.error_with_message_no_bump(message);
        }
        if p.at(SyntaxKind::LParen) {
            parse_attribute_args(p);
        }
        m.complete(p, SyntaxKind::Attribute);
        p.expect_stmt_boundary();
    }
}

fn parse_attribute_args(p: &mut Parser) {
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after attribute name");
    let mut first = true;
    while !p.at(SyntaxKind::RParen) && !p.is_at_eof() {
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RParen]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RParen) {
                    break;
                }
            }
        }
        p.expect_with_message(SyntaxKind::Ident, "expected attribute argument name");
        p.expect_with_message(
            SyntaxKind::Equals,
            "expected '=' after attribute argument name",
        );
        parse_attribute_arg_value(p);
        first = false;
    }
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after attribute arguments");
}

fn parse_attribute_arg_value(p: &mut Parser) {
    if is_attribute_arg_value_start(p.peek()) {
        p.bump();
        return;
    }
    p.error_with_message_no_bump("expected attribute argument value");
}

fn parse_func_signature_and_block(p: &mut Parser, head: SyntaxKind, allow_metadata: bool) {
    p.expect_with_message(head, expected_head_error(head));
    p.expect_with_message(SyntaxKind::Ident, expected_name_error(head));
    if allow_metadata {
        parse_system_metadata(p);
    } else if p.at(SyntaxKind::LBracket) {
        // Parse optional type parameters: fn foo[T, U: Bound](...)
        let m = p.start();
        p.bump(); // consume [
        let mut first = true;
        while !p.at(SyntaxKind::RBracket) && !p.is_at_eof() {
            if !first {
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else {
                    break;
                }
            }
            p.expect_with_message(SyntaxKind::Ident, "expected type parameter name");
            // Optional bound: T: BoundName
            if p.at(SyntaxKind::Colon) {
                p.bump();
                p.expect_with_message(SyntaxKind::Ident, "expected bound name after ':'");
            }
            first = false;
        }
        p.expect_with_message(SyntaxKind::RBracket, "expected ']' after type parameters");
        m.complete(p, SyntaxKind::TypeParamList);
    }
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after function name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after function parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    expect_block_intro(p, "expected '{' after function signature");
    parse_block(p);
}

fn parse_material_declaration(p: &mut Parser) {
    p.expect_with_message(
        SyntaxKind::MaterialKw,
        "expected 'material' to start a material declaration",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected material name after 'material'");

    if p.at(SyntaxKind::LParen) {
        parse_legacy_material_signature_and_block(p);
        return;
    }

    expect_block_intro(p, "expected '{' after material declaration name");
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("surface_model") {
                parse_material_surface_model_clause(p);
                continue;
            }
            if p.at(SyntaxKind::PresetKw) || p.at_ident_text("preset") {
                parse_material_preset_clause(p);
                continue;
            }
            if p.at_ident_text("textures") {
                parse_material_textures_clause(p);
                continue;
            }
            if p.at_ident_text("params") {
                parse_material_params_clause(p);
                continue;
            }
            if p.at_ident_text("features") {
                parse_material_features_clause(p);
                continue;
            }
            if p.at_ident_text("semantics") {
                parse_material_semantics_clause(p);
                continue;
            }
            if p.at(SyntaxKind::RenderKw) {
                parse_material_render_clause(p);
                continue;
            }

            p.error_with_message_no_bump(
                "expected material clause (`surface_model`, `preset`, `textures`, `params`, `features`, `semantics`, or `render ...`)",
            );
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace, SyntaxKind::Eof]);
            if p.at(SyntaxKind::Newline) {
                p.bump();
            }
        }
        p.expect(SyntaxKind::RBrace);
    }
    p.expect_stmt_boundary();
}

fn parse_legacy_material_signature_and_block(p: &mut Parser) {
    p.error_with_message_no_bump(
        "legacy material function declarations were removed; use `material <Name> { surface_model <id> ... }`",
    );

    if p.at(SyntaxKind::LParen) {
        p.bump();
        p.recover_until(&[SyntaxKind::RParen, SyntaxKind::LBrace, SyntaxKind::Eof]);
        if p.at(SyntaxKind::RParen) {
            p.bump();
        }
    }
    if p.at(SyntaxKind::Arrow) {
        p.bump();
        types::parse_type(p);
    }
    if p.at(SyntaxKind::Colon) {
        p.bump();
    }

    expect_block_intro(p, "expected '{' after material declaration name");
    parse_block(p);
}

fn parse_material_surface_model_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("surface_model") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `surface_model` clause");
    }
    parse_material_clause_value(p, "expected surface model id after `surface_model`");
    m.complete(p, SyntaxKind::MaterialSurfaceModelClause);
    p.expect_stmt_boundary();
}

fn parse_material_preset_clause(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::PresetKw) || p.at_ident_text("preset") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `preset` clause");
    }
    parse_material_clause_value(p, "expected preset id after `preset`");
    m.complete(p, SyntaxKind::MaterialPresetClause);
    p.expect_stmt_boundary();
}

fn parse_material_textures_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("textures") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `textures` clause");
    }
    parse_material_clause_value(p, "expected texture slot after `textures`");
    parse_material_clause_value(p, "expected texture path or id after texture slot");
    m.complete(p, SyntaxKind::MaterialTexturesClause);
    p.expect_stmt_boundary();
}

fn parse_material_params_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("params") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `params` clause");
    }
    if is_material_identifier_like_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected parameter name after `params`");
    }
    if matches!(p.peek(), SyntaxKind::IntNumber | SyntaxKind::FloatNumber) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected numeric parameter value after `params <name>`");
    }
    m.complete(p, SyntaxKind::MaterialParamsClause);
    p.expect_stmt_boundary();
}

fn parse_material_features_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("features") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `features` clause");
    }
    if is_material_identifier_like_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected feature name after `features`");
    }
    if matches!(p.peek(), SyntaxKind::TrueKw | SyntaxKind::FalseKw)
        || is_material_identifier_like_start(p.peek())
    {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected boolean or feature flag id after `features <name>`");
    }
    m.complete(p, SyntaxKind::MaterialFeaturesClause);
    p.expect_stmt_boundary();
}

fn parse_material_semantics_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("semantics") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `semantics` clause");
    }
    if is_material_identifier_like_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected semantic key after `semantics`");
    }
    parse_material_clause_value(p, "expected semantic value after `semantics <key>`");
    m.complete(p, SyntaxKind::MaterialSemanticsClause);
    p.expect_stmt_boundary();
}

fn parse_material_render_clause(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(SyntaxKind::RenderKw, "expected `render` clause");
    if p.at_ident_text("alpha") {
        p.bump();
        parse_material_clause_value(p, "expected alpha mode after `render alpha`");
        m.complete(p, SyntaxKind::MaterialRenderAlphaClause);
        p.expect_stmt_boundary();
        return;
    }
    if p.at_ident_text("double_sided") {
        p.bump();
        if matches!(p.peek(), SyntaxKind::TrueKw | SyntaxKind::FalseKw) {
            p.bump();
        } else {
            p.error_with_message_no_bump("expected boolean value after `render double_sided`");
        }
        m.complete(p, SyntaxKind::MaterialRenderDoubleSidedClause);
        p.expect_stmt_boundary();
        return;
    }
    if p.at_ident_text("receives_decals") {
        p.bump();
        if matches!(p.peek(), SyntaxKind::TrueKw | SyntaxKind::FalseKw) {
            p.bump();
        } else {
            p.error_with_message_no_bump("expected boolean value after `render receives_decals`");
        }
        m.complete(p, SyntaxKind::MaterialRenderReceivesDecalsClause);
        p.expect_stmt_boundary();
        return;
    }

    p.error_with_message_no_bump(
        "expected render material clause (`alpha`, `double_sided`, or `receives_decals`)",
    );
    p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace, SyntaxKind::Eof]);
    if p.at(SyntaxKind::Newline) {
        p.bump();
    }
    m.complete(p, SyntaxKind::Error);
}

fn parse_material_clause_value(p: &mut Parser, message: &str) {
    if is_material_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump(message);
    }
}

fn is_material_clause_value_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Ident
            | SyntaxKind::StringLiteral
            | SyntaxKind::IntNumber
            | SyntaxKind::FloatNumber
            | SyntaxKind::TrueKw
            | SyntaxKind::FalseKw
    ) || is_material_identifier_like_start(kind)
}

fn is_material_identifier_like_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Ident
            | SyntaxKind::RenderKw
            | SyntaxKind::PresetKw
            | SyntaxKind::ProfileKw
            | SyntaxKind::OverridesKw
            | SyntaxKind::MaterialKw
            | SyntaxKind::GpuKw
            | SyntaxKind::AssetsKw
            | SyntaxKind::MmoKw
            | SyntaxKind::AssetSpecKw
            | SyntaxKind::StyleProfileKw
            | SyntaxKind::GeneratorProfileKw
            | SyntaxKind::QualityProfileKw
            | SyntaxKind::ProvenancePolicyKw
            | SyntaxKind::CharacterSpecKw
            | SyntaxKind::RigSpecKw
            | SyntaxKind::AnimSetSpecKw
            | SyntaxKind::AudioSpecKw
            | SyntaxKind::VfxSpecKw
            | SyntaxKind::UiSpecKw
            | SyntaxKind::WorldRecipeKw
    )
}

fn expected_head_error(head: SyntaxKind) -> &'static str {
    match head {
        SyntaxKind::FnKw => "expected 'fn' to start a function definition",
        SyntaxKind::SystemKw => "expected 'system' to start a system declaration",
        SyntaxKind::ViewKw => "expected 'view' to start a view declaration",
        SyntaxKind::MaterialKw => "expected 'material' to start a material declaration",
        SyntaxKind::AnimKw => "expected 'anim' to start an anim declaration",
        _ => "expected declaration keyword",
    }
}

fn expected_name_error(head: SyntaxKind) -> &'static str {
    match head {
        SyntaxKind::FnKw => "expected function name after 'fn'",
        SyntaxKind::SystemKw => "expected system name after 'system'",
        SyntaxKind::ViewKw => "expected view name after 'view'",
        SyntaxKind::MaterialKw => "expected material name after 'material'",
        SyntaxKind::AnimKw => "expected anim name after 'anim'",
        _ => "expected declaration name",
    }
}

fn parse_system_metadata(p: &mut Parser) {
    if !p.at(SyntaxKind::LBracket) {
        return;
    }
    p.bump();
    let mut first = true;
    while !p.at(SyntaxKind::RBracket) && !p.is_at_eof() {
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RBracket]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RBracket) {
                    break;
                }
            }
        }
        p.expect_with_message(SyntaxKind::Ident, "expected system metadata key");
        p.expect_with_message(SyntaxKind::Equals, "expected '=' after metadata key");
        parse_metadata_value(p);
        first = false;
    }
    p.expect_with_message(SyntaxKind::RBracket, "expected ']' after system metadata");
}

fn parse_metadata_value(p: &mut Parser) {
    if p.at(SyntaxKind::LBracket) {
        parse_metadata_list(p);
        return;
    }
    if is_metadata_value_start(p.peek()) {
        p.bump();
        return;
    }
    p.error_with_message_no_bump("expected system metadata value");
}

fn parse_metadata_list(p: &mut Parser) {
    p.expect(SyntaxKind::LBracket);
    let mut first = true;
    while !p.at(SyntaxKind::RBracket) && !p.is_at_eof() {
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RBracket]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RBracket) {
                    break;
                }
            }
        }
        if is_metadata_value_start(p.peek()) {
            p.bump();
        } else {
            p.error_with_message_no_bump("expected metadata list item");
            break;
        }
        first = false;
    }
    p.expect_with_message(SyntaxKind::RBracket, "expected ']' after metadata list");
}

fn is_attribute_arg_value_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Ident
            | SyntaxKind::StringLiteral
            | SyntaxKind::IntNumber
            | SyntaxKind::FloatNumber
            | SyntaxKind::TrueKw
            | SyntaxKind::FalseKw
            | SyntaxKind::RenderKw
            | SyntaxKind::PresetKw
            | SyntaxKind::ProfileKw
            | SyntaxKind::OverridesKw
            | SyntaxKind::GpuKw
    )
}

fn is_metadata_value_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Ident
            | SyntaxKind::StringLiteral
            | SyntaxKind::RenderKw
            | SyntaxKind::PresetKw
            | SyntaxKind::ProfileKw
            | SyntaxKind::OverridesKw
            | SyntaxKind::GpuKw
    )
}

fn legacy_render_annotation_message(attr_name: &str) -> Option<&'static str> {
    match attr_name {
        "shader" => Some(
            "legacy render annotation `@shader` was removed; use `render <Name> { resources <AssetsDeclaration> temporal <mode> quality tier <tier> budget tags <tag>[, <tag>...] }` and `gpu fn`",
        ),
        "pipeline" => Some(
            "legacy render annotation `@pipeline` was removed; use `render <Name> { resources <AssetsDeclaration> temporal <mode> quality tier <tier> budget tags <tag>[, <tag>...] }` and `gpu fn`",
        ),
        "pass" => Some(
            "legacy render annotation `@pass` was removed; use `render <Name> { resources <AssetsDeclaration> temporal <mode> quality tier <tier> budget tags <tag>[, <tag>...] }` and `gpu fn`",
        ),
        _ => None,
    }
}
