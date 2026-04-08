use super::{expect_block_intro, expr, parse_block, parse_param_list, parse_statement, types};
use crate::parser::Parser;
use crate::parser::ast::is_name_like_label_token;
use crate::parser::kind::SyntaxKind;

#[derive(Clone, Copy)]
enum FunctionDeclHead {
    Function,
    Kernel,
    System,
}

pub fn func_def(p: &mut Parser) {
    let m = p.start();
    parse_func_attributes(p);
    parse_func_signature_and_block(p, FunctionDeclHead::Function);
    m.complete(p, SyntaxKind::FuncDef);
}

pub fn kernel_def(p: &mut Parser) {
    let m = p.start();
    parse_func_attributes(p);
    parse_func_signature_and_block(p, FunctionDeclHead::Kernel);
    m.complete(p, SyntaxKind::KernelDef);
}

pub fn system_def(p: &mut Parser) {
    let m = p.start();
    parse_func_attributes(p);
    parse_func_signature_and_block(p, FunctionDeclHead::System);
    m.complete(p, SyntaxKind::SystemDef);
}

pub fn field_decl(p: &mut Parser) {
    let m = p.start();
    parse_field_signature_and_block(p);
    m.complete(p, SyntaxKind::FieldDecl);
}

pub fn region_decl(p: &mut Parser) {
    let m = p.start();
    parse_region_signature_and_block(p);
    m.complete(p, SyntaxKind::RegionDecl);
}

pub fn domain_decl(p: &mut Parser) {
    let m = p.start();
    parse_domain_signature_and_block(p);
    m.complete(p, SyntaxKind::DomainDecl);
}

pub fn render_decl(p: &mut Parser) {
    let m = p.start();
    parse_render_signature_and_block(p);
    m.complete(p, SyntaxKind::RenderDecl);
}

pub fn radiance_decl(p: &mut Parser) {
    let m = p.start();
    parse_radiance_signature_and_block(p);
    m.complete(p, SyntaxKind::RadianceDecl);
}

pub fn volume_decl(p: &mut Parser) {
    let m = p.start();
    parse_volume_signature_and_block(p);
    m.complete(p, SyntaxKind::VolumeDecl);
}

pub fn material_decl(p: &mut Parser) {
    let m = p.start();
    parse_material_signature_and_block(p);
    m.complete(p, SyntaxKind::MaterialDecl);
}

pub fn shape_decl(p: &mut Parser) {
    let m = p.start();
    parse_shape_signature_and_block(p);
    m.complete(p, SyntaxKind::ShapeDecl);
}

pub fn attributed_func_or_check_def(p: &mut Parser) -> bool {
    if !p.at(SyntaxKind::At) {
        return false;
    }
    let m = p.start();
    parse_func_attributes(p);
    if p.at(SyntaxKind::FnKw) || p.at(SyntaxKind::KernelKw) || p.at(SyntaxKind::SystemKw) {
        let (head, node_kind) = if p.at(SyntaxKind::KernelKw) {
            (FunctionDeclHead::Kernel, SyntaxKind::KernelDef)
        } else if p.at(SyntaxKind::SystemKw) {
            (FunctionDeclHead::System, SyntaxKind::SystemDef)
        } else {
            (FunctionDeclHead::Function, SyntaxKind::FuncDef)
        };
        parse_func_signature_and_block(p, head);
        m.complete(p, node_kind);
        return true;
    }
    p.error_with_message_no_bump("expected `fn`, `kernel fn`, or `system` after attributes");
    m.complete(p, SyntaxKind::Error);
    true
}

fn parse_func_attributes(p: &mut Parser) {
    while p.at(SyntaxKind::At) {
        let m = p.start();
        p.bump();
        p.expect_with_message(SyntaxKind::Ident, "expected attribute name after '@'");
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

fn parse_func_signature_and_block(p: &mut Parser, head: FunctionDeclHead) {
    parse_function_decl_head(p, head);
    p.expect_with_message(SyntaxKind::Ident, expected_name_error(head));
    if matches!(head, FunctionDeclHead::System) {
        parse_system_metadata(p);
    } else {
        parse_optional_type_params(p);
    }
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after function name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after function parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    expect_block_intro(p, "expected '{' after function signature");
    parse_block(p);
}

fn parse_field_signature_and_block(p: &mut Parser) {
    expect_ident_text(p, "field", "expected 'field' to start a field declaration");
    parse_field_class(p);
    expect_ident_text(p, "distance", "expected 'distance' after field class");
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected field name after 'field exact distance'",
    );
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after field name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after field parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    parse_field_body(p);
}

fn parse_field_body(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else if p.at(SyntaxKind::Colon) {
        p.error_with_message_no_bump("':' block introducer was removed; use braces: `{ ... }`");
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' after field signature");
    }

    p.consume_trivia();
    while parse_field_clause(p) {
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
        p.consume_trivia();
    }
    if is_field_semantic_start(p) {
        parse_field_expr(p);
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
        p.expect_with_message(
            SyntaxKind::RBrace,
            "expected '}' after field composition expression",
        );
        m.complete(p, SyntaxKind::Block);
        return;
    }

    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        p.consume_trivia();
        if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
            break;
        }
        let cursor = p.cursor_pos();
        parse_statement(p);
        if p.cursor_pos() == cursor {
            p.error();
        }
    }
    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::Block);
}

