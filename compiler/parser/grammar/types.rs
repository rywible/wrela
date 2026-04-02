use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn parse_type(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::Ident) || p.at(SyntaxKind::IntNumber) {
        p.bump();
    } else {
        p.expect(SyntaxKind::Ident);
    }
    if p.at(SyntaxKind::LBracket) {
        parse_type_args(p);
    }
    m.complete(p, SyntaxKind::TypeRef);
}

fn parse_type_args(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::LBracket);
    if !p.at(SyntaxKind::RBracket) {
        loop {
            parse_type(p);
            if p.at(SyntaxKind::Comma) {
                p.bump();
                if p.at(SyntaxKind::RBracket) {
                    break;
                }
            } else {
                break;
            }
        }
    }
    p.expect(SyntaxKind::RBracket);
    m.complete(p, SyntaxKind::TypeArgList);
}
