use super::{expect_block_intro, expr, parse_block, parse_param_list, types};
use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn component_def(p: &mut Parser) {
    class_like_def(
        p,
        SyntaxKind::ComponentKw,
        SyntaxKind::ComponentDef,
        "expected type name after 'component'",
    );
}

pub fn resource_def(p: &mut Parser) {
    class_like_def(
        p,
        SyntaxKind::ResourceKw,
        SyntaxKind::ResourceDef,
        "expected type name after 'resource'",
    );
}

pub fn event_def(p: &mut Parser) {
    class_like_def(
        p,
        SyntaxKind::EventKw,
        SyntaxKind::EventDef,
        "expected type name after 'event'",
    );
}

pub fn scene_def(p: &mut Parser) {
    class_like_def(
        p,
        SyntaxKind::SceneKw,
        SyntaxKind::SceneDef,
        "expected type name after 'scene'",
    );
}

pub fn theme_def(p: &mut Parser) {
    class_like_def(
        p,
        SyntaxKind::ThemeKw,
        SyntaxKind::ThemeDef,
        "expected type name after 'theme'",
    );
}

pub fn node_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::NodeKw,
        "expected 'node' to start a node declaration",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected type name after 'node'");
    parse_type_params(p);
    parse_node_profile_clause(p);

    if p.at(SyntaxKind::DerivesKw) {
        p.error_with_message_no_bump(
            "derive traits were removed; semantics are structural by default",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::LBrace, SyntaxKind::Newline]);
    }

    parse_class_body(p);
    m.complete(p, SyntaxKind::NodeDef);
}

fn class_like_def(
    p: &mut Parser,
    keyword: SyntaxKind,
    node_kind: SyntaxKind,
    missing_name_message: &'static str,
) {
    let m = p.start();
    p.expect_with_message(keyword, "expected declaration keyword");
    p.expect_with_message(SyntaxKind::Ident, missing_name_message);
    parse_type_params(p);

    if p.at(SyntaxKind::DerivesKw) {
        p.error_with_message_no_bump(
            "derive traits were removed; semantics are structural by default",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::LBrace, SyntaxKind::Newline]);
    }

    parse_class_body(p);
    m.complete(p, node_kind);
}

fn parse_node_profile_clause(p: &mut Parser) {
    let m = p.start();
    if !p.at(SyntaxKind::ProfileKw) {
        p.error_with_message_no_bump("node declarations require `profile ui|world|canvas`");
        m.complete(p, SyntaxKind::NodeProfileClause);
        return;
    }
    p.bump();

    if p.at_ident_text("ui") || p.at_ident_text("world") || p.at_ident_text("canvas") {
        p.bump();
    } else {
        p.error_with_message_no_bump("node profile must be one of `ui`, `world`, or `canvas`");
        if p.at(SyntaxKind::Ident) {
            p.bump();
        }
    }

    m.complete(p, SyntaxKind::NodeProfileClause);
}

pub fn class_def(p: &mut Parser) {
    if p.at(SyntaxKind::EnumKw) {
        enum_def(p);
        return;
    }

    let m = p.start();
    let is_interface = p.at(SyntaxKind::InterfaceKw);
    if p.at(SyntaxKind::ClassKw) || is_interface {
        p.bump();
    } else {
        p.error_with_message(
            "expected 'class' or 'interface' to start a type definition",
            true,
        );
    }

    p.expect_with_message(
        SyntaxKind::Ident,
        "expected type name after 'class'/'interface'",
    );
    parse_type_params(p);

    if p.at(SyntaxKind::DerivesKw) {
        p.error_with_message_no_bump(
            "derive traits were removed; semantics are structural by default",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::LBrace, SyntaxKind::Newline]);
    }

    parse_class_body(p);

    if is_interface {
        // Keep interfaces on the same node kind for now; semantic passes distinguish by body shape.
    }
    m.complete(p, SyntaxKind::ClassDef);
}