fn parse_radiance_signature_and_block(p: &mut Parser) {
    expect_ident_text(
        p,
        "radiance",
        "expected 'radiance' to start a radiance field declaration",
    );
    expect_ident_text(p, "field", "expected 'field' after 'radiance'");
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected radiance field name after 'radiance field'",
    );
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after radiance field name");
    parse_param_list(p);
    p.expect_with_message(
        SyntaxKind::RParen,
        "expected ')' after radiance field parameters",
    );
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    expect_block_intro(p, "expected '{' after radiance field signature");
    parse_block(p);
}

fn parse_region_signature_and_block(p: &mut Parser) {
    expect_ident_text(
        p,
        "region",
        "expected 'region' to start a region declaration",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected region name after 'region'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after region name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after region parameters");

    expect_block_intro(p, "expected '{' after region signature");
    parse_region_body(p);
}

fn parse_domain_signature_and_block(p: &mut Parser) {
    expect_ident_text(
        p,
        "domain",
        "expected 'domain' to start a domain declaration",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected domain name after 'domain'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after domain name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after domain parameters");

    expect_block_intro(p, "expected '{' after domain signature");
    parse_block(p);
}

fn parse_render_signature_and_block(p: &mut Parser) {
    expect_ident_text(
        p,
        "render",
        "expected 'render' to start a render declaration",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected render name after 'render'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after render name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after render parameters");

    expect_block_intro(p, "expected '{' after render signature");
    parse_block(p);
}

fn parse_region_body(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' after region signature");
    }

    p.consume_trivia();
    while parse_region_item(p) {
        p.consume_trivia();
    }

    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        p.consume_trivia();
        if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
            break;
        }
        let cursor = p.cursor_pos();
        p.error_with_message_no_bump(
            "expected region statement: place, overlay, replace, scatter, or if",
        );
        p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
        p.expect_stmt_boundary();
        if p.cursor_pos() == cursor {
            p.error();
        }
    }
    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::Block);
}

fn parse_region_item(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::IfKw) {
        parse_region_if(p);
        return true;
    }
    if p.at_ident_text("place") {
        parse_region_named_assignment(
            p,
            "place",
            SyntaxKind::RegionPlaceStmt,
            "expected region placement name",
        );
        return true;
    }
    if p.at_ident_text("overlay") {
        parse_region_named_assignment(
            p,
            "overlay",
            SyntaxKind::RegionOverlayStmt,
            "expected overlay name",
        );
        return true;
    }
    if p.at_ident_text("replace") {
        parse_region_named_assignment(
            p,
            "replace",
            SyntaxKind::RegionReplaceStmt,
            "expected replacement name",
        );
        return true;
    }
    if p.at_ident_text("scatter") {
        parse_region_scatter(p);
        return true;
    }
    false
}

fn parse_region_named_assignment(
    p: &mut Parser,
    keyword: &str,
    node_kind: SyntaxKind,
    missing_name_message: &str,
) {
    let m = p.start();
    expect_ident_text(
        p,
        keyword,
        &format!("expected '{keyword}' to start a region statement"),
    );
    p.expect_with_message(SyntaxKind::Ident, missing_name_message);
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after region item name");
    expr::expr(p);
    m.complete(p, node_kind);
    p.expect_stmt_boundary();
}

fn parse_region_scatter(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "scatter",
        "expected 'scatter' to start a region scatter statement",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected scatter name after 'scatter'");
    let b = p.start();
    expect_block_intro(p, "expected '{' after scatter name");
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        p.consume_trivia();
        while parse_region_item(p) {
            p.consume_trivia();
        }
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            p.error_with_message_no_bump(
                "expected region statement inside scatter: place, overlay, replace, scatter, or if",
            );
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
            p.expect_stmt_boundary();
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after scatter name");
    }
    b.complete(p, SyntaxKind::Block);
    m.complete(p, SyntaxKind::RegionScatterStmt);
    p.expect_stmt_boundary();
}

fn parse_region_if(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::IfKw);
    expr::expr(p);
    expect_block_intro(p, "expected '{' after if condition");
    parse_region_body(p);
    if p.at(SyntaxKind::ElseKw) {
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_region_if(p);
        } else {
            expect_block_intro(p, "expected '{' after else");
            parse_region_body(p);
        }
    } else if p.at_ident_text("but") {
        p.error_with_message_no_bump("`but if` was removed; use `else if`");
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_region_if(p);
        }
    } else if p.at(SyntaxKind::DefaultKw) {
        p.error_with_message_no_bump("`otherwise` was removed from control-flow; use `else`");
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_region_if(p);
        } else {
            if p.at(SyntaxKind::Colon) {
                p.bump();
            }
            if p.at(SyntaxKind::LBrace) {
                parse_region_body(p);
            }
        }
    } else if p.at_ident_text("otherwise") {
        p.error_with_message_no_bump("`otherwise` was removed from control-flow; use `else`");
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_region_if(p);
        } else {
            if p.at(SyntaxKind::Colon) {
                p.bump();
            }
            if p.at(SyntaxKind::LBrace) {
                parse_region_body(p);
            }
        }
    }
    m.complete(p, SyntaxKind::IfStmt);
}

