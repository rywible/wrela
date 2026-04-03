use super::types;
use crate::parser::ast::is_name_like_label_token;
use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn expr(p: &mut Parser) {
    expr_binding_power(p, 0);
}

fn expr_binding_power(p: &mut Parser, min_bp: u8) {
    expr_binding_power_until(p, min_bp, None);
}

pub fn expr_until_otherwise(p: &mut Parser) {
    expr_binding_power_until(p, 0, Some(SyntaxKind::ElseKw));
}

fn expr_binding_power_until(p: &mut Parser, min_bp: u8, stop: Option<SyntaxKind>) {
    let mut lhs = match lhs(p) {
        Some(m) => m,
        None => return,
    };
    lhs = parse_postfixes(p, lhs);

    loop {
        let op = p.peek();
        if stop == Some(op) {
            break;
        }
        if op == SyntaxKind::Ident && p.peek_text() == "otherwise" {
            p.error_with_message_no_bump(
                "removed keyword `otherwise`; use `??` for Result fallback",
            );
            break;
        }
        if (op == SyntaxKind::DefaultKw || op == SyntaxKind::QuestionQuestion)
            && p.has_newline_before_next_token()
        {
            break;
        }
        if op == SyntaxKind::DefaultKw {
            p.error_with_message_no_bump(
                "`default` is only valid in match cases; use `??` for Result fallback",
            );
            break;
        }
        let (left_bp, right_bp) = match infix_binding_power(op) {
            Some(bp) => bp,
            None => break,
        };

        if left_bp < min_bp {
            break;
        }

        p.bump(); // consume operator
        let m = lhs.precede(p);
        expr_binding_power(p, right_bp);
        lhs = m.complete(p, SyntaxKind::BinExpr);
    }
}

fn lhs(p: &mut Parser) -> Option<crate::parser::CompletedMarker> {
    let m = p.start();
    match p.peek() {
        SyntaxKind::Minus
        | SyntaxKind::NotKw
        | SyntaxKind::ErrKw
        | SyntaxKind::AwaitKw
        | SyntaxKind::FireKw
        | SyntaxKind::BitwiseNot => {
            p.bump();
            expr_binding_power(p, 23);
            Some(m.complete(p, SyntaxKind::PrefixExpr))
        }
        SyntaxKind::CrashKw => parse_crash(p, m),
        SyntaxKind::DetachKw | SyntaxKind::SpawnKw => parse_detach(p, m),
        SyntaxKind::Ident => {
            if p.at_ident_text("it") || p.at_ident_text("its") {
                p.error_with_message_no_bump("removed receiver aliases `it`/`its`; use `self`");
            }
            p.bump();
            Some(m.complete(p, SyntaxKind::IdentExpr))
        }
        SyntaxKind::SelfKw => {
            p.bump();
            Some(m.complete(p, SyntaxKind::IdentExpr))
        }
        SyntaxKind::IntNumber | SyntaxKind::FloatNumber => {
            p.bump();
            Some(m.complete(p, SyntaxKind::LiteralExpr))
        }
        SyntaxKind::TrueKw | SyntaxKind::FalseKw | SyntaxKind::NothingKw => {
            p.bump();
            Some(m.complete(p, SyntaxKind::LiteralExpr))
        }
        SyntaxKind::StringLiteral => {
            p.bump();
            Some(m.complete(p, SyntaxKind::LiteralExpr))
        }
        SyntaxKind::StringStart => parse_string_interp(p, m),
        SyntaxKind::LParen => {
            p.bump();
            expr(p);
            p.expect(SyntaxKind::RParen);
            Some(m.complete(p, SyntaxKind::ParenExpr))
        }
        SyntaxKind::LBracket => parse_list(p, m),
        SyntaxKind::LBrace => parse_map(p, m),
        _ => {
            if is_expr_recovery_token(p.peek()) {
                p.error_no_bump();
            } else {
                p.error();
            }
            None
        }
    }
}

fn is_expr_recovery_token(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::RParen
            | SyntaxKind::RBracket
            | SyntaxKind::RBrace
            | SyntaxKind::Newline
            | SyntaxKind::Eof
            | SyntaxKind::Comma
            | SyntaxKind::Colon
    )
}

fn infix_binding_power(kind: SyntaxKind) -> Option<(u8, u8)> {
    match kind {
        SyntaxKind::Equals
        | SyntaxKind::PlusEq
        | SyntaxKind::MinusEq
        | SyntaxKind::StarEq
        | SyntaxKind::SlashEq => Some((0, 0)),
        SyntaxKind::QuestionQuestion => Some((0, 1)),
        SyntaxKind::OrKw => Some((1, 2)),
        SyntaxKind::AndKw => Some((3, 4)),
        SyntaxKind::Pipe => Some((5, 6)),
        SyntaxKind::Caret => Some((7, 8)),
        SyntaxKind::Ampersand => Some((9, 10)),
        SyntaxKind::EqEq | SyntaxKind::BangEq => Some((11, 12)),
        SyntaxKind::Less | SyntaxKind::LessEq | SyntaxKind::Greater | SyntaxKind::GreaterEq => {
            Some((13, 14))
        }
        SyntaxKind::Range => Some((15, 16)),
        SyntaxKind::ShiftLeft | SyntaxKind::ShiftRight => Some((17, 18)),
        SyntaxKind::Plus | SyntaxKind::Minus => Some((19, 20)),
        SyntaxKind::Star | SyntaxKind::Slash | SyntaxKind::Percent => Some((21, 22)),
        _ => None,
    }
}

