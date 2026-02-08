pub mod class;
pub mod expr;
pub mod func;
pub mod types;

use crate::parser::Parser;
use crate::parser::kind::SyntaxKind;

pub fn root(p: &mut Parser) {
    let m = p.start();
    while !p.is_at_eof() {
        p.consume_trivia();
        if p.is_at_eof() {
            break;
        }
        let cursor = p.cursor_pos();
        parse_statement(p);
        if p.cursor_pos() == cursor {
            // Ensure forward progress to avoid infinite loops on unexpected tokens.
            p.error();
        }
    }
    m.complete(p, SyntaxKind::Root);
}

pub(crate) fn parse_statement(p: &mut Parser) {
    if p.at(SyntaxKind::PrivateKw) && p.peek_nontrivia_at(1) == SyntaxKind::Colon {
        parse_private_block(p);
        return;
    }
    if is_class_start(p) {
        class::class_def(p);
        return;
    }
    if is_func_start(p) {
        func::func_def(p);
        return;
    }
    if is_check_start(p) {
        func::check_def(p);
        return;
    }
    if is_var_assign_start(p) {
        parse_var_assign(p);
        return;
    }
    match p.peek() {
        SyntaxKind::IfKw => parse_if(p),
        SyntaxKind::WhileKw => parse_while(p),
        SyntaxKind::ForKw => parse_for(p),
        SyntaxKind::ReturnKw => parse_return(p),
        SyntaxKind::BreakKw => parse_break(p),
        SyntaxKind::ContinueKw => parse_continue(p),
        SyntaxKind::OptimizeKw => parse_optimize(p),
        SyntaxKind::AssertKw => parse_assert(p),
        SyntaxKind::RequireKw => parse_require(p),
        SyntaxKind::DeferKw => parse_defer(p),
        SyntaxKind::IgnoreKw => parse_ignore_result(p),
        SyntaxKind::CaptureKw => parse_capture(p),
        SyntaxKind::MatchKw => parse_match(p),
        SyntaxKind::UseKw => parse_use(p),
        SyntaxKind::Eof => (),
        _ => {
            let m_stmt = p.start();
            expr::expr(p);
            m_stmt.complete(p, SyntaxKind::StmtExpr);
            p.expect_stmt_boundary();
        }
    }
}

fn parse_assert(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::AssertKw);
    if p.at_ident_text("value") {
        p.bump();
        expr::expr(p);
        m.complete(p, SyntaxKind::AssertStmt);
        p.expect_stmt_boundary();
        return;
    }
    if p.at_ident_text("identity") {
        p.bump();
        expr::expr(p);
        m.complete(p, SyntaxKind::AssertStmt);
        p.expect_stmt_boundary();
        return;
    }
    p.error_with_message_no_bump("expected 'value' or 'identity' after assert");
    m.complete(p, SyntaxKind::AssertStmt);
    p.expect_stmt_boundary();
}

fn parse_var_assign(p: &mut Parser) {
    let m = p.start();
    let _is_mutable = if p.at(SyntaxKind::MutableKw) {
        p.bump();
        true
    } else {
        false
    };
    p.expect(SyntaxKind::Ident);

    match p.peek() {
        SyntaxKind::Equals
        | SyntaxKind::PlusEq
        | SyntaxKind::MinusEq
        | SyntaxKind::StarEq
        | SyntaxKind::SlashEq => {
            p.bump();
        }
        _ => {
            p.error_with_message("expected assignment operator", true);
        }
    }

    expr::expr(p);
    m.complete(p, SyntaxKind::VarAssign);
    p.expect_stmt_boundary();
}

fn parse_return(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::ReturnKw);
    if !p.at_stmt_boundary() {
        expr::expr(p);
    }
    m.complete(p, SyntaxKind::ReturnStmt);
    p.expect_stmt_boundary();
}

fn parse_break(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::BreakKw);
    m.complete(p, SyntaxKind::BreakStmt);
    p.expect_stmt_boundary();
}

fn parse_continue(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::ContinueKw);
    m.complete(p, SyntaxKind::ContinueStmt);
    p.expect_stmt_boundary();
}

fn parse_if(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::IfKw);
    expr::expr(p);
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after if condition");
    parse_block(p);
    if p.at(SyntaxKind::ButKw) {
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_if(p);
        } else {
            p.error_with_message_no_bump("expected 'if' after 'but'");
        }
    } else if p.at(SyntaxKind::OtherwiseKw) {
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_if(p);
        } else {
            p.expect_with_message(SyntaxKind::Colon, "expected ':' after otherwise");
            parse_block(p);
        }
    }
    m.complete(p, SyntaxKind::IfStmt);
}