fn parse_class_body(p: &mut Parser) {
    expect_block_intro(p, "expected '{' after type declaration");

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        if p.at(SyntaxKind::IsKw) {
            parse_is_a_clause(p);
        }
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            if is_field_start(p) {
                field_def(p);
                p.expect_stmt_boundary();
            } else if is_class_item_start(p) {
                if p.at(SyntaxKind::FnKw) {
                    method_def(p);
                } else if p.at(SyntaxKind::MustKw) {
                    must_method_def(p);
                } else if p.at(SyntaxKind::DerivesKw) {
                    p.error_with_message_no_bump("removed keyword `derives`; use `fn` for methods");
                    p.bump();
                } else if p.at(SyntaxKind::PrivateKw) {
                    parse_private_block(p);
                }
            } else if p.at(SyntaxKind::CanKw)
                || p.at(SyntaxKind::ChecksKw)
                || p.at(SyntaxKind::CheckKw)
            {
                p.error_with_message_no_bump("removed keyword in type body; use `fn` for methods");
                p.bump();
            } else {
                p.error();
                p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
                p.expect_stmt_boundary();
            }
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after type declaration");
    }
}

fn enum_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::EnumKw,
        "expected 'enum' to start enum definition",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected enum name after 'enum'");
    parse_type_params(p);

    if p.at(SyntaxKind::DerivesKw) {
        p.error_with_message_no_bump(
            "derive traits were removed; semantics are structural by default",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::LBrace, SyntaxKind::Newline]);
    }

    parse_enum_body(p);
    m.complete(p, SyntaxKind::EnumDef);
}

fn parse_type_params(p: &mut Parser) {
    if !p.at(SyntaxKind::LBracket) {
        return;
    }
    let m = p.start();
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
        p.expect_with_message(SyntaxKind::Ident, "expected type parameter name");
        first = false;
    }
    p.expect_with_message(SyntaxKind::RBracket, "expected ']' after type parameters");
    m.complete(p, SyntaxKind::TypeParamList);
}

fn parse_is_a_clause(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::IsKw);
    if !p.at_ident_text("a") {
        p.error_with_message("expected 'a' after 'is'", true);
    } else {
        p.bump();
    }
    p.expect_with_message(SyntaxKind::Ident, "expected interface name after 'is a'");
    m.complete(p, SyntaxKind::IsAClause);
    p.expect_stmt_boundary();
}

fn parse_enum_body(p: &mut Parser) {
    expect_block_intro(p, "expected '{' after enum declaration");
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            parse_enum_variant(p);
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after enum declaration");
    }
}

fn parse_enum_variant(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(SyntaxKind::Ident, "expected enum variant name");
    if p.at(SyntaxKind::LParen) {
        p.bump();
        parse_param_list(p);
        p.expect_with_message(SyntaxKind::RParen, "expected ')' after variant parameters");
    }
    m.complete(p, SyntaxKind::EnumVariant);
    p.expect_stmt_boundary();
}

fn must_method_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::MustKw,
        "expected 'must' to start an interface method signature",
    );
    if p.at(SyntaxKind::CheckKw) {
        p.error_with_message_no_bump("`check` was removed; use `Boolean` return type instead");
        p.bump();
    }
    p.expect_with_message(SyntaxKind::Ident, "expected method name after 'must'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after method name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after method parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);
    m.complete(p, SyntaxKind::MustMethodDef);
    p.expect_stmt_boundary();
}

fn field_def(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::MutableKw) {
        p.bump();
    }
    p.expect_with_message(SyntaxKind::Ident, "expected field name");
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after field name");
    types::parse_type(p);
    if p.at(SyntaxKind::Equals) {
        p.bump();
        expr::expr(p);
    }
    m.complete(p, SyntaxKind::FieldDef);
}

fn method_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::FnKw,
        "expected 'fn' to start a method definition",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected method name after 'fn'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after method name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after method parameters");

    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    expect_block_intro(p, "expected '{' after method signature");

    parse_block(p);

    m.complete(p, SyntaxKind::MethodDef);
}

fn is_class_item_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::FnKw) || p.at(SyntaxKind::MustKw) {
        return true;
    }
    if p.at(SyntaxKind::PrivateKw) {
        return true;
    }
    false
}

fn is_field_start(p: &Parser) -> bool {
    if p.at(SyntaxKind::Ident) && p.peek_nontrivia_at(1) == SyntaxKind::Colon {
        return true;
    }
    if p.at(SyntaxKind::MutableKw)
        && p.peek_nontrivia_at(1) == SyntaxKind::Ident
        && p.peek_nontrivia_at(2) == SyntaxKind::Colon
    {
        return true;
    }
    false
}