fn parse_crash(p: &mut Parser, m: crate::parser::Marker) -> Option<crate::parser::CompletedMarker> {
    p.expect(SyntaxKind::CrashKw);
    if p.at(SyntaxKind::LParen) {
        p.bump();
        expr(p);
        p.expect(SyntaxKind::RParen);
    } else {
        p.error_with_message_no_bump("expected '(' after crash");
    }
    Some(m.complete(p, SyntaxKind::CrashExpr))
}

fn parse_postfixes(
    p: &mut Parser,
    mut lhs: crate::parser::CompletedMarker,
) -> crate::parser::CompletedMarker {
    loop {
        if p.at_stmt_boundary() {
            break;
        }
        if p.at(SyntaxKind::LParen) {
            lhs = parse_call(p, lhs);
            continue;
        }
        if p.at(SyntaxKind::LBracket) {
            if should_parse_type_apply(p) {
                lhs = parse_type_apply(p, lhs);
            } else {
                lhs = parse_index(p, lhs);
            }
            continue;
        }
        if p.at(SyntaxKind::Dot) {
            lhs = parse_member(p, lhs);
            continue;
        }
        if p.at(SyntaxKind::GivenKw) {
            parse_legacy_given_error(p);
            continue;
        }
        if p.at(SyntaxKind::Question) {
            lhs = parse_try(p, lhs);
            continue;
        }
        break;
    }
    lhs
}

fn parse_try(
    p: &mut Parser,
    lhs: crate::parser::CompletedMarker,
) -> crate::parser::CompletedMarker {
    let m = lhs.precede(p);
    p.expect(SyntaxKind::Question);
    m.complete(p, SyntaxKind::TryExpr)
}

fn parse_legacy_given_error(p: &mut Parser) {
    p.error_with_message_no_bump(
        "legacy `given` call syntax is not supported; use standard call syntax",
    );
    p.expect(SyntaxKind::GivenKw);
    if p.at_stmt_boundary()
        || p.at(SyntaxKind::Colon)
        || p.at(SyntaxKind::RParen)
        || p.at(SyntaxKind::RBracket)
        || p.at(SyntaxKind::Comma)
    {
        return;
    }
    let mut first = true;
    while !p.at_stmt_boundary() && !p.is_at_eof() {
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                break;
            }
        }
        parse_arg(p);
        first = false;
        if !p.at(SyntaxKind::Comma) && p.at_stmt_boundary() {
            break;
        }
    }
}

fn parse_type_apply(
    p: &mut Parser,
    lhs: crate::parser::CompletedMarker,
) -> crate::parser::CompletedMarker {
    let m = lhs.precede(p);
    parse_type_args(p);
    m.complete(p, SyntaxKind::TypeApplyExpr)
}

fn parse_index(
    p: &mut Parser,
    lhs: crate::parser::CompletedMarker,
) -> crate::parser::CompletedMarker {
    let m = lhs.precede(p);
    p.expect(SyntaxKind::LBracket);
    expr(p);
    p.expect(SyntaxKind::RBracket);
    m.complete(p, SyntaxKind::IndexExpr)
}

fn should_parse_type_apply(p: &Parser) -> bool {
    let mut depth = 0usize;
    let mut offset = 0usize;
    loop {
        let token = p.peek_nontrivia_at(offset);
        match token {
            SyntaxKind::LBracket => depth += 1,
            SyntaxKind::RBracket => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
                if depth == 0 {
                    let next = p.peek_nontrivia_at(offset + 1);
                    return matches!(next, SyntaxKind::LParen);
                }
            }
            SyntaxKind::Eof => return false,
            _ => {}
        }
        offset += 1;
    }
}

