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
        parse_root_statement(p);
        if p.cursor_pos() == cursor {
            // Ensure forward progress to avoid infinite loops on unexpected tokens.
            p.error();
        }
    }
    m.complete(p, SyntaxKind::Root);
}

pub(crate) fn parse_statement(p: &mut Parser) {
    if func::attributed_func_or_check_def(p) {
        return;
    }
    if p.at(SyntaxKind::PrivateKw) {
        parse_private_block(p);
        return;
    }
    if p.at_ident_text("shape") {
        func::shape_decl(p);
        return;
    }
    if p.at(SyntaxKind::ResourceKw) {
        class::resource_def(p);
        return;
    }
    if p.at(SyntaxKind::EventKw) {
        class::event_def(p);
        return;
    }
    if is_value_start(p) {
        class::value_def(p);
        return;
    }
    if is_class_start(p) {
        class::class_def(p);
        return;
    }
    if p.at(SyntaxKind::KernelKw) {
        func::kernel_def(p);
        return;
    }
    if p.at(SyntaxKind::SystemKw) {
        func::system_def(p);
        return;
    }
    if is_func_start(p) {
        func::func_def(p);
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

fn parse_root_statement(p: &mut Parser) {
    if func::attributed_func_or_check_def(p) {
        return;
    }
    if p.at_ident_text("region") {
        func::region_decl(p);
        return;
    }
    if p.at_ident_text("domain") {
        func::domain_decl(p);
        return;
    }
    if p.at_ident_text("render") {
        func::render_decl(p);
        return;
    }
    if p.at_ident_text("radiance") {
        func::radiance_decl(p);
        return;
    }
    if p.at_ident_text("volume") {
        func::volume_decl(p);
        return;
    }
    if p.at_ident_text("field") {
        func::field_decl(p);
        return;
    }
    if p.at_ident_text("material") {
        func::material_decl(p);
        return;
    }
    if p.at_ident_text("shape") {
        func::shape_decl(p);
        return;
    }
    parse_statement(p);
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
    if p.at_ident_text("approx") {
        p.bump();
        expr::expr(p);
        p.expect_with_message(
            SyntaxKind::BitwiseNot,
            "expected '~=' after left-hand approximate assertion value",
        );
        p.expect_with_message(
            SyntaxKind::Equals,
            "expected '=' after '~' in approximate assertion",
        );
        expr::expr(p);
        if p.at_ident_text("within") {
            p.bump();
            expr::expr(p);
        } else {
            p.error_with_message_no_bump(
                "expected 'within' and a tolerance after approximate assertion",
            );
        }
        m.complete(p, SyntaxKind::AssertStmt);
        p.expect_stmt_boundary();
        return;
    }
    p.error_with_message_no_bump("expected 'value', 'identity', or 'approx' after assert");
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
    if p.at_ident_text("it") || p.at_ident_text("its") {
        p.error_with_message_no_bump("removed receiver aliases `it`/`its`; use `self`");
    }
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
    expect_block_intro(p, "expected '{' after if condition");
    parse_block(p);
    if p.at(SyntaxKind::ElseKw) {
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_if(p);
        } else {
            expect_block_intro(p, "expected '{' after else");
            parse_block(p);
        }
    } else if p.at_ident_text("but") {
        p.error_with_message_no_bump("`but if` was removed; use `else if`");
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_if(p);
        }
    } else if p.at(SyntaxKind::DefaultKw) {
        p.error_with_message_no_bump("`otherwise` was removed from control-flow; use `else`");
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_if(p);
        } else {
            if p.at(SyntaxKind::Colon) {
                p.bump();
            }
            if p.at(SyntaxKind::LBrace) {
                parse_block(p);
            }
        }
    } else if p.at_ident_text("otherwise") {
        p.error_with_message_no_bump("`otherwise` was removed from control-flow; use `else`");
        p.bump();
        if p.at(SyntaxKind::IfKw) {
            parse_if(p);
        } else {
            if p.at(SyntaxKind::Colon) {
                p.bump();
            }
            if p.at(SyntaxKind::LBrace) {
                parse_block(p);
            }
        }
    }
    m.complete(p, SyntaxKind::IfStmt);
}