fn parse_private_block(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::PrivateKw);
    expect_block_intro(p, "expected '{' after 'private'");
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            if is_field_start(p) {
                field_def(p);
                p.expect_stmt_boundary();
            } else if p.at(SyntaxKind::FnKw) {
                method_def(p);
            } else if p.at(SyntaxKind::DerivesKw) {
                p.error_with_message_no_bump("removed keyword `derives`; use `fn` for methods");
                p.bump();
            } else if p.at(SyntaxKind::MustKw) {
                must_method_def(p);
            } else {
                p.error();
                p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
                p.expect_stmt_boundary();
            }
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after 'private'");
    }
    m.complete(p, SyntaxKind::PrivateBlock);
}

pub fn asset_decl(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::AssetKw,
        "expected 'asset' to start an asset declaration",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected asset name after 'asset'");
    expect_block_intro(p, "expected '{' after asset name");

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("kind") {
                let cm = p.start();
                p.bump(); // kind
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::Ident) {
                    p.bump();
                } else {
                    p.error_with_message("expected asset kind (mesh, texture, or audio)", true);
                }
                cm.complete(p, SyntaxKind::AssetKindClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("prompt") {
                let cm = p.start();
                p.bump(); // prompt
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::StringLiteral) {
                    p.bump();
                } else {
                    p.error_with_message("expected string literal for prompt", true);
                }
                cm.complete(p, SyntaxKind::AssetPromptClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("style") {
                let cm = p.start();
                p.bump(); // style
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::StringLiteral) {
                    p.bump();
                } else {
                    p.error_with_message("expected string literal for style", true);
                }
                cm.complete(p, SyntaxKind::AssetStyleClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("negative") {
                let cm = p.start();
                p.bump(); // negative
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::StringLiteral) {
                    p.bump();
                } else {
                    p.error_with_message("expected string literal for negative", true);
                }
                cm.complete(p, SyntaxKind::AssetNegativeClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("lod_budget") {
                let cm = p.start();
                p.bump(); // lod_budget
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::IntNumber) {
                    p.bump();
                } else {
                    p.error_with_message("expected integer for lod_budget", true);
                }
                cm.complete(p, SyntaxKind::AssetLodBudgetClause);
                p.expect_stmt_boundary();
                continue;
            }
            p.error_with_message(
                "expected 'kind', 'prompt', 'style', 'negative', or 'lod_budget'",
                true,
            );
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
            p.expect_stmt_boundary();
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after asset name");
    }
    m.complete(p, SyntaxKind::AssetDecl);
}

pub fn scene_decl(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::SceneKw,
        "expected 'scene' to start a scene declaration",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected scene name after 'scene'");
    expect_block_intro(p, "expected '{' after scene name");

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("entity") {
                parse_scene_entity_block(p);
                continue;
            }
            if p.at_ident_text("lighting") {
                parse_scene_lighting_block(p);
                continue;
            }
            if p.at_ident_text("camera") {
                parse_scene_camera_block(p);
                continue;
            }
            p.error_with_message("expected 'entity', 'lighting', or 'camera'", true);
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
            p.expect_stmt_boundary();
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after scene name");
    }
    m.complete(p, SyntaxKind::SceneDef);
}

fn parse_scene_entity_block(p: &mut Parser) {
    let m = p.start();
    p.bump(); // entity
    p.expect_with_message(SyntaxKind::Ident, "expected entity name after 'entity'");
    expect_block_intro(p, "expected '{' after entity name");

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("mesh") {
                let cm = p.start();
                p.bump(); // mesh
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::Ident) {
                    p.bump();
                } else {
                    p.error_with_message("expected asset name for mesh", true);
                }
                cm.complete(p, SyntaxKind::SceneEntityMeshClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("position") {
                let cm = p.start();
                p.bump(); // position
                p.expect(SyntaxKind::Colon);
                parse_vec3_or_rgb_literal(p);
                cm.complete(p, SyntaxKind::SceneEntityPositionClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("rotation") {
                let cm = p.start();
                p.bump(); // rotation
                p.expect(SyntaxKind::Colon);
                parse_vec3_or_rgb_literal(p);
                cm.complete(p, SyntaxKind::SceneEntityRotationClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("scale") {
                let cm = p.start();
                p.bump(); // scale
                p.expect(SyntaxKind::Colon);
                parse_vec3_or_rgb_literal(p);
                cm.complete(p, SyntaxKind::SceneEntityScaleClause);
                p.expect_stmt_boundary();
                continue;
            }
            p.error_with_message("expected 'mesh', 'position', 'rotation', or 'scale'", true);
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
            p.expect_stmt_boundary();
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after entity name");
    }
    m.complete(p, SyntaxKind::SceneEntityBlock);
}

fn parse_scene_lighting_block(p: &mut Parser) {
    let m = p.start();
    p.bump(); // lighting
    expect_block_intro(p, "expected '{' after 'lighting'");

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("sun_direction") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                parse_vec3_or_rgb_literal(p);
                cm.complete(p, SyntaxKind::SceneLightingSunDirectionClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("sun_color") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                parse_vec3_or_rgb_literal(p);
                cm.complete(p, SyntaxKind::SceneLightingSunColorClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("sun_intensity") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::IntNumber) {
                    p.bump();
                } else {
                    p.error_with_message("expected integer for sun_intensity", true);
                }
                cm.complete(p, SyntaxKind::SceneLightingSunIntensityClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("ambient_color") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                parse_vec3_or_rgb_literal(p);
                cm.complete(p, SyntaxKind::SceneLightingAmbientColorClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("ambient_intensity") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::IntNumber) {
                    p.bump();
                } else {
                    p.error_with_message("expected integer for ambient_intensity", true);
                }
                cm.complete(p, SyntaxKind::SceneLightingAmbientIntensityClause);
                p.expect_stmt_boundary();
                continue;
            }
            p.error_with_message(
                "expected 'sun_direction', 'sun_color', 'sun_intensity', 'ambient_color', or 'ambient_intensity'",
                true,
            );
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
            p.expect_stmt_boundary();
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after 'lighting'");
    }
    m.complete(p, SyntaxKind::SceneLightingBlock);
}

fn parse_scene_camera_block(p: &mut Parser) {
    let m = p.start();
    p.bump(); // camera
    expect_block_intro(p, "expected '{' after 'camera'");

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("mode") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::Ident) {
                    p.bump();
                } else {
                    p.error_with_message("expected camera mode identifier", true);
                }
                cm.complete(p, SyntaxKind::SceneCameraModeClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("target") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                if p.at(SyntaxKind::Ident) {
                    p.bump();
                } else {
                    p.error_with_message("expected target entity name", true);
                }
                cm.complete(p, SyntaxKind::SceneCameraTargetClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("distance") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                parse_possibly_negative_int(p);
                cm.complete(p, SyntaxKind::SceneCameraDistanceClause);
                p.expect_stmt_boundary();
                continue;
            }
            if p.at_ident_text("pitch") {
                let cm = p.start();
                p.bump();
                p.expect(SyntaxKind::Colon);
                parse_possibly_negative_int(p);
                cm.complete(p, SyntaxKind::SceneCameraPitchClause);
                p.expect_stmt_boundary();
                continue;
            }
            p.error_with_message("expected 'mode', 'target', 'distance', or 'pitch'", true);
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
            p.expect_stmt_boundary();
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after 'camera'");
    }
    m.complete(p, SyntaxKind::SceneCameraBlock);
}

/// Parse `vec3(x, y, z)` or `rgb(r, g, b)` call syntax.
fn parse_vec3_or_rgb_literal(p: &mut Parser) {
    if p.at(SyntaxKind::Ident) {
        p.bump(); // vec3 or rgb
        p.expect(SyntaxKind::LParen);
        parse_possibly_negative_int(p);
        p.expect(SyntaxKind::Comma);
        parse_possibly_negative_int(p);
        p.expect(SyntaxKind::Comma);
        parse_possibly_negative_int(p);
        p.expect(SyntaxKind::RParen);
    } else {
        p.error_with_message("expected 'vec3(...)' or 'rgb(...)'", true);
    }
}

/// Parse an integer that may optionally be prefixed with a minus sign.
fn parse_possibly_negative_int(p: &mut Parser) {
    if p.at(SyntaxKind::Minus) {
        p.bump();
    }
    if p.at(SyntaxKind::IntNumber) {
        p.bump();
    } else {
        p.error_with_message("expected integer", true);
    }
}
