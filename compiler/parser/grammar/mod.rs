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
    if reject_removed_keyword_statement_head(p) {
        return;
    }
    if func::attributed_func_or_check_def(p) {
        return;
    }
    if p.at(SyntaxKind::PrivateKw) {
        parse_private_block(p);
        return;
    }
    if p.at(SyntaxKind::NodeKw) {
        class::node_def(p);
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
    if p.at(SyntaxKind::ThemeKw) {
        class::theme_def(p);
        return;
    }
    if p.at(SyntaxKind::AssetKw) {
        class::asset_decl(p);
        return;
    }
    if p.at(SyntaxKind::SceneKw) {
        class::scene_decl(p);
        return;
    }
    if is_class_start(p) {
        class::class_def(p);
        return;
    }
    if p.at(SyntaxKind::SystemKw) {
        func::system_def(p);
        return;
    }
    if p.at(SyntaxKind::ViewKw) {
        func::view_def(p);
        return;
    }
    if p.at(SyntaxKind::MaterialKw) {
        func::material_def(p);
        return;
    }
    if p.at(SyntaxKind::AnimKw) {
        func::anim_def(p);
        return;
    }
    if p.at(SyntaxKind::GpuKw) {
        func::gpu_func_def(p);
        return;
    }
    if p.at(SyntaxKind::ShaderKw) {
        func::shader_def(p);
        return;
    }
    if p.at(SyntaxKind::RenderKw) {
        parse_render_def(p);
        return;
    }
    if p.at(SyntaxKind::AssetsKw) {
        parse_assets_def(p);
        return;
    }
    if p.at(SyntaxKind::MmoKw) {
        parse_mmo_def(p);
        return;
    }
    if p.at(SyntaxKind::AssetSpecKw) {
        parse_asset_spec_def(p);
        return;
    }
    if p.at(SyntaxKind::StyleProfileKw) {
        parse_style_profile_def(p);
        return;
    }
    if p.at(SyntaxKind::GeneratorProfileKw) {
        parse_generator_profile_def(p);
        return;
    }
    if p.at(SyntaxKind::QualityProfileKw) {
        parse_quality_profile_def(p);
        return;
    }
    if p.at(SyntaxKind::ProvenancePolicyKw) {
        parse_provenance_policy_def(p);
        return;
    }
    if p.at(SyntaxKind::CharacterSpecKw) {
        parse_character_spec_def(p);
        return;
    }
    if p.at(SyntaxKind::RigSpecKw) {
        parse_rig_spec_def(p);
        return;
    }
    if p.at(SyntaxKind::AnimSetSpecKw) {
        parse_anim_set_spec_def(p);
        return;
    }
    if p.at(SyntaxKind::AudioSpecKw) {
        parse_audio_spec_def(p);
        return;
    }
    if p.at(SyntaxKind::VfxSpecKw) {
        parse_vfx_spec_def(p);
        return;
    }
    if p.at(SyntaxKind::UiSpecKw) {
        parse_ui_spec_def(p);
        return;
    }
    if p.at(SyntaxKind::WorldRecipeKw) {
        parse_world_recipe_def(p);
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

fn is_func_start(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::FnKw)
        || p.at(SyntaxKind::SystemKw)
        || p.at(SyntaxKind::ViewKw)
        || p.at(SyntaxKind::AnimKw)
        || p.at(SyntaxKind::GpuKw)
    {
        return true;
    }
    false
}

fn parse_render_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::RenderKw,
        "expected 'render' to start a render contract",
    );
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected render contract name after 'render'",
    );
    expect_block_intro(p, "expected '{' after render contract name");
    let mut saw_resources = false;
    let mut saw_temporal = false;
    let mut saw_quality_tier = false;
    let mut saw_budget_tags = false;

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("resources") {
                parse_render_resources_clause(p);
                saw_resources = true;
                continue;
            }
            if p.at_ident_text("temporal") {
                parse_render_temporal_clause(p);
                saw_temporal = true;
                continue;
            }
            if p.at_ident_text("quality") {
                parse_render_quality_tier_clause(p);
                saw_quality_tier = true;
                continue;
            }
            if p.at_ident_text("budget") {
                parse_render_budget_tags_clause(p);
                saw_budget_tags = true;
                continue;
            }
            if p.at(SyntaxKind::ShaderKw) || p.at_ident_text("shader") {
                parse_render_shader_clause(p);
                continue;
            }
            if p.at(SyntaxKind::PresetKw)
                || p.at(SyntaxKind::ProfileKw)
                || p.at(SyntaxKind::OverridesKw)
                || p.at_ident_text("target")
            {
                parse_render_removed_legacy_clause(p);
                continue;
            }
            p.error_with_message_no_bump(
                "expected render v5 clause (`resources`, `temporal`, `quality tier`, or `budget tags`)",
            );
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace, SyntaxKind::Eof]);
            if p.at_stmt_boundary() {
                p.expect_stmt_boundary();
            }
        }
        p.expect(SyntaxKind::RBrace);
    }

    if !saw_resources {
        p.error_with_message_no_bump(
            "render v5 contract is missing required `resources <AssetsDeclaration>` clause; migrate legacy presets/targets to an explicit assets binding",
        );
    }
    if !saw_temporal {
        p.error_with_message_no_bump(
            "render v5 contract is missing required `temporal <mode>` clause; explicit temporal mode is mandatory in v5",
        );
    }
    if !saw_quality_tier {
        p.error_with_message_no_bump(
            "render v5 contract is missing required `quality tier <tier>` clause; implicit quality defaults were removed",
        );
    }
    if !saw_budget_tags {
        p.error_with_message_no_bump(
            "render v5 contract is missing required `budget tags <tag>[, <tag>...]` clause; at least one explicit budget tag is required",
        );
    }
    m.complete(p, SyntaxKind::RenderDef);
    p.expect_stmt_boundary();
}