fn parse_volume_signature_and_block(p: &mut Parser) {
    expect_ident_text(
        p,
        "volume",
        "expected 'volume' to start a volume field declaration",
    );
    expect_ident_text(p, "field", "expected 'field' after 'volume'");
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected volume field name after 'volume field'",
    );
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after volume field name");
    parse_param_list(p);
    p.expect_with_message(
        SyntaxKind::RParen,
        "expected ')' after volume field parameters",
    );
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    expect_block_intro(p, "expected '{' after volume field signature");
    parse_block(p);
}

fn parse_field_clause(p: &mut Parser) -> bool {
    if p.at_ident_text("support") {
        parse_field_support_clause(p);
        return true;
    }
    if p.at_ident_text("bounds") {
        parse_field_bounds_clause(p);
        return true;
    }
    false
}

fn is_field_semantic_start(p: &Parser) -> bool {
    p.at(SyntaxKind::UseKw)
        || p.at_ident_text("union")
        || p.at_ident_text("intersection")
        || p.at_ident_text("subtract")
        || p.at_ident_text("smooth_union")
        || p.at_ident_text("smooth_intersection")
        || p.at_ident_text("smooth_subtract")
        || p.at_ident_text("translate")
        || p.at_ident_text("rotate")
        || p.at_ident_text("uniform_scale")
        || p.at_ident_text("affine_transform")
        || p.at_ident_text("warp")
        || p.at_ident_text("repeat_linear")
        || p.at_ident_text("repeat_grid")
        || p.at_ident_text("radial_repeat")
        || p.at_ident_text("mirror_array")
        || p.at_ident_text("instance_array")
        || p.at_ident_text("bend")
        || p.at_ident_text("twist")
        || p.at_ident_text("taper")
        || p.at_ident_text("displace")
        || p.at_ident_text("extrude")
        || p.at_ident_text("revolve")
        || p.at_ident_text("sweep")
        || p.at_ident_text("loft")
        || is_field_primitive_name(p.peek_text())
}

fn is_field_primitive_name(text: &str) -> bool {
    matches!(
        text,
        "sphere"
            | "box"
            | "capsule"
            | "cylinder"
            | "plane"
            | "torus"
            | "rounded_box"
            | "ellipsoid"
            | "cone"
            | "capped_cone"
            | "box_frame"
            | "slab"
            | "triangle_prism"
            | "hex_prism"
    )
}

fn is_profile_primitive_name(text: &str) -> bool {
    matches!(
        text,
        "circle2" | "rect2" | "rounded_rect2" | "capsule2" | "segment2" | "polygon2" | "polyline2"
    )
}

