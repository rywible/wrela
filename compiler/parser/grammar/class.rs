use super::{expect_block_intro, expr, parse_block, parse_param_list, types};
use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn component_def(p: &mut Parser) {
    class_like_def(
        p,
        SyntaxKind::ComponentKw,
        SyntaxKind::ComponentDef,
        "expected type name after 'component'",
    );
}

pub fn resource_def(p: &mut Parser) {
    class_like_def(
        p,
        SyntaxKind::ResourceKw,
        SyntaxKind::ResourceDef,
        "expected type name after 'resource'",
    );
}

pub fn event_def(p: &mut Parser) {
    class_like_def(
        p,
        SyntaxKind::EventKw,
        SyntaxKind::EventDef,
        "expected type name after 'event'",
    );
}

pub fn command_def(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("command") {
        p.bump();
    } else {
        p.error_with_message("expected 'command' to start a command declaration", true);
    }
    p.expect_with_message(SyntaxKind::Ident, "expected type name after 'command'");
    parse_type_params(p);
    parse_class_body(p);
    m.complete(p, SyntaxKind::CommandDef);
}

pub fn value_def(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("value") {
        p.bump();
    } else {
        p.error_with_message("expected 'value' to start a value declaration", true);
    }
    p.expect_with_message(SyntaxKind::Ident, "expected type name after 'value'");
    parse_type_params(p);
    parse_class_body(p);
    m.complete(p, SyntaxKind::ValueDef);
}

fn class_like_def(
    p: &mut Parser,
    keyword: SyntaxKind,
    node_kind: SyntaxKind,
    missing_name_message: &'static str,
) {
    let m = p.start();
    p.expect_with_message(keyword, "expected declaration keyword");
    p.expect_with_message(SyntaxKind::Ident, missing_name_message);
    parse_type_params(p);

    if p.at(SyntaxKind::DerivesKw) {
        p.error_with_message_no_bump(
            "derive traits were removed; semantics are structural by default",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::LBrace, SyntaxKind::Newline]);
    }

    parse_class_body(p);
    m.complete(p, node_kind);
}

pub fn class_def(p: &mut Parser) {
    if p.at(SyntaxKind::EnumKw) {
        enum_def(p);
        return;
    }

    let m = p.start();
    let is_interface = p.at(SyntaxKind::InterfaceKw);
    if p.at(SyntaxKind::ClassKw) || is_interface {
        p.bump();
    } else {
        p.error_with_message(
            "expected 'class' or 'interface' to start a type definition",
            true,
        );
    }

    p.expect_with_message(
        SyntaxKind::Ident,
        "expected type name after 'class'/'interface'",
    );
    parse_type_params(p);

    if p.at(SyntaxKind::DerivesKw) {
        p.error_with_message_no_bump(
            "derive traits were removed; semantics are structural by default",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::LBrace, SyntaxKind::Newline]);
    }

    parse_class_body(p);

    if is_interface {
        // Keep interfaces on the same node kind for now; semantic passes distinguish by body shape.
    }
    m.complete(p, SyntaxKind::ClassDef);
}

fn parse_class_body(p: &mut Parser) {
    expect_block_intro(p, "expected '{' after type declaration");

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        if p.at(SyntaxKind::IsKw) {
            parse_is_a_clause(p);
        }
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            if is_field_start(p) {
                field_def(p);
                p.expect_stmt_boundary();
            } else if is_class_item_start(p) {
                if p.at(SyntaxKind::FnKw) {
                    method_def(p);
                } else if p.at(SyntaxKind::MustKw) {
                    must_method_def(p);
                } else if p.at(SyntaxKind::DerivesKw) {
                    p.error_with_message_no_bump("removed keyword `derives`; use `fn` for methods");
                    p.bump();
                } else if p.at(SyntaxKind::PrivateKw) {
                    parse_private_block(p);
                }
            } else if p.at(SyntaxKind::CanKw)
                || p.at(SyntaxKind::ChecksKw)
                || p.at(SyntaxKind::CheckKw)
            {
                p.error_with_message_no_bump("removed keyword in type body; use `fn` for methods");
                p.bump();
            } else {
                p.error();
                p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
                p.expect_stmt_boundary();
            }
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after type declaration");
    }
}

fn enum_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::EnumKw,
        "expected 'enum' to start enum definition",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected enum name after 'enum'");
    parse_type_params(p);

    if p.at(SyntaxKind::DerivesKw) {
        p.error_with_message_no_bump(
            "derive traits were removed; semantics are structural by default",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::LBrace, SyntaxKind::Newline]);
    }

    parse_enum_body(p);
    m.complete(p, SyntaxKind::EnumDef);
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
    expect_block_intro(p, "expected '{' after enum declaration");
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            parse_enum_variant(p);
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after enum declaration");
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
    if p.at(SyntaxKind::CheckKw) {
        p.error_with_message_no_bump("`check` was removed; use `Boolean` return type instead");
        p.bump();
    }
    p.expect_with_message(SyntaxKind::Ident, "expected method name after 'must'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after method name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after method parameters");
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);
    m.complete(p, SyntaxKind::MustMethodDef);
    p.expect_stmt_boundary();
}

fn field_def(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::MutableKw) {
        p.bump();
    }
    p.expect_with_message(SyntaxKind::Ident, "expected field name");
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after field name");
    types::parse_type(p);
    if p.at(SyntaxKind::Equals) {
        p.bump();
        expr::expr(p);
    }
    m.complete(p, SyntaxKind::FieldDef);
}

fn method_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::FnKw,
        "expected 'fn' to start a method definition",
    );
    p.expect_with_message(SyntaxKind::Ident, "expected method name after 'fn'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after method name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after method parameters");

    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    expect_block_intro(p, "expected '{' after method signature");

    parse_block(p);

    m.complete(p, SyntaxKind::MethodDef);
}

fn is_class_item_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::FnKw) || p.at(SyntaxKind::MustKw) {
        return true;
    }
    if p.at(SyntaxKind::PrivateKw) {
        return true;
    }
    false
}

fn is_field_start(p: &Parser) -> bool {
    if p.at(SyntaxKind::Ident) && p.peek_nontrivia_at(1) == SyntaxKind::Colon {
        return true;
    }
    if p.at(SyntaxKind::MutableKw)
        && p.peek_nontrivia_at(1) == SyntaxKind::Ident
        && p.peek_nontrivia_at(2) == SyntaxKind::Colon
    {
        return true;
    }
    false
}

fn parse_private_block(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::PrivateKw);
    expect_block_intro(p, "expected '{' after 'private'");
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            if is_field_start(p) {
                field_def(p);
                p.expect_stmt_boundary();
            } else if p.at(SyntaxKind::FnKw) {
                method_def(p);
            } else if p.at(SyntaxKind::DerivesKw) {
                p.error_with_message_no_bump("removed keyword `derives`; use `fn` for methods");
                p.bump();
            } else if p.at(SyntaxKind::MustKw) {
                must_method_def(p);
            } else {
                p.error();
                p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
                p.expect_stmt_boundary();
            }
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after 'private'");
    }
    m.complete(p, SyntaxKind::PrivateBlock);
}