fn parse_render_resources_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("resources") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `resources` clause");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected assets declaration name after `resources`");
    }
    m.complete(p, SyntaxKind::RenderResourcesClause);
    p.expect_stmt_boundary();
}

fn parse_render_temporal_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("temporal") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `temporal` clause");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected temporal mode after `temporal`");
    }
    m.complete(p, SyntaxKind::RenderTemporalClause);
    p.expect_stmt_boundary();
}

fn parse_render_quality_tier_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("quality") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `quality tier` clause");
    }
    if p.at_ident_text("tier") {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected `tier` after `quality`");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected quality tier value after `quality tier`");
    }
    m.complete(p, SyntaxKind::RenderQualityTierClause);
    p.expect_stmt_boundary();
}

fn parse_render_budget_tags_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("budget") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `budget tags` clause");
    }
    if p.at_ident_text("tags") {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected `tags` after `budget` in `budget tags` clause");
    }

    let mut saw_tag = false;
    loop {
        if is_declaration_clause_value_start(p.peek()) {
            saw_tag = true;
            p.bump();
        } else if !saw_tag {
            p.error_with_message_no_bump("expected at least one budget tag after `budget tags`");
            break;
        } else {
            break;
        }
        if p.at(SyntaxKind::Comma) {
            p.bump();
            continue;
        }
        break;
    }

    m.complete(p, SyntaxKind::RenderBudgetTagsClause);
    p.expect_stmt_boundary();
}

