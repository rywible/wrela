use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;
use super::types;

pub fn expr(p: &mut Parser) {
    expr_binding_power(p, 0);
}

fn expr_binding_power(p: &mut Parser, min_bp: u8) {
    expr_binding_power_until(p, min_bp, None);
}

pub fn expr_until_otherwise(p: &mut Parser) {
    expr_binding_power_until(p, 0, Some(SyntaxKind::OtherwiseKw));
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
        if op == SyntaxKind::OtherwiseKw && p.has_newline_before_next_token() {
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
            p.bump();
            Some(m.complete(p, SyntaxKind::IdentExpr))
        }
        SyntaxKind::ItsKw => {
            p.bump();
            Some(m.complete(p, SyntaxKind::ItsExpr))
        }
        SyntaxKind::ItKw => {
            p.bump();
            Some(m.complete(p, SyntaxKind::ItExpr))
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
            | SyntaxKind::Dedent
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
        SyntaxKind::OtherwiseKw => Some((0, 1)),
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
            lhs = parse_type_apply(p, lhs);
            continue;
        }
        if p.at(SyntaxKind::Dot) {
            lhs = parse_member(p, lhs);
            continue;
        }
        if p.at(SyntaxKind::GivenKw) {
            lhs = parse_given(p, lhs);
            continue;
        }
        break;
    }
    lhs
}

fn parse_given(
    p: &mut Parser,
    lhs: crate::parser::CompletedMarker,
) -> crate::parser::CompletedMarker {
    let m = lhs.precede(p);
    p.expect(SyntaxKind::GivenKw);
    if p.at_stmt_boundary()
        || p.at(SyntaxKind::Colon)
        || p.at(SyntaxKind::RParen)
        || p.at(SyntaxKind::RBracket)
        || p.at(SyntaxKind::Comma)
    {
        return m.complete(p, SyntaxKind::GivenExpr);
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
        if !p.at(SyntaxKind::Comma) {
            if p.at_stmt_boundary() {
                break;
            }
        }
    }
    m.complete(p, SyntaxKind::GivenExpr)
}

fn parse_type_apply(
    p: &mut Parser,
    lhs: crate::parser::CompletedMarker,
) -> crate::parser::CompletedMarker {
    let m = lhs.precede(p);
    parse_type_args(p);
    m.complete(p, SyntaxKind::TypeApplyExpr)
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
    if p.at(SyntaxKind::OptimizeKw) {
        p.bump();
        if p.at(SyntaxKind::Ident) {
            p.bump();
        } else {
            p.error_with_message_no_bump("expected objective after optimize");
        }
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
    let mut saw_dedent = false;
    while !p.at(SyntaxKind::RParen) && !p.is_at_eof() {
        if p.at(SyntaxKind::Dedent) {
            saw_dedent = true;
            break;
        }
        let before = p.cursor_pos();
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[
                    SyntaxKind::Comma,
                    SyntaxKind::RParen,
                    SyntaxKind::Newline,
                    SyntaxKind::Dedent,
                ]);
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
            p.recover_until(&[
                SyntaxKind::Comma,
                SyntaxKind::RParen,
                SyntaxKind::Newline,
                SyntaxKind::Dedent,
            ]);
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
    } else if saw_dedent {
        p.error_with_message_no_bump("expected ')'");
    } else {
        p.expect(SyntaxKind::RParen);
    }
    m.complete(p, SyntaxKind::CallExpr)
}

fn parse_arg(p: &mut Parser) {
    if p.at(SyntaxKind::Ident) && p.peek_nontrivia_at(1) == SyntaxKind::Equals {
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
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[
                    SyntaxKind::Comma,
                    SyntaxKind::RBracket,
                    SyntaxKind::Newline,
                    SyntaxKind::Dedent,
                ]);
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
            p.recover_until(&[
                SyntaxKind::Comma,
                SyntaxKind::RBracket,
                SyntaxKind::Newline,
                SyntaxKind::Dedent,
            ]);
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else if p.at(SyntaxKind::RBracket) {
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
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[
                    SyntaxKind::Comma,
                    SyntaxKind::RBrace,
                    SyntaxKind::Newline,
                    SyntaxKind::Dedent,
                ]);
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
            p.recover_until(&[
                SyntaxKind::Comma,
                SyntaxKind::RBrace,
                SyntaxKind::Newline,
                SyntaxKind::Dedent,
            ]);
            if p.at(SyntaxKind::Comma) || p.at(SyntaxKind::RBrace) {
                continue;
            }
        }
        expr(p);
        if !p.at(SyntaxKind::Comma) && !p.at(SyntaxKind::RBrace) {
            p.error_no_bump();
            p.recover_until(&[
                SyntaxKind::Comma,
                SyntaxKind::RBrace,
                SyntaxKind::Newline,
                SyntaxKind::Dedent,
            ]);
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else if p.at(SyntaxKind::RBrace) {
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