fn parse_field_expr(p: &mut Parser) {
    if is_field_primitive_name(p.peek_text()) {
        parse_field_primitive_expr(p);
        return;
    }
    if p.at(SyntaxKind::UseKw) {
        parse_field_use_expr(p);
        return;
    }
    if p.at_ident_text("union") {
        parse_field_union_expr(p);
        return;
    }
    if p.at_ident_text("intersection") {
        parse_field_intersection_expr(p);
        return;
    }
    if p.at_ident_text("subtract") {
        parse_field_subtract_expr(p);
        return;
    }
    if p.at_ident_text("smooth_union") {
        parse_field_smooth_union_expr(p);
        return;
    }
    if p.at_ident_text("smooth_intersection") {
        parse_field_smooth_intersection_expr(p);
        return;
    }
    if p.at_ident_text("smooth_subtract") {
        parse_field_smooth_subtract_expr(p);
        return;
    }
    if p.at_ident_text("translate") {
        parse_field_wrapped_expr(
            p,
            "translate",
            SyntaxKind::FieldTranslateExpr,
            "expected '{' after 'translate'",
            "expected '}' after field translate body",
        );
        return;
    }
    if p.at_ident_text("rotate") {
        parse_field_wrapped_expr(
            p,
            "rotate",
            SyntaxKind::FieldRotateExpr,
            "expected '{' after 'rotate'",
            "expected '}' after field rotate body",
        );
        return;
    }
    if p.at_ident_text("uniform_scale") {
        parse_field_wrapped_expr(
            p,
            "uniform_scale",
            SyntaxKind::FieldUniformScaleExpr,
            "expected '{' after 'uniform_scale'",
            "expected '}' after field uniform scale body",
        );
        return;
    }
    if p.at_ident_text("affine_transform") {
        parse_field_wrapped_expr(
            p,
            "affine_transform",
            SyntaxKind::FieldAffineTransformExpr,
            "expected '{' after 'affine_transform'",
            "expected '}' after field affine transform body",
        );
        return;
    }
    if p.at_ident_text("warp") {
        parse_field_wrapped_expr(
            p,
            "warp",
            SyntaxKind::FieldWarpExpr,
            "expected '{' after 'warp'",
            "expected '}' after field warp body",
        );
        return;
    }
    if p.at_ident_text("repeat_linear") {
        parse_field_wrapped_expr(
            p,
            "repeat_linear",
            SyntaxKind::FieldRepeatLinearExpr,
            "expected '{' after 'repeat_linear'",
            "expected '}' after field repeat linear body",
        );
        return;
    }
    if p.at_ident_text("repeat_grid") {
        parse_field_wrapped_expr(
            p,
            "repeat_grid",
            SyntaxKind::FieldRepeatGridExpr,
            "expected '{' after 'repeat_grid'",
            "expected '}' after field repeat grid body",
        );
        return;
    }
    if p.at_ident_text("radial_repeat") {
        parse_field_wrapped_expr(
            p,
            "radial_repeat",
            SyntaxKind::FieldRadialRepeatExpr,
            "expected '{' after 'radial_repeat'",
            "expected '}' after field radial repeat body",
        );
        return;
    }
    if p.at_ident_text("mirror_array") {
        parse_field_wrapped_expr(
            p,
            "mirror_array",
            SyntaxKind::FieldMirrorArrayExpr,
            "expected '{' after 'mirror_array'",
            "expected '}' after field mirror array body",
        );
        return;
    }
    if p.at_ident_text("instance_array") {
        parse_field_wrapped_expr(
            p,
            "instance_array",
            SyntaxKind::FieldInstanceArrayExpr,
            "expected '{' after 'instance_array'",
            "expected '}' after field instance array body",
        );
        return;
    }
    if p.at_ident_text("bend") {
        parse_field_wrapped_expr(
            p,
            "bend",
            SyntaxKind::FieldBendExpr,
            "expected '{' after 'bend'",
            "expected '}' after field bend body",
        );
        return;
    }
    if p.at_ident_text("twist") {
        parse_field_wrapped_expr(
            p,
            "twist",
            SyntaxKind::FieldTwistExpr,
            "expected '{' after 'twist'",
            "expected '}' after field twist body",
        );
        return;
    }
    if p.at_ident_text("taper") {
        parse_field_wrapped_expr(
            p,
            "taper",
            SyntaxKind::FieldTaperExpr,
            "expected '{' after 'taper'",
            "expected '}' after field taper body",
        );
        return;
    }
    if p.at_ident_text("displace") {
        parse_field_wrapped_expr(
            p,
            "displace",
            SyntaxKind::FieldDisplaceExpr,
            "expected '{' after 'displace'",
            "expected '}' after field displace body",
        );
        return;
    }
    if p.at_ident_text("extrude") {
        parse_field_profile_wrapped_expr(
            p,
            "extrude",
            SyntaxKind::FieldExtrudeExpr,
            "expected '{' after 'extrude'",
            "expected '}' after field extrude body",
        );
        return;
    }
    if p.at_ident_text("revolve") {
        parse_field_revolve_expr(p);
        return;
    }
    if p.at_ident_text("sweep") {
        parse_field_profile_wrapped_expr(
            p,
            "sweep",
            SyntaxKind::FieldSweepExpr,
            "expected '{' after 'sweep'",
            "expected '}' after field sweep body",
        );
        return;
    }
    if p.at_ident_text("loft") {
        parse_field_loft_expr(p);
        return;
    }
    p.error_with_message_no_bump("expected field composition expression");
}

fn parse_field_use_expr(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::UseKw);
    p.expect_with_message(SyntaxKind::Ident, "expected field name after 'use'");
    m.complete(p, SyntaxKind::FieldUseExpr);
}

fn parse_field_union_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "union", "expected 'union' to start a field union");
    parse_field_expr_block(p, SyntaxKind::FieldUnionExpr, "expected '{' after 'union'");
    m.complete(p, SyntaxKind::FieldUnionExpr);
}

fn parse_field_intersection_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "intersection",
        "expected 'intersection' to start a field intersection",
    );
    parse_field_expr_block(
        p,
        SyntaxKind::FieldIntersectionExpr,
        "expected '{' after 'intersection'",
    );
    m.complete(p, SyntaxKind::FieldIntersectionExpr);
}

fn parse_field_subtract_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "subtract",
        "expected 'subtract' to start field subtraction",
    );
    let inner = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' after 'subtract'");
    }
    if p.at(SyntaxKind::ProvenancePolicyKw) {
        parse_field_provenance_policy_clause(p, "", &["left", "right"]);
    }
    parse_field_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    parse_field_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    p.expect_with_message(
        SyntaxKind::RBrace,
        "expected '}' after field subtraction operands",
    );
    inner.complete(p, SyntaxKind::Block);
    m.complete(p, SyntaxKind::FieldSubtractExpr);
}