fn parse_render_shader_clause(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::ShaderKw) || p.at_ident_text("shader") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `shader` clause");
    }
    if p.at_ident_text("generated") {
        p.bump();
        m.complete(p, SyntaxKind::RenderShaderClause);
        p.expect_stmt_boundary();
        return;
    }
    if p.at(SyntaxKind::MaterialKw) || p.at_ident_text("material") {
        p.bump();
        if is_declaration_clause_value_start(p.peek()) {
            p.bump();
        } else {
            p.error_with_message_no_bump(
                "expected material declaration name after `shader material`",
            );
        }
        m.complete(p, SyntaxKind::RenderShaderClause);
        p.expect_stmt_boundary();
        return;
    }
    if p.at(SyntaxKind::GpuKw) || p.at_ident_text("gpu") {
        p.bump();
        if is_declaration_clause_value_start(p.peek()) {
            p.bump();
        } else {
            p.error_with_message_no_bump("expected gpu function name after `shader gpu`");
        }
        m.complete(p, SyntaxKind::RenderShaderClause);
        p.expect_stmt_boundary();
        return;
    }
    p.error_with_message_no_bump(
        "expected shader mode (`generated`, `material <Name>`, or `gpu <FunctionName>`)",
    );
    p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace, SyntaxKind::Eof]);
    m.complete(p, SyntaxKind::RenderShaderClause);
    p.expect_stmt_boundary();
}

fn is_declaration_clause_value_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Ident
            | SyntaxKind::StringLiteral
            | SyntaxKind::RenderKw
            | SyntaxKind::PresetKw
            | SyntaxKind::ProfileKw
            | SyntaxKind::OverridesKw
            | SyntaxKind::MaterialKw
            | SyntaxKind::GpuKw
            | SyntaxKind::AssetsKw
            | SyntaxKind::MmoKw
            | SyntaxKind::AssetSpecKw
            | SyntaxKind::StyleProfileKw
            | SyntaxKind::GeneratorProfileKw
            | SyntaxKind::QualityProfileKw
            | SyntaxKind::ProvenancePolicyKw
            | SyntaxKind::CharacterSpecKw
            | SyntaxKind::RigSpecKw
            | SyntaxKind::AnimSetSpecKw
            | SyntaxKind::AudioSpecKw
            | SyntaxKind::VfxSpecKw
            | SyntaxKind::UiSpecKw
            | SyntaxKind::WorldRecipeKw
    )
}

fn parse_render_removed_legacy_clause(p: &mut Parser) {
    if p.at(SyntaxKind::PresetKw) {
        p.error_with_message_no_bump(
            "legacy render clause `preset` was removed in v5; migrate to `resources <AssetsDeclaration>` and `quality tier <tier>`",
        );
    } else if p.at(SyntaxKind::ProfileKw) {
        p.error_with_message_no_bump(
            "legacy render clause `profile` was removed in v5; migrate to `temporal <mode>` and `quality tier <tier>`",
        );
    } else if p.at_ident_text("target") {
        p.error_with_message_no_bump(
            "legacy render clause `target` was removed in v5; express asset binding with `resources <AssetsDeclaration>`",
        );
    } else if p.at(SyntaxKind::OverridesKw) {
        p.error_with_message_no_bump(
            "legacy render clause `overrides` was removed in v5; migrate constraints to `budget tags <tag>[, ...]`",
        );
    }
    p.bump();
    p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace, SyntaxKind::Eof]);
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
}

fn parse_assets_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::AssetsKw,
        "expected 'assets' to start an assets declaration",
    );
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected assets declaration name after 'assets'",
    );
    expect_block_intro(p, "expected '{' after assets declaration name");
    let mut saw_manifest = false;
    let mut saw_streaming = false;

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("manifest") {
                parse_assets_manifest_clause(p);
                saw_manifest = true;
                continue;
            }
            if p.at_ident_text("streaming") {
                parse_assets_streaming_clause(p);
                saw_streaming = true;
                continue;
            }
            p.error_with_message_no_bump("expected assets clause (`manifest` or `streaming`)");
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace, SyntaxKind::Eof]);
            if p.at(SyntaxKind::Newline) {
                p.bump();
            }
        }
        p.expect(SyntaxKind::RBrace);
    }

    if !saw_manifest {
        p.error_with_message_no_bump(
            "assets declaration is missing required `manifest <id>` clause",
        );
    }
    if !saw_streaming {
        p.error_with_message_no_bump(
            "assets declaration is missing required `streaming <id>` clause",
        );
    }
    m.complete(p, SyntaxKind::AssetsDef);
    p.expect_stmt_boundary();
}

