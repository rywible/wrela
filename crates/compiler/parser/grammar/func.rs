use super::{parse_block, parse_param_list, parse_visibility, types};
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
        types::parse_type(p);
    }

    p.expect(SyntaxKind::Colon);

    parse_block(p);

    m.complete(p, SyntaxKind::FuncDef);
}