fn parse_field_wrapped_expr(
    p: &mut Parser,
    keyword: &str,
    expr_kind: SyntaxKind,
    open_error: &str,
    close_error: &str,
) {
    let m = p.start();
    expect_ident_text(
        p,
        keyword,
        &format!("expected '{keyword}' to start a field wrapper"),
    );
    p.expect_with_message(
        SyntaxKind::Equals,
        &format!("expected '=' after '{keyword}'"),
    );
    expr::expr(p);
    p.consume_trivia();
    let body = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump(open_error);
    }
    p.consume_trivia();
    if !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        let cursor = p.cursor_pos();
        parse_field_expr(p);
        if p.cursor_pos() == cursor {
            p.error();
        }
        p.consume_trivia();
        if !p.at(SyntaxKind::RBrace) {
            p.error_with_message_no_bump(close_error);
            p.recover_until(&[SyntaxKind::RBrace]);
        }
    } else {
        p.error_with_message_no_bump(close_error);
    }
    p.expect_with_message(SyntaxKind::RBrace, close_error);
    body.complete(p, SyntaxKind::Block);
    m.complete(p, expr_kind);
}

fn parse_field_profile_wrapped_expr(
    p: &mut Parser,
    keyword: &str,
    expr_kind: SyntaxKind,
    open_error: &str,
    close_error: &str,
) {
    let m = p.start();
    expect_ident_text(
        p,
        keyword,
        &format!("expected '{keyword}' to start a field wrapper"),
    );
    p.expect_with_message(
        SyntaxKind::Equals,
        &format!("expected '=' after '{keyword}'"),
    );
    expr::expr(p);
    p.consume_trivia();
    let body = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump(open_error);
    }
    p.consume_trivia();
    if !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        let cursor = p.cursor_pos();
        parse_profile_expr(p);
        if p.cursor_pos() == cursor {
            p.error();
        }
        p.consume_trivia();
        if !p.at(SyntaxKind::RBrace) {
            p.error_with_message_no_bump(close_error);
            p.recover_until(&[SyntaxKind::RBrace]);
        }
    } else {
        p.error_with_message_no_bump(close_error);
    }
    p.expect_with_message(SyntaxKind::RBrace, close_error);
    body.complete(p, SyntaxKind::Block);
    m.complete(p, expr_kind);
}

fn parse_profile_expr(p: &mut Parser) {
    if is_profile_primitive_name(p.peek_text()) {
        parse_field_primitive_expr(p);
        return;
    }
    p.error_with_message_no_bump("expected profile primitive expression");
}

fn parse_field_revolve_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "revolve", "expected 'revolve' to start a field revolve");
    let body = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' after 'revolve'");
    }
    p.consume_trivia();
    parse_profile_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    p.expect_with_message(SyntaxKind::RBrace, "expected '}' after field revolve body");
    body.complete(p, SyntaxKind::Block);
    m.complete(p, SyntaxKind::FieldRevolveExpr);
}

fn parse_field_loft_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "loft", "expected 'loft' to start a field loft");
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after 'loft'");
    expr::expr(p);
    p.consume_trivia();
    let body = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' after 'loft'");
    }
    p.consume_trivia();
    p.expect_with_message(
        SyntaxKind::FromKw,
        "expected 'from' to start loft profile pair",
    );
    parse_profile_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    expect_ident_text(p, "to", "expected 'to' after loft source profile");
    parse_profile_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    p.expect_with_message(SyntaxKind::RBrace, "expected '}' after field loft body");
    body.complete(p, SyntaxKind::Block);
    m.complete(p, SyntaxKind::FieldLoftExpr);
}

fn parse_field_smoothing_clause(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "smoothing",
        "expected 'smoothing' to start a smooth field clause",
    );
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after 'smoothing'");
    expr::expr(p);
    m.complete(p, SyntaxKind::FieldSmoothingClause);
}

fn parse_field_smooth_expr_block(
    p: &mut Parser,
    expr_kind: SyntaxKind,
    open_error: &str,
    close_error: &str,
) {
    let block = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump(open_error);
    }
    p.consume_trivia();
    parse_field_smoothing_clause(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        p.consume_trivia();
        if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
            break;
        }
        let cursor = p.cursor_pos();
        parse_field_expr(p);
        if p.cursor_pos() == cursor {
            p.error();
            break;
        }
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
    }
    p.expect_with_message(SyntaxKind::RBrace, close_error);
    block.complete(p, SyntaxKind::Block);
    let _ = expr_kind;
}

fn parse_field_smooth_union_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "smooth_union",
        "expected 'smooth_union' to start a smooth field union",
    );
    parse_field_smooth_expr_block(
        p,
        SyntaxKind::FieldSmoothUnionExpr,
        "expected '{' after 'smooth_union'",
        "expected '}' after smooth field union",
    );
    m.complete(p, SyntaxKind::FieldSmoothUnionExpr);
}