fn parse_assets_manifest_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("manifest") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `manifest` clause");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected manifest id after `manifest`");
    }
    m.complete(p, SyntaxKind::AssetsManifestClause);
    p.expect_stmt_boundary();
}

fn parse_assets_streaming_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("streaming") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `streaming` clause");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected streaming id after `streaming`");
    }
    m.complete(p, SyntaxKind::AssetsStreamingClause);
    p.expect_stmt_boundary();
}

fn parse_mmo_def(p: &mut Parser) {
    let m = p.start();
    p.expect_with_message(
        SyntaxKind::MmoKw,
        "expected 'mmo' to start an mmo declaration",
    );
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected mmo declaration name after 'mmo'",
    );
    expect_block_intro(p, "expected '{' after mmo declaration name");
    let mut saw_gateway = false;
    let mut saw_zone = false;
    let mut saw_world = false;

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("gateway") {
                parse_mmo_gateway_clause(p);
                saw_gateway = true;
                continue;
            }
            if p.at_ident_text("zone") {
                parse_mmo_zone_clause(p);
                saw_zone = true;
                continue;
            }
            if p.at_ident_text("world") {
                parse_mmo_world_clause(p);
                saw_world = true;
                continue;
            }
            p.error_with_message_no_bump("expected mmo clause (`gateway`, `zone`, or `world`)");
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace, SyntaxKind::Eof]);
            if p.at(SyntaxKind::Newline) {
                p.bump();
            }
        }
        p.expect(SyntaxKind::RBrace);
    }

    if !saw_gateway {
        p.error_with_message_no_bump("mmo declaration is missing required `gateway <id>` clause");
    }
    if !saw_zone {
        p.error_with_message_no_bump("mmo declaration is missing required `zone <id>` clause");
    }
    if !saw_world {
        p.error_with_message_no_bump("mmo declaration is missing required `world <id>` clause");
    }
    m.complete(p, SyntaxKind::MmoDef);
    p.expect_stmt_boundary();
}

fn parse_mmo_gateway_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("gateway") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `gateway` clause");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected gateway id after `gateway`");
    }
    m.complete(p, SyntaxKind::MmoGatewayClause);
    p.expect_stmt_boundary();
}

fn parse_mmo_zone_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("zone") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `zone` clause");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected zone id after `zone`");
    }
    m.complete(p, SyntaxKind::MmoZoneClause);
    p.expect_stmt_boundary();
}

fn parse_mmo_world_clause(p: &mut Parser) {
    let m = p.start();
    if p.at_ident_text("world") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `world` clause");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected world id after `world`");
    }
    m.complete(p, SyntaxKind::MmoWorldClause);
    p.expect_stmt_boundary();
}

fn parse_asset_spec_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::AssetSpecKw,
        SyntaxKind::AssetSpecDef,
        SyntaxKind::AssetSpecIdClause,
        SyntaxKind::AssetSpecProfileClause,
        "asset_spec",
    );
}

fn parse_style_profile_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::StyleProfileKw,
        SyntaxKind::StyleProfileDef,
        SyntaxKind::StyleProfileIdClause,
        SyntaxKind::StyleProfileProfileClause,
        "style_profile",
    );
}

fn parse_generator_profile_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::GeneratorProfileKw,
        SyntaxKind::GeneratorProfileDef,
        SyntaxKind::GeneratorProfileIdClause,
        SyntaxKind::GeneratorProfileProfileClause,
        "generator_profile",
    );
}

fn parse_quality_profile_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::QualityProfileKw,
        SyntaxKind::QualityProfileDef,
        SyntaxKind::QualityProfileIdClause,
        SyntaxKind::QualityProfileProfileClause,
        "quality_profile",
    );
}