fn parse_while(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::WhileKw);
    expr::expr(p);
    expect_block_intro(p, "expected '{' after while condition");
    parse_block(p);
    m.complete(p, SyntaxKind::WhileStmt);
}

fn parse_for(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::ForKw);
    p.expect_with_message(SyntaxKind::Ident, "expected loop variable after 'for'");
    if p.at(SyntaxKind::Comma) {
        p.bump();
        p.expect_with_message(
            SyntaxKind::Ident,
            "expected map value variable after ',' in for loop",
        );
    }
    p.expect_with_message(SyntaxKind::InKw, "expected 'in' after loop variable");
    expr::expr(p);
    if p.at_ident_text("with") {
        p.bump();
        if p.at_ident_text("index") {
            p.bump();
            p.expect_with_message(
                SyntaxKind::Ident,
                "expected index variable after 'with index'",
            );
        } else {
            p.error_with_message_no_bump("expected 'index' after 'with'");
        }
    }
    expect_block_intro(p, "expected '{' after for loop header");
    parse_block(p);
    m.complete(p, SyntaxKind::ForStmt);
}

fn parse_match(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::MatchKw);
    expr::expr(p);
    expect_block_intro(p, "expected '{' after match expression");
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            let cursor = p.cursor_pos();
            parse_match_case(p);
            if p.cursor_pos() == cursor {
                p.error();
            }
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after match expression");
    }
    m.complete(p, SyntaxKind::MatchStmt);
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
    if p.at(SyntaxKind::DefaultKw) {
        let m = p.start();
        p.bump();
        expect_block_intro(p, "expected '{' after default");
        parse_case_body(p);
        m.complete(p, SyntaxKind::OtherwiseCase);
        return;
    }
    if p.at_ident_text("otherwise") {
        let m = p.start();
        p.error_with_message_no_bump("`otherwise` was removed from match; use `default`");
        p.bump();
        if p.at(SyntaxKind::Colon) {
            p.bump();
        }
        parse_case_body(p);
        m.complete(p, SyntaxKind::OtherwiseCase);
        return;
    }

    let m = p.start();
    parse_case_label(p);
    while p.at(SyntaxKind::Comma) || p.at(SyntaxKind::Pipe) {
        p.bump();
        parse_case_label(p);
    }
    if p.at(SyntaxKind::IfKw) {
        p.bump();
        if p.at_stmt_boundary() {
            p.error_with_message_no_bump("expected guard expression after 'if' in match case");
        } else {
            expr::expr(p);
        }
    }
    expect_block_intro(p, "expected '{' after match case");
    parse_case_body(p);
    m.complete(p, SyntaxKind::MatchCase);
}

fn parse_case_label(p: &mut Parser) {
    parse_pattern(p);
}

fn parse_case_body(p: &mut Parser) {
    if p.at(SyntaxKind::LBrace) {
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
        if p.at(SyntaxKind::LBrace) && looks_like_pattern_field_list(p) {
            parse_pattern_field_list(p);
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
        let before = p.cursor_pos();
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
        if p.cursor_pos() == before {
            p.error();
            if p.cursor_pos() == before {
                break;
            }
        }
        first = false;
    }
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after pattern arguments");
    m.complete(p, SyntaxKind::PatternArgList);
}

fn parse_pattern_field_list(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::LBrace);
    let mut first = true;
    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        let before = p.cursor_pos();
        if !first {
            if p.at(SyntaxKind::Comma) {
                p.bump();
            } else {
                p.error_no_bump();
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RBrace]);
                if p.at(SyntaxKind::Comma) {
                    p.bump();
                } else if p.at(SyntaxKind::RBrace) {
                    break;
                }
            }
        }
        parse_pattern_field(p);
        if p.cursor_pos() == before {
            p.error();
            if p.cursor_pos() == before {
                break;
            }
        }
        first = false;
    }
    p.expect_with_message(SyntaxKind::RBrace, "expected '}' after pattern fields");
    m.complete(p, SyntaxKind::PatternFieldList);
}

