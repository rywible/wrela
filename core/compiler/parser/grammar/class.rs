use super::{expr, parse_block, parse_param_list, types};
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
    parse_type_params(p);

    if p.at(SyntaxKind::IsKw) {
        p.bump();
        if p.at(SyntaxKind::EitherKw) {
            p.bump();
            parse_enum_body(p);
            m.complete(p, SyntaxKind::EnumDef);
            return;
        }
        p.error_with_message("expected 'either' after 'is'", true);
    }

    p.expect_with_message(SyntaxKind::Colon, "expected ':' after class name");

    if p.at(SyntaxKind::Indent) {
        p.bump();
        if p.at(SyntaxKind::IsKw) {
            parse_is_a_clause(p);
        }
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            if is_class_item_start(p) {
                if p.at(SyntaxKind::HasKw) {
                    parse_has(p);
                } else if p.at(SyntaxKind::CanKw) {
                    method_def(p);
                } else if p.at(SyntaxKind::MustKw) {
                    must_method_def(p);
                } else if p.at(SyntaxKind::DerivesKw) {
                    derive_def(p);
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

fn parse_type_params(p: &mut Parser) {
    if !p.at(SyntaxKind::LBracket) {
        return;
    }
    let m = p.start();
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
        p.expect_with_message(SyntaxKind::Ident, "expected type parameter name");
        first = false;
    }
    p.expect_with_message(SyntaxKind::RBracket, "expected ']' after type parameters");
    m.complete(p, SyntaxKind::TypeParamList);
}

fn parse_is_a_clause(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::IsKw);
    if !p.at_ident_text("a") {
        p.error_with_message("expected 'a' after 'is'", true);
    } else {
        p.bump();
    }
    p.expect_with_message(SyntaxKind::Ident, "expected interface name after 'is a'");
    m.complete(p, SyntaxKind::IsAClause);
    p.expect_stmt_boundary();
}

fn parse_enum_body(p: &mut Parser) {
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after 'is either'");
    if p.at(SyntaxKind::Indent) {
        p.bump();
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            parse_enum_variant(p);
        }
        p.expect(SyntaxKind::Dedent);
    } else {
        p.error_expected_indented_block();
    }
}

fn parse_enum_variant(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(SyntaxKind::Ident, "expected enum variant name");
    if p.at(SyntaxKind::LParen) {
        p.bump();
        parse_param_list(p);
        p.expect_with_message(SyntaxKind::RParen, "expected ')' after variant parameters");
    }
    m.complete(p, SyntaxKind::EnumVariant);
    p.expect_stmt_boundary();
}

fn must_method_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::MustKw,
        "expected 'must' to start an interface method signature",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected method name after 'must'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after method name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after method parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);
    m.complete(p, SyntaxKind::MustMethodDef);
    p.expect_stmt_boundary();
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
    if p.at(SyntaxKind::Equals) {
        p.bump();
        expr::expr(p);
    }
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

fn derive_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::DerivesKw,
        "expected 'derives' to start a derived property definition",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected derived name after 'derives'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after derived name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after derived parameters");

    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    p.expect_with_message(SyntaxKind::Colon, "expected ':' after derived signature");

    parse_block(p);

    m.complete(p, SyntaxKind::DeriveDef);
}

fn is_class_item_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::HasKw)
        || p.at(SyntaxKind::CanKw)
        || p.at(SyntaxKind::MustKw)
        || p.at(SyntaxKind::DerivesKw)
    {
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
            } else if p.at(SyntaxKind::DerivesKw) {
                derive_def(p);
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