fn parse_provenance_policy_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::ProvenancePolicyKw,
        SyntaxKind::ProvenancePolicyDef,
        SyntaxKind::ProvenancePolicyIdClause,
        SyntaxKind::ProvenancePolicyProfileClause,
        "provenance_policy",
    );
}

fn parse_character_spec_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::CharacterSpecKw,
        SyntaxKind::CharacterSpecDef,
        SyntaxKind::CharacterSpecIdClause,
        SyntaxKind::CharacterSpecProfileClause,
        "character_spec",
    );
}

fn parse_rig_spec_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::RigSpecKw,
        SyntaxKind::RigSpecDef,
        SyntaxKind::RigSpecIdClause,
        SyntaxKind::RigSpecProfileClause,
        "rig_spec",
    );
}

fn parse_anim_set_spec_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::AnimSetSpecKw,
        SyntaxKind::AnimSetSpecDef,
        SyntaxKind::AnimSetSpecIdClause,
        SyntaxKind::AnimSetSpecProfileClause,
        "anim_set_spec",
    );
}

fn parse_audio_spec_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::AudioSpecKw,
        SyntaxKind::AudioSpecDef,
        SyntaxKind::AudioSpecIdClause,
        SyntaxKind::AudioSpecProfileClause,
        "audio_spec",
    );
}

fn parse_vfx_spec_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::VfxSpecKw,
        SyntaxKind::VfxSpecDef,
        SyntaxKind::VfxSpecIdClause,
        SyntaxKind::VfxSpecProfileClause,
        "vfx_spec",
    );
}

fn parse_ui_spec_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::UiSpecKw,
        SyntaxKind::UiSpecDef,
        SyntaxKind::UiSpecIdClause,
        SyntaxKind::UiSpecProfileClause,
        "ui_spec",
    );
}

fn parse_world_recipe_def(p: &mut Parser) {
    parse_profiled_spec_def(
        p,
        SyntaxKind::WorldRecipeKw,
        SyntaxKind::WorldRecipeDef,
        SyntaxKind::WorldRecipeIdClause,
        SyntaxKind::WorldRecipeProfileClause,
        "world_recipe",
    );
}

fn parse_profiled_spec_def(
    p: &mut Parser,
    declaration_kw: SyntaxKind,
    def_kind: SyntaxKind,
    id_clause_kind: SyntaxKind,
    profile_clause_kind: SyntaxKind,
    declaration_name: &str,
) {
    let m = p.start();
    p.expect_with_message(
        declaration_kw,
        &format!("expected '{declaration_name}' to start a {declaration_name} declaration"),
    );
    p.expect_with_message(
        SyntaxKind::Ident,
        &format!("expected {declaration_name} declaration name after '{declaration_name}'"),
    );
    expect_block_intro(
        p,
        &format!("expected '{{' after {declaration_name} declaration name"),
    );
    let mut saw_id = false;

    if p.at(SyntaxKind::LBrace) {
        p.bump();
        while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
            p.consume_trivia();
            if p.at(SyntaxKind::RBrace) || p.is_at_eof() {
                break;
            }
            if p.at_ident_text("id") {
                parse_profiled_spec_id_clause(p, id_clause_kind, declaration_name);
                saw_id = true;
                continue;
            }
            if p.at(SyntaxKind::ProfileKw) {
                parse_profiled_spec_profile_clause(p, profile_clause_kind, declaration_name);
                continue;
            }
            p.error_with_message_no_bump(&format!(
                "expected {declaration_name} clause (`id` or `profile`)"
            ));
            if p.at(SyntaxKind::Ident) {
                p.bump();
            }
            p.recover_until(&[
                SyntaxKind::Newline,
                SyntaxKind::ProfileKw,
                SyntaxKind::RBrace,
                SyntaxKind::Eof,
            ]);
            if p.at(SyntaxKind::Newline) {
                p.bump();
            }
        }
        p.expect(SyntaxKind::RBrace);
    }

    if !saw_id {
        p.error_with_message_no_bump(&format!(
            "{declaration_name} declaration is missing required `id <value>` clause"
        ));
    }
    m.complete(p, def_kind);
    p.expect_stmt_boundary();
}