fn parse_field_smooth_intersection_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "smooth_intersection",
        "expected 'smooth_intersection' to start a smooth field intersection",
    );
    parse_field_smooth_expr_block(
        p,
        SyntaxKind::FieldSmoothIntersectionExpr,
        "expected '{' after 'smooth_intersection'",
        "expected '}' after smooth field intersection",
    );
    m.complete(p, SyntaxKind::FieldSmoothIntersectionExpr);
}

fn parse_field_smooth_subtract_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "smooth_subtract",
        "expected 'smooth_subtract' to start smooth field subtraction",
    );
    let inner = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' after 'smooth_subtract'");
    }
    p.consume_trivia();
    parse_field_smoothing_clause(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    parse_field_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    parse_field_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    p.expect_with_message(
        SyntaxKind::RBrace,
        "expected '}' after smooth field subtraction operands",
    );
    inner.complete(p, SyntaxKind::Block);
    m.complete(p, SyntaxKind::FieldSmoothSubtractExpr);
}

fn parse_field_expr_block(p: &mut Parser, expr_kind: SyntaxKind, open_error: &str) {
    let block = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump(open_error);
    }
    if p.at(SyntaxKind::ProvenancePolicyKw) {
        parse_field_provenance_policy_clause(p, "", &["nearest", "ordered"]);
    }
    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        p.consume_trivia();
        if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
            break;
        }
        let cursor = p.cursor_pos();
        parse_field_expr(p);
        if p.cursor_pos() == cursor {
            p.error();
            break;
        }
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
    }
    p.expect_with_message(
        SyntaxKind::RBrace,
        "expected '}' after field composition block",
    );
    block.complete(p, SyntaxKind::Block);
    let _ = expr_kind;
}

fn parse_field_provenance_policy_clause(
    p: &mut Parser,
    missing_error: &str,
    allowed_policies: &[&str],
) {
    p.consume_trivia();
    if !p.at(SyntaxKind::ProvenancePolicyKw) {
        p.error_with_message_no_bump(missing_error);
        return;
    }

    let m = p.start();
    p.bump();
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after provenance_policy");
    p.consume_trivia();
    if !is_name_like_label_token(p.peek()) {
        p.error_with_message_no_bump("expected provenance policy name");
    } else {
        let policy = p.peek_text().to_string();
        if allowed_policies.iter().any(|allowed| *allowed == policy) {
            p.bump();
        } else {
            p.error_with_message_no_bump(&format!(
                "expected provenance policy {}",
                allowed_policies
                    .iter()
                    .map(|allowed| format!("`{allowed}`"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ));
            p.bump();
        }
    }
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    m.complete(p, SyntaxKind::FieldProvenancePolicyClause);
}

fn parse_field_primitive_expr(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(SyntaxKind::Ident, "expected primitive name in field body");
    parse_primitive_call_tail(p, "expected '(' after field primitive name");
    m.complete(p, SyntaxKind::FieldPrimitiveExpr);
}

fn parse_field_support_clause(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "support",
        "expected 'support' to start a field support clause",
    );
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after 'support'");
    expr::expr(p);
    m.complete(p, SyntaxKind::FieldSupportClause);
}

fn parse_field_bounds_clause(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "bounds",
        "expected 'bounds' to start a field bounds clause",
    );
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after 'bounds'");
    expr::expr(p);
    m.complete(p, SyntaxKind::FieldBoundsClause);
}

fn parse_primitive_call_tail(p: &mut Parser, open_error: &str) {
    p.expect_with_message(SyntaxKind::LParen, open_error);
    let mut first = true;
    while !p.at(SyntaxKind::RParen) && !p.is_at_eof() {
        let before = p.cursor_pos();
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RParen, SyntaxKind::Newline]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RParen) {
                    break;
                }
            }
        }
        parse_primitive_arg(p);
        if !p.at(SyntaxKind::Comma) && !p.at(SyntaxKind::RParen) {
            p.error_no_bump();
            p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RParen, SyntaxKind::Newline]);
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else if p.at(SyntaxKind::RParen) {
                break;
            }
        }
        if p.cursor_pos() == before {
            p.error();
        }
        first = false;
    }
    if p.at(SyntaxKind::RParen) {
        p.bump();
    } else {
        p.expect(SyntaxKind::RParen);
    }
}

fn parse_primitive_arg(p: &mut Parser) {
    if is_name_like_label_token(p.peek()) && p.peek_nontrivia_at(1) == SyntaxKind::Equals {
        let m = p.start();
        p.bump();
        p.expect(SyntaxKind::Equals);
        expr::expr(p);
        m.complete(p, SyntaxKind::NamedArg);
        return;
    }
    expr::expr(p);
}