fn parse_while(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::WhileKw);
    expr::expr(p);
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after while condition");
    parse_block(p);
    m.complete(p, SyntaxKind::WhileStmt);
}

fn parse_for(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::ForKw);
    p.expect_with_message(SyntaxKind::Ident, "expected loop variable after 'for'");
    p.expect_with_message(SyntaxKind::InKw, "expected 'in' after loop variable");
    expr::expr(p);
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after for loop header");
    parse_block(p);
    m.complete(p, SyntaxKind::ForStmt);
}

fn parse_match(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::MatchKw);
    expr::expr(p);
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after match expression");
    if p.at(SyntaxKind::Indent) {
        p.bump();
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            parse_match_case(p);
        }
        p.expect(SyntaxKind::Dedent);
    } else {
        p.error_expected_indented_block();
    }
    m.complete(p, SyntaxKind::MatchStmt);
}

fn parse_optimize(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::OptimizeKw);
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected optimization objective after 'optimize'",
    );
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after optimize header");
    parse_block(p);
    m.complete(p, SyntaxKind::OptimizeStmt);
}

fn parse_defer(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::DeferKw);
    if !p.at_stmt_boundary() {
        expr::expr(p);
    } else {
        p.error_with_message_no_bump("expected expression after 'defer'");
    }
    m.complete(p, SyntaxKind::DeferStmt);
    p.expect_stmt_boundary();
}

fn parse_ignore_result(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::IgnoreKw);
    if p.at_ident_text("result") {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected 'result' after 'ignore'");
    }
    if !p.at_stmt_boundary() {
        expr::expr(p);
    } else {
        p.error_with_message_no_bump("expected expression after 'ignore result'");
    }
    m.complete(p, SyntaxKind::IgnoreResultStmt);
    p.expect_stmt_boundary();
}

fn parse_capture(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::CaptureKw);
    p.expect_with_message(SyntaxKind::Ident, "expected variable name after 'capture'");
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after capture name");
    if !p.at_stmt_boundary() {
        expr::expr(p);
    } else {
        p.error_with_message_no_bump("expected expression after 'capture'");
    }
    m.complete(p, SyntaxKind::CaptureStmt);
    p.expect_stmt_boundary();
}

fn parse_match_case(p: &mut Parser) {
    if p.at(SyntaxKind::OtherwiseKw) || p.at_ident_text("otherwise") {
        let m = p.start();
        p.bump();
        p.expect_with_message(SyntaxKind::Colon, "expected ':' after otherwise");
        parse_case_body(p);
        m.complete(p, SyntaxKind::OtherwiseCase);
        return;
    }

    let m = p.start();
    parse_case_label(p);
    while p.at(SyntaxKind::Comma) {
        p.bump();
        parse_case_label(p);
    }
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after match case");
    parse_case_body(p);
    m.complete(p, SyntaxKind::MatchCase);
}

fn parse_case_label(p: &mut Parser) {
    parse_pattern(p);
}

fn parse_case_body(p: &mut Parser) {
    if p.at(SyntaxKind::Indent) {
        parse_block(p);
    } else {
        parse_statement(p);
    }
}

fn parse_pattern(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::Ident) {
        p.bump();
        while p.at(SyntaxKind::Dot) {
            p.bump();
            p.expect_with_message(
                SyntaxKind::Ident,
                "expected identifier after '.' in pattern",
            );
        }
        if p.at(SyntaxKind::LParen) {
            parse_pattern_arg_list(p);
        }
        m.complete(p, SyntaxKind::Pattern);
        return;
    }
    if matches!(
        p.peek(),
        SyntaxKind::StringLiteral
            | SyntaxKind::IntNumber
            | SyntaxKind::FloatNumber
            | SyntaxKind::TrueKw
            | SyntaxKind::FalseKw
            | SyntaxKind::NothingKw
    ) {
        p.bump();
        m.complete(p, SyntaxKind::Pattern);
        return;
    }
    p.error_with_message("expected pattern", true);
    m.complete(p, SyntaxKind::Pattern);
}

fn parse_pattern_arg_list(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::LParen);
    let mut first = true;
    while !p.at(SyntaxKind::RParen) && !p.is_at_eof() {
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RParen]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RParen) {
                    break;
                }
            }
        }
        parse_pattern(p);
        first = false;
    }
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after pattern arguments");
    m.complete(p, SyntaxKind::PatternArgList);
}

