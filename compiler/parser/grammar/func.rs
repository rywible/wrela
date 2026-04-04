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
        || p.at_ident_text("transform")
        || p.at_ident_text("mirror")
        || p.at_ident_text("repeat")
        || p.at_ident_text("instance")
        || is_field_primitive_name(p.peek_text())
}

fn is_field_primitive_name(text: &str) -> bool {
    matches!(
        text,
        "sphere" | "box" | "capsule" | "cylinder" | "plane" | "torus"
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
    if p.at_ident_text("transform") {
        parse_field_transform_expr(p);
        return;
    }
    if p.at_ident_text("mirror") {
        parse_field_mirror_expr(p);
        return;
    }
    if p.at_ident_text("repeat") {
        parse_field_repeat_expr(p);
        return;
    }
    if p.at_ident_text("instance") {
        parse_field_instance_expr(p);
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

fn parse_field_transform_expr(p: &mut Parser) {
    parse_field_wrapped_expr(
        p,
        "transform",
        SyntaxKind::FieldTransformExpr,
        "expected '{' after 'transform'",
        "expected '}' after field transform body",
    );
}

fn parse_field_mirror_expr(p: &mut Parser) {
    parse_field_wrapped_expr(
        p,
        "mirror",
        SyntaxKind::FieldMirrorExpr,
        "expected '{' after 'mirror'",
        "expected '}' after field mirror body",
    );
}

fn parse_field_repeat_expr(p: &mut Parser) {
    parse_field_wrapped_expr(
        p,
        "repeat",
        SyntaxKind::FieldRepeatExpr,
        "expected '{' after 'repeat'",
        "expected '}' after field repeat body",
    );
}

fn parse_field_instance_expr(p: &mut Parser) {
    parse_field_wrapped_expr(
        p,
        "instance",
        SyntaxKind::FieldInstanceExpr,
        "expected '{' after 'instance'",
        "expected '}' after field instance body",
    );
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
    p.expect_with_message(
        SyntaxKind::Equals,
        "expected '=' after provenance_policy",
    );
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
    p.at_ident_text("field") || p.at_ident_text("material") || p.at_ident_text("payload")
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
    p.expect_with_message(
        SyntaxKind::Equals,
        "expected '=' after provenance_policy",
    );
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
