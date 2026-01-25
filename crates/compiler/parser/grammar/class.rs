use super::{parse_block, parse_param_list, parse_visibility, types};
use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn class_def(p: &mut Parser) {
    let m = p.start();
    parse_visibility(p);
    if p.at(SyntaxKind::ClassKw) || p.at(SyntaxKind::AnKw) {
        p.bump(); // A | An
    } else {
        p.error();
    }

    p.expect(SyntaxKind::Ident);
    p.expect(SyntaxKind::Colon);

    if p.at(SyntaxKind::Indent) {
        p.bump();
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            if is_class_item_start(p) {
                if p.at(SyntaxKind::HasKw) || p.peek_nontrivia_at(1) == SyntaxKind::HasKw {
                    parse_has(p);
                } else if p.at(SyntaxKind::CanKw) || p.peek_nontrivia_at(1) == SyntaxKind::CanKw {
                    method_def(p);
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
    if p.at(SyntaxKind::PublicKw) || p.at(SyntaxKind::PrivateKw) {
        parse_visibility(p);
    }
    if p.at(SyntaxKind::HasKw) {
        p.bump();
    } else {
        p.error();
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
    let _ = parse_visibility(p);
    field_def(p);
    p.expect_stmt_boundary();
}

fn field_def(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::Ident);
    p.expect(SyntaxKind::Colon);
    types::parse_type(p);
    m.complete(p, SyntaxKind::FieldDef);
}

fn method_def(p: &mut Parser) {
    let m = p.start();
    parse_visibility(p);
    p.expect(SyntaxKind::CanKw);

    p.expect(SyntaxKind::Ident);
    p.expect(SyntaxKind::LParen);
    parse_param_list(p);
    p.expect(SyntaxKind::RParen);

    if p.at(SyntaxKind::Arrow) {
        p.bump();
        types::parse_type(p);
    }

    p.expect(SyntaxKind::Colon);

    parse_block(p);

    m.complete(p, SyntaxKind::MethodDef);
}

fn is_class_item_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::HasKw) || p.at(SyntaxKind::CanKw) {
        return true;
    }
    if p.at(SyntaxKind::PublicKw) || p.at(SyntaxKind::PrivateKw) {
        let next = p.peek_nontrivia_at(1);
        return matches!(next, SyntaxKind::HasKw | SyntaxKind::CanKw);
    }
    false
}