fn parse_use(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::UseKw);
    if p.at(SyntaxKind::Colon) {
        p.bump();
        if p.at(SyntaxKind::Indent) {
            p.bump();
            parse_use_list_block(p);
            p.expect(SyntaxKind::Dedent);
        } else {
            p.error_expected_indented_block();
        }
    } else {
        parse_use_list_inline(p);
    }
    p.expect_with_message(SyntaxKind::FromKw, "expected 'from' after use list");
    parse_module_path(p);
    m.complete(p, SyntaxKind::UseStmt);
    p.expect_stmt_boundary();
}

fn parse_use_list_inline(p: &mut Parser) {
    let mut first = true;
    while !p.at(SyntaxKind::FromKw) && !p.is_at_eof() {
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::FromKw]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::FromKw) {
                    break;
                }
            }
        }
        parse_use_name(p);
        first = false;
    }
}

fn parse_use_list_block(p: &mut Parser) {
    while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
        parse_use_name(p);
        if p.at(SyntaxKind::Comma) {
            p.bump();
            continue;
        }
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
            continue;
        }
        if !p.at(SyntaxKind::Dedent) {
            p.error_no_bump();
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Dedent, SyntaxKind::Comma]);
            if p.at_stmt_boundary() {
                p.expect_stmt_boundary();
            }
        }
    }
}

fn parse_use_name(p: &mut Parser) {
    if p.at(SyntaxKind::Ident) || p.at(SyntaxKind::Star) {
        p.bump();
    } else {
        p.error();
    }
}

fn parse_module_path(p: &mut Parser) {
    p.expect(SyntaxKind::Ident);
    while p.at(SyntaxKind::Slash) || p.at(SyntaxKind::Dot) {
        p.bump();
        p.expect(SyntaxKind::Ident);
    }
}

pub(crate) fn parse_block(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::Indent) {
        p.bump();
        while !p.at(SyntaxKind::Dedent) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::Dedent) || p.is_at_eof() {
                break;
            }
            let cursor = p.cursor_pos();
            parse_statement(p);
            if p.cursor_pos() == cursor {
                // Ensure forward progress to avoid infinite loops on unexpected tokens.
                p.error();
            }
        }
        p.expect(SyntaxKind::Dedent);
    } else {
        p.error_expected_indented_block();
    }
    m.complete(p, SyntaxKind::Block);
}

pub(crate) fn parse_param_list(p: &mut Parser) {
    let m = p.start();
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
        parse_param(p);
        first = false;
    }
    m.complete(p, SyntaxKind::ParamList);
}

fn parse_param(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::Ident);
    p.expect(SyntaxKind::Colon);
    types::parse_type(p);
    m.complete(p, SyntaxKind::Param);
}

fn is_class_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::ClassKw) || p.at(SyntaxKind::AnKw) {
        return true;
    }
    false
}

fn is_func_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::ToKw) {
        return true;
    }
    false
}

fn is_check_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::CheckKw) {
        return true;
    }
    false
}

fn parse_require(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::RequireKw);
    if p.at_stmt_boundary() {
        p.error_with_message_no_bump("expected expression after 'require'");
        m.complete(p, SyntaxKind::RequireStmt);
        p.expect_stmt_boundary();
        return;
    }
    expr::expr_until_otherwise(p);
    if p.at(SyntaxKind::OtherwiseKw) {
        p.bump();
        expr::expr(p);
    } else {
        p.error_with_message_no_bump("expected 'otherwise' after require condition");
    }
    m.complete(p, SyntaxKind::RequireStmt);
    p.expect_stmt_boundary();
}

fn is_var_assign_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::Ident) {
        let next = p.peek_nontrivia_at(1);
        return matches!(
            next,
            SyntaxKind::Equals
                | SyntaxKind::PlusEq
                | SyntaxKind::MinusEq
                | SyntaxKind::StarEq
                | SyntaxKind::SlashEq
        );
    }
    if p.at(SyntaxKind::MutableKw) {
        let next = p.peek_nontrivia_at(1);
        return next == SyntaxKind::Ident && p.peek_nontrivia_at(2) == SyntaxKind::Equals;
    }
    false
}

fn parse_private_block(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::PrivateKw);
    p.expect_with_message(SyntaxKind::Colon, "expected ':' after 'private'");
    parse_block(p);
    m.complete(p, SyntaxKind::PrivateBlock);
}