fn parse_type_args(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::LBracket);
    if !p.at(SyntaxKind::RBracket) {
        loop {
            types::parse_type(p);
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

fn parse_detach(
    p: &mut Parser,
    m: crate::parser::Marker,
) -> Option<crate::parser::CompletedMarker> {
    if p.at(SyntaxKind::DetachKw) {
        p.bump();
    } else {
        p.expect(SyntaxKind::SpawnKw);
    }
    if !p.at(SyntaxKind::Ident) {
        p.error();
        return Some(m.complete(p, SyntaxKind::PrefixExpr));
    }
    let ident_m = p.start();
    p.expect(SyntaxKind::Ident);
    let lhs = ident_m.complete(p, SyntaxKind::IdentExpr);
    let _ = parse_postfixes(p, lhs);
    if p.at(SyntaxKind::Star) {
        p.bump();
        if p.at(SyntaxKind::IntNumber) || p.at(SyntaxKind::Ident) {
            p.bump();
        } else {
            p.error_with_message_no_bump("expected pool size after '*'");
        }
    } else {
        p.error_with_message_no_bump("expected '*' after detach");
    }
    Some(m.complete(p, SyntaxKind::PrefixExpr))
}

fn parse_call(
    p: &mut Parser,
    lhs: crate::parser::CompletedMarker,
) -> crate::parser::CompletedMarker {
    let m = lhs.precede(p);
    p.expect(SyntaxKind::LParen);
    let mut first = true;
    while !p.at(SyntaxKind::RParen) && !p.is_at_eof() {
        let before = p.cursor_pos();
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RParen, SyntaxKind::Newline]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RParen) {
                    break;
                }
            }
        }
        parse_arg(p);
        if !p.at(SyntaxKind::Comma) && !p.at(SyntaxKind::RParen) {
            p.error_no_bump();
            p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RParen, SyntaxKind::Newline]);
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else if p.at(SyntaxKind::RParen) {
                break;
            }
        }
        if p.cursor_pos() == before {
            p.error();
        }
        first = false;
    }
    if p.at(SyntaxKind::RParen) {
        p.bump();
    } else {
        p.expect(SyntaxKind::RParen);
    }
    m.complete(p, SyntaxKind::CallExpr)
}

fn parse_arg(p: &mut Parser) {
    if is_name_like_label_token(p.peek()) && p.peek_nontrivia_at(1) == SyntaxKind::Equals {
        let m = p.start();
        p.bump();
        p.expect(SyntaxKind::Equals);
        expr(p);
        m.complete(p, SyntaxKind::NamedArg);
        return;
    }
    expr(p);
}

fn parse_member(
    p: &mut Parser,
    lhs: crate::parser::CompletedMarker,
) -> crate::parser::CompletedMarker {
    let m = lhs.precede(p);
    p.expect(SyntaxKind::Dot);
    p.expect(SyntaxKind::Ident);
    m.complete(p, SyntaxKind::MemberExpr)
}

fn parse_list(p: &mut Parser, m: crate::parser::Marker) -> Option<crate::parser::CompletedMarker> {
    p.expect(SyntaxKind::LBracket);
    let mut first = true;
    while !p.at(SyntaxKind::RBracket) && !p.is_at_eof() {
        let before = p.cursor_pos();
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RBracket, SyntaxKind::Newline]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RBracket) {
                    break;
                }
            }
        }
        expr(p);
        if !p.at(SyntaxKind::Comma) && !p.at(SyntaxKind::RBracket) {
            p.error_no_bump();
            p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RBracket, SyntaxKind::Newline]);
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else if p.at(SyntaxKind::RBracket) {
                break;
            }
        }
        if p.cursor_pos() == before {
            p.error();
            if p.cursor_pos() == before {
                break;
            }
        }
        first = false;
    }
    p.expect(SyntaxKind::RBracket);
    Some(m.complete(p, SyntaxKind::ListExpr))
}

fn parse_map(p: &mut Parser, m: crate::parser::Marker) -> Option<crate::parser::CompletedMarker> {
    p.expect(SyntaxKind::LBrace);
    let mut first = true;
    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        let before = p.cursor_pos();
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RBrace, SyntaxKind::Newline]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RBrace) {
                    break;
                }
            }
        }
        expr(p);
        if p.at(SyntaxKind::Colon) {
            p.bump();
        } else {
            p.error_no_bump();
            p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RBrace, SyntaxKind::Newline]);
            if p.at(SyntaxKind::Comma) {
                p.bump();
                first = false;
                continue;
            } else if p.at(SyntaxKind::RBrace) {
                break;
            } else if p.cursor_pos() == before {
                p.error();
                if p.cursor_pos() == before {
                    break;
                }
            }
        }
        expr(p);
        if !p.at(SyntaxKind::Comma) && !p.at(SyntaxKind::RBrace) {
            p.error_no_bump();
            p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RBrace, SyntaxKind::Newline]);
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else if p.at(SyntaxKind::RBrace) {
                break;
            }
        }
        if p.cursor_pos() == before {
            p.error();
            if p.cursor_pos() == before {
                break;
            }
        }
        first = false;
    }
    p.expect(SyntaxKind::RBrace);
    Some(m.complete(p, SyntaxKind::MapExpr))
}

fn parse_string_interp(
    p: &mut Parser,
    m: crate::parser::Marker,
) -> Option<crate::parser::CompletedMarker> {
    p.expect(SyntaxKind::StringStart);
    loop {
        p.expect(SyntaxKind::LBrace);
        expr(p);
        p.expect(SyntaxKind::RBrace);
        if p.at(SyntaxKind::StringPart) {
            p.bump();
            continue;
        }
        break;
    }
    p.expect(SyntaxKind::StringEnd);
    Some(m.complete(p, SyntaxKind::StringInterp))
}