fn parse_material_signature_and_block(p: &mut Parser) {
    expect_ident_text(
        p,
        "material",
        "expected 'material' to start a material declaration",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected material name after 'material'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after material name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after material parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    expect_block_intro(p, "expected '{' after material signature");
    parse_block(p);
}

fn parse_shape_signature_and_block(p: &mut Parser) {
    expect_ident_text(p, "shape", "expected 'shape' to start a shape declaration");
    p.expect_with_message(SyntaxKind::Ident, "expected shape name after 'shape'");
    parse_shape_body(p);
}

fn parse_shape_body(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else if p.at(SyntaxKind::Colon) {
        p.error_with_message_no_bump("':' block introducer was removed; use braces: `{ ... }`");
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' after shape signature");
    }

    p.consume_trivia();
    if is_shape_semantic_start(p) {
        parse_shape_expr(p);
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
        p.expect_with_message(
            SyntaxKind::RBrace,
            "expected '}' after shape composition expression",
        );
        m.complete(p, SyntaxKind::Block);
        return;
    }

    if is_shape_leaf_start(p) {
        parse_shape_leaf_expr(p);
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
        p.expect_with_message(SyntaxKind::RBrace, "expected '}' after shape leaf binding");
        m.complete(p, SyntaxKind::Block);
        return;
    }

    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        p.consume_trivia();
        if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
            break;
        }
        let cursor = p.cursor_pos();
        parse_shape_expr(p);
        if p.cursor_pos() == cursor {
            p.error();
            break;
        }
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
    }
    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::Block);
}

fn is_shape_semantic_start(p: &Parser) -> bool {
    p.at(SyntaxKind::UseKw)
        || p.at_ident_text("union")
        || p.at_ident_text("intersection")
        || p.at_ident_text("subtract")
}

fn is_shape_leaf_start(p: &Parser) -> bool {
    p.at_ident_text("field")
        || p.at_ident_text("material")
        || p.at_ident_text("radiance")
        || p.at_ident_text("volume")
        || p.at_ident_text("payload")
}

fn parse_shape_expr(p: &mut Parser) {
    if p.at(SyntaxKind::UseKw) {
        parse_shape_use_expr(p);
        return;
    }
    if p.at_ident_text("union") {
        parse_shape_union_expr(p);
        return;
    }
    if p.at_ident_text("intersection") {
        parse_shape_intersection_expr(p);
        return;
    }
    if p.at_ident_text("subtract") {
        parse_shape_subtract_expr(p);
        return;
    }
    if is_shape_leaf_start(p) {
        parse_shape_leaf_expr(p);
        return;
    }
    p.error_with_message_no_bump("expected shape expression");
}

fn parse_shape_use_expr(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::UseKw);
    p.expect_with_message(SyntaxKind::Ident, "expected shape name after 'use'");
    m.complete(p, SyntaxKind::ShapeUseExpr);
}

fn parse_shape_union_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "union", "expected 'union' to start a shape union");
    parse_shape_expr_block(p, "expected '{' after 'union'");
    m.complete(p, SyntaxKind::ShapeUnionExpr);
}

fn parse_shape_intersection_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "intersection",
        "expected 'intersection' to start a shape intersection",
    );
    parse_shape_expr_block(p, "expected '{' after 'intersection'");
    m.complete(p, SyntaxKind::ShapeIntersectionExpr);
}

fn parse_shape_subtract_expr(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "subtract",
        "expected 'subtract' to start shape subtraction",
    );
    let inner = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' after 'subtract'");
    }
    if p.at(SyntaxKind::ProvenancePolicyKw) {
        parse_shape_provenance_policy_clause(p, "", &["left", "right"]);
    }
    parse_shape_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    parse_shape_expr(p);
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    p.expect_with_message(
        SyntaxKind::RBrace,
        "expected '}' after shape subtraction operands",
    );
    inner.complete(p, SyntaxKind::Block);
    m.complete(p, SyntaxKind::ShapeSubtractExpr);
}

fn parse_shape_expr_block(p: &mut Parser, open_error: &str) {
    let block = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump(open_error);
    }
    if p.at(SyntaxKind::ProvenancePolicyKw) {
        parse_shape_provenance_policy_clause(p, "", &["nearest", "ordered"]);
    }
    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        p.consume_trivia();
        if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
            break;
        }
        let cursor = p.cursor_pos();
        parse_shape_expr(p);
        if p.cursor_pos() == cursor {
            p.error();
            break;
        }
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
    }
    p.expect_with_message(
        SyntaxKind::RBrace,
        "expected '}' after shape composition block",
    );
    block.complete(p, SyntaxKind::Block);
}

