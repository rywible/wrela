use super::{parse_block, parse_param_list, types};
use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn class_def(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::ClassKw) || p.at(SyntaxKind::AnKw) {
        p.bump(); // A | An
    } else {
        p.error_with_message("expected 'A' or 'An' to start a class definition", true);
    }

    p.expect_with_message(SyntaxKind::Ident, "expected class name after 'A' or 'An'");
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after class name");

    if p.at(SyntaxKind::Indent) {
        p.bump();
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            if is_class_item_start(p) {
                if p.at(SyntaxKind::HasKw) {
                    parse_has(p);
                } else if p.at(SyntaxKind::CanKw) {
                    method_def(p);
                } else if p.at(SyntaxKind::PrivateKw) {
                    parse_private_block(p);
                }
            } else {
                p.error();
                p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Dedent]);
                p.expect_stmt_boundary();
            }
        }
        p.expect(SyntaxKind::Dedent);
    } else {
        p.error_expected_indented_block();
    }

    m.complete(p, SyntaxKind::ClassDef);
}

fn parse_has(p: &mut Parser) {
    if p.at(SyntaxKind::HasKw) {
        p.bump();
    } else {
        p.error_with_message("expected 'has' to start field definitions", true);
    }
    if p.at(SyntaxKind::Colon) {
        let m = p.start();
        p.bump();
        if p.at(SyntaxKind::Indent) {
            p.bump();
            while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
                field_item(p);
            }
            p.expect(SyntaxKind::Dedent);
        } else {
            p.error_expected_indented_block();
        }
        m.complete(p, SyntaxKind::HasBlock);
    } else {
        field_def(p);
        p.expect_stmt_boundary();
    }
}

fn field_item(p: &mut Parser) {
    if p.at(SyntaxKind::PrivateKw) && p.peek_nontrivia_at(1) == SyntaxKind::Colon {
        parse_private_fields_block(p);
        return;
    }
    field_def(p);
    p.expect_stmt_boundary();
}

fn field_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(SyntaxKind::Ident, "expected field name");
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after field name");
    types::parse_type(p);
    m.complete(p, SyntaxKind::FieldDef);
}

fn parse_private_fields_block(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::PrivateKw);
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after 'private'");
    if p.at(SyntaxKind::Indent) {
        p.bump();
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            field_def(p);
            p.expect_stmt_boundary();
        }
        p.expect(SyntaxKind::Dedent);
    } else {
        p.error_expected_indented_block();
    }
    m.complete(p, SyntaxKind::PrivateBlock);
}

fn method_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::CanKw,
        "expected 'can' to start a method definition",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected method name after 'can'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after method name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after method parameters");

    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    p.expect_with_message(SyntaxKind::Colon, "expected ':' after method signature");

    parse_block(p);

    m.complete(p, SyntaxKind::MethodDef);
}

fn is_class_item_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::HasKw) || p.at(SyntaxKind::CanKw) {
        return true;
    }
    if p.at(SyntaxKind::PrivateKw) {
        return true;
    }
    false
}

fn parse_private_block(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::PrivateKw);
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after 'private'");
    if p.at(SyntaxKind::Indent) {
        p.bump();
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            if p.at(SyntaxKind::HasKw) {
                parse_has(p);
            } else if p.at(SyntaxKind::CanKw) {
                method_def(p);
            } else {
                p.error();
                p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Dedent]);
                p.expect_stmt_boundary();
            }
        }
        p.expect(SyntaxKind::Dedent);
    } else {
        p.error_expected_indented_block();
    }
    m.complete(p, SyntaxKind::PrivateBlock);
}
