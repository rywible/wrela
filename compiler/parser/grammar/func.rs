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

pub fn attributed_func_or_check_def(p: &mut Parser) -> bool {
    if !p.at(SyntaxKind::At) {
        return false;
    }
    let m = p.start();
    parse_func_attributes(p);
    if p.at(SyntaxKind::FnKw) || p.at(SyntaxKind::SystemKw) {
        let (head, node_kind, allow_metadata) = if p.at(SyntaxKind::SystemKw) {
            (SyntaxKind::SystemKw, SyntaxKind::SystemDef, true)
        } else {
            (SyntaxKind::FnKw, SyntaxKind::FuncDef, false)
        };
        parse_func_signature_and_block(p, head, allow_metadata);
        m.complete(p, node_kind);
        return true;
    }
    p.error_with_message_no_bump("expected `fn` after attributes");
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

fn expected_head_error(head: SyntaxKind) -> &'static str {
    match head {
        SyntaxKind::FnKw => "expected 'fn' to start a function definition",
        SyntaxKind::SystemKw => "expected 'system' to start a system declaration",
        _ => "expected declaration keyword",
    }
}

fn expected_name_error(head: SyntaxKind) -> &'static str {
    match head {
        SyntaxKind::FnKw => "expected function name after 'fn'",
        SyntaxKind::SystemKw => "expected system name after 'system'",
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
