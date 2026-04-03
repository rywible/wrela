use super::{expect_block_intro, parse_block, parse_param_list, types};
use crate::parser::Parser;
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
