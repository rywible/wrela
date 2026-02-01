use super::{parse_block, parse_param_list, types};
use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn func_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::ToKw,
        "expected 'to' to start a function definition",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected function name after 'to'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after function name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after function parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    p.expect_with_message(SyntaxKind::Colon, "expected ':' after function signature");

    parse_block(p);

    m.complete(p, SyntaxKind::FuncDef);
}
