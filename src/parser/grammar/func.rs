use super::{parse_param_list, parse_statement, parse_visibility};
use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn func_def(p: &mut Parser) {
    let m = p.start();
    parse_visibility(p);
    p.expect(SyntaxKind::ToKw);

    p.expect(SyntaxKind::Ident);
    p.expect(SyntaxKind::LParen);
    parse_param_list(p);
    p.expect(SyntaxKind::RParen);

    if p.at(SyntaxKind::Arrow) {
        p.bump();
        p.expect(SyntaxKind::Ident); // return type
    }

    p.expect(SyntaxKind::Colon);

    if p.at(SyntaxKind::Indent) {
        p.bump();
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            parse_statement(p);
        }
        p.expect(SyntaxKind::Dedent);
    } else {
        p.error_expected_indented_block();
    }

    m.complete(p, SyntaxKind::FuncDef);
}