fn looks_like_pattern_field_list(p: &Parser) -> bool {
    if !p.at(SyntaxKind::LBrace) {
        return false;
    }
    let first = p.peek_nth_non_trivia(1);
    if first == SyntaxKind::RBrace {
        return true;
    }
    if first != SyntaxKind::Ident {
        return false;
    }
    let second = p.peek_nth_non_trivia(2);
    matches!(
        second,
        SyntaxKind::Comma | SyntaxKind::Colon | SyntaxKind::RBrace
    )
}

fn parse_pattern_field(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected field name in structural pattern",
    );
    if p.at(SyntaxKind::Colon) {
        p.bump();
        parse_pattern(p);
    }
    m.complete(p, SyntaxKind::PatternField);
}

fn parse_use(p: &mut Parser) {
    let m = p.start();
    p.expect(SyntaxKind::UseKw);
    if p.at(SyntaxKind::Colon) {
        p.error_with_message_no_bump("':' block introducer was removed; use `{ ... }`");
        p.bump();
    }
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        parse_use_list_block(p, SyntaxKind::RBrace);
        p.expect(SyntaxKind::RBrace);
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

fn parse_use_list_block(p: &mut Parser, close: SyntaxKind) {
    while !p.at(close) && !p.is_at_eof() {
        parse_use_name(p);
        if p.at(SyntaxKind::Comma) {
            p.bump();
            continue;
        }
        if p.at_stmt_boundary() {
            p.expect_stmt_boundary();
            continue;
        }
        if !p.at(close) {
            p.error_no_bump();
            p.recover_until(&[SyntaxKind::Newline, close, SyntaxKind::Comma]);
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

fn parse_module_path_segment(p: &mut Parser) {
    if p.at(SyntaxKind::Ident) || p.at(SyntaxKind::DefaultKw) {
        p.bump();
    } else {
        p.error();
    }
}

fn parse_module_path(p: &mut Parser) {
    parse_module_path_segment(p);
    while p.at(SyntaxKind::Slash) || p.at(SyntaxKind::Dot) {
        p.bump();
        parse_module_path_segment(p);
    }
}

pub(crate) fn parse_block(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            let cursor = p.cursor_pos();
            parse_statement(p);
            if p.cursor_pos() == cursor {
                p.error();
            }
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' to start a block");
    }
    m.complete(p, SyntaxKind::Block);
}

pub(crate) fn expect_block_intro(p: &mut Parser, colon_error: &str) {
    if p.at(SyntaxKind::LBrace) {
        return;
    }
    if p.at(SyntaxKind::Colon) {
        p.error_with_message_no_bump("':' block introducer was removed; use braces: `{ ... }`");
        p.bump();
        return;
    }
    p.error_with_message(colon_error, true);
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
                p.recover_until(&[SyntaxKind::Comma, SyntaxKind::RParen, SyntaxKind::Newline]);
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
    if p.at(SyntaxKind::ClassKw) || p.at(SyntaxKind::InterfaceKw) || p.at(SyntaxKind::EnumKw) {
        return true;
    }
    false
}

fn is_value_start(p: &mut Parser) -> bool {
    p.at_ident_text("value")
        && p.peek_nth_non_trivia(1) == SyntaxKind::Ident
        && matches!(
            p.peek_nth_non_trivia(2),
            SyntaxKind::LBrace | SyntaxKind::LBracket
        )
}

fn is_func_start(p: &mut Parser) -> bool {
    p.at(SyntaxKind::FnKw) || p.at(SyntaxKind::SystemKw)
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
    if p.at(SyntaxKind::ElseKw) {
        p.bump();
        expr::expr(p);
    } else if p.at(SyntaxKind::DefaultKw) || p.at_ident_text("otherwise") {
        p.error_with_message_no_bump("`otherwise` was removed from control-flow; use `else`");
        p.bump();
        expr::expr(p);
    } else {
        p.error_with_message_no_bump("expected 'else' after require condition");
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
    expect_block_intro(p, "expected '{' after 'private'");
    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            let cursor = p.cursor_pos();
            parse_root_statement(p);
            if p.cursor_pos() == cursor {
                p.error();
            }
        }
        p.expect(SyntaxKind::RBrace);
    } else {
        p.error_with_message_no_bump("expected '{' after 'private'");
    }
    m.complete(p, SyntaxKind::PrivateBlock);
}
