use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn expr(p: &mut Parser) {
    expr_binding_power(p, 0);
}

fn expr_binding_power(p: &mut Parser, min_bp: u8) {
    let mut lhs = match lhs(p) {
        Some(m) => m,
        None => return,
    };
    lhs = parse_postfixes(p, lhs);

    loop {
        let op = p.peek();
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
        SyntaxKind::SpawnKw => parse_spawn(p, m),
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
        if p.at(SyntaxKind::LParen) {
            lhs = parse_call(p, lhs);
            continue;
        }
        if p.at(SyntaxKind::Dot) {
            lhs = parse_member(p, lhs);
            continue;
        }
        break;
    }
    lhs
}

fn parse_spawn(p: &mut Parser, m: crate::parser::Marker) -> Option<crate::parser::CompletedMarker> {
    p.expect(SyntaxKind::SpawnKw);
    if !p.at(SyntaxKind::Ident) {
        p.error();
        return Some(m.complete(p, SyntaxKind::PrefixExpr));
    }
    let ident_m = p.start();
    p.expect(SyntaxKind::Ident);
    let mut lhs = ident_m.complete(p, SyntaxKind::IdentExpr);
    if p.at(SyntaxKind::LParen) {
        lhs = parse_call(p, lhs);
        let _ = lhs;
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
        first = false;
    }
    p.expect(SyntaxKind::RParen);
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