fn parse_profiled_spec_id_clause(p: &mut Parser, clause_kind: SyntaxKind, declaration_name: &str) {
    let m = p.start();
    if p.at_ident_text("id") {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected `id` clause");
    }
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump(&format!(
            "expected id value after `id` in {declaration_name} declaration"
        ));
    }
    m.complete(p, clause_kind);
    p.expect_stmt_boundary();
}

fn parse_profiled_spec_profile_clause(
    p: &mut Parser,
    clause_kind: SyntaxKind,
    declaration_name: &str,
) {
    let m = p.start();
    p.expect(SyntaxKind::ProfileKw);
    if is_declaration_clause_value_start(p.peek()) {
        p.bump();
    } else {
        p.error_with_message_no_bump(&format!(
            "expected profile value after `profile` in {declaration_name} declaration"
        ));
    }
    m.complete(p, clause_kind);
    p.expect_stmt_boundary();
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
    parse_block(p);
    m.complete(p, SyntaxKind::PrivateBlock);
}

fn reject_removed_keyword_statement_head(p: &mut Parser) -> bool {
    if p.at(SyntaxKind::CheckKw) {
        p.error_with_message_no_bump(
            "removed keyword `check`; declare `fn ... -> Boolean` instead",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Eof]);
        if p.at(SyntaxKind::Newline) {
            p.bump();
        }
        return true;
    }
    if p.at(SyntaxKind::GivenKw) {
        p.error_with_message_no_bump(
            "legacy `given` call syntax is not supported; use standard call syntax",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Eof]);
        if p.at(SyntaxKind::Newline) {
            p.bump();
        }
        return true;
    }
    if p.at(SyntaxKind::ComponentKw) {
        p.error_with_message_no_bump(
            "removed keyword `component`; use `node <Name> profile ui|world|canvas { ... }`",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Eof]);
        if p.at(SyntaxKind::Newline) {
            p.bump();
        }
        return true;
    }
    // Note: SceneKw is no longer rejected — it's now dispatched to scene_decl() in parse_statement()
    if p.at(SyntaxKind::WidgetKw) {
        p.error_with_message_no_bump(
            "removed keyword `widget`; use `material <Name> { ... }` or `view`",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Eof]);
        if p.at(SyntaxKind::Newline) {
            p.bump();
        }
        return true;
    }
    if p.at(SyntaxKind::DefaultKw) {
        p.error_with_message_no_bump(
            "removed keyword `otherwise`; use `else`, `default`, or `??` depending on context",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Eof]);
        if p.at(SyntaxKind::Newline) {
            p.bump();
        }
        return true;
    }
    if p.at(SyntaxKind::DerivesKw) {
        p.error_with_message_no_bump(
            "removed keyword `derives`; use `fn` for methods and intrinsic structural semantics",
        );
        p.bump();
        p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Eof]);
        if p.at(SyntaxKind::Newline) {
            p.bump();
        }
        return true;
    }
    let removed = [
        ("optimize", "detach without optimize objective"),
        ("to", "fn"),
        ("A", "class"),
        ("An", "interface"),
        ("can", "fn"),
        ("checks", "fn"),
        ("check", "fn"),
        ("derives", "fn / structural semantics"),
        ("given", "standard call syntax"),
        ("component", "node"),
        ("scene", "node"),
        ("widget", "material/view"),
        ("but", "else"),
        ("otherwise", "else/default/??"),
        ("it", "self"),
        ("its", "self"),
    ];
    for (old, replacement) in removed {
        if p.at_ident_text(old) {
            p.error_with_message_no_bump(&format!("removed keyword `{old}`; use `{replacement}`"));
            p.bump();
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::Eof]);
            if p.at(SyntaxKind::Newline) {
                p.bump();
            }
            return true;
        }
    }
    false
}