fn parse_shape_provenance_policy_clause(
    p: &mut Parser,
    missing_error: &str,
    allowed_policies: &[&str],
) {
    p.consume_trivia();
    if !p.at(SyntaxKind::ProvenancePolicyKw) {
        p.error_with_message_no_bump(missing_error);
        return;
    }

    let m = p.start();
    p.bump();
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after provenance_policy");
    p.consume_trivia();
    if !is_name_like_label_token(p.peek()) {
        p.error_with_message_no_bump("expected provenance policy name");
    } else {
        let policy = p.peek_text().to_string();
        if allowed_policies.iter().any(|allowed| *allowed == policy) {
            p.bump();
        } else {
            p.error_with_message_no_bump(&format!(
                "expected provenance policy {}",
                allowed_policies
                    .iter()
                    .map(|allowed| format!("`{allowed}`"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            ));
            p.bump();
        }
    }
    p.consume_trivia();
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
    m.complete(p, SyntaxKind::ShapeProvenancePolicyClause);
}

fn parse_shape_leaf_expr(p: &mut Parser) {
    let m = p.start();
    let block = p.start();
    while is_shape_leaf_start(p) && !p.is_at_eof() {
        p.consume_trivia();
        if !is_shape_leaf_start(p) || p.is_at_eof() {
            break;
        }
        let cursor = p.cursor_pos();
        if p.at_ident_text("field") {
            parse_shape_field_binding(p);
        } else if p.at_ident_text("material") {
            parse_shape_material_binding(p);
        } else if p.at_ident_text("radiance") {
            parse_shape_radiance_binding(p);
        } else if p.at_ident_text("volume") {
            parse_shape_volume_binding(p);
        } else if p.at_ident_text("payload") {
            parse_shape_payload_binding(p);
        } else {
            p.error();
        }
        if p.cursor_pos() == cursor {
            p.error();
            break;
        }
        p.consume_trivia();
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
        }
    }
    block.complete(p, SyntaxKind::Block);
    m.complete(p, SyntaxKind::ShapeLeafExpr);
}

fn parse_shape_field_binding(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "field", "expected 'field' binding in shape leaf");
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after shape field binding");
    expr::expr(p);
    m.complete(p, SyntaxKind::ShapeFieldBinding);
}

fn parse_shape_material_binding(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "material", "expected 'material' binding in shape leaf");
    p.expect_with_message(
        SyntaxKind::Equals,
        "expected '=' after shape material binding",
    );
    expr::expr(p);
    m.complete(p, SyntaxKind::ShapeMaterialBinding);
}

fn parse_shape_radiance_binding(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "radiance", "expected 'radiance' binding in shape leaf");
    p.expect_with_message(
        SyntaxKind::Equals,
        "expected '=' after shape radiance binding",
    );
    expr::expr(p);
    m.complete(p, SyntaxKind::ShapeRadianceBinding);
}

fn parse_shape_volume_binding(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "volume", "expected 'volume' binding in shape leaf");
    p.expect_with_message(
        SyntaxKind::Equals,
        "expected '=' after shape volume binding",
    );
    expr::expr(p);
    m.complete(p, SyntaxKind::ShapeVolumeBinding);
}

fn parse_shape_payload_binding(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "payload", "expected 'payload' binding in shape leaf");
    p.expect_with_message(
        SyntaxKind::Equals,
        "expected '=' after shape payload binding",
    );
    expr::expr(p);
    m.complete(p, SyntaxKind::ShapePayloadBinding);
}

fn parse_function_decl_head(p: &mut Parser, head: FunctionDeclHead) {
    match head {
        FunctionDeclHead::Function => p.expect_with_message(
            SyntaxKind::FnKw,
            "expected 'fn' to start a function definition",
        ),
        FunctionDeclHead::Kernel => {
            p.expect_with_message(
                SyntaxKind::KernelKw,
                "expected 'kernel fn' to start a kernel declaration",
            );
            p.expect_with_message(SyntaxKind::FnKw, "expected 'fn' after 'kernel'");
        }
        FunctionDeclHead::System => p.expect_with_message(
            SyntaxKind::SystemKw,
            "expected 'system' to start a system declaration",
        ),
    }
}

fn expected_name_error(head: FunctionDeclHead) -> &'static str {
    match head {
        FunctionDeclHead::Function => "expected function name after 'fn'",
        FunctionDeclHead::Kernel => "expected function name after 'kernel fn'",
        FunctionDeclHead::System => "expected system name after 'system'",
    }
}

fn expect_ident_text(p: &mut Parser, expected: &str, message: &str) {
    if p.at_ident_text(expected) {
        p.bump();
        return;
    }
    if p.at(SyntaxKind::Ident) {
        p.error_with_message_no_bump(message);
        p.bump();
        return;
    }
    p.expect_with_message(SyntaxKind::Ident, message);
}

fn parse_field_class(p: &mut Parser) {
    if p.at_ident_text("exact") || p.at_ident_text("conservative") {
        p.bump();
        return;
    }
    expect_ident_text(
        p,
        "exact",
        "expected 'exact' or 'conservative' after 'field'",
    );
}

fn parse_optional_type_params(p: &mut Parser) {
    if !p.at(SyntaxKind::LBracket) {
        return;
    }
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
            | SyntaxKind::PresetKw
            | SyntaxKind::ProfileKw
            | SyntaxKind::OverridesKw
    )
}

fn is_metadata_value_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Ident
            | SyntaxKind::StringLiteral
            | SyntaxKind::PresetKw
            | SyntaxKind::ProfileKw
            | SyntaxKind::OverridesKw
    )
}
