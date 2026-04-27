use super::{expect_block_intro, expr, parse_block, parse_param_list, types};
use crate::parser::Parser;
use crate::parser::ast::is_name_like_label_token;
use crate::parser::kind::SyntaxKind;

pub fn at_audio_rt_audio_field(p: &Parser) -> bool {
    at_audio_rt_runtime_field(p, "audio")
}

pub fn at_audio_rt_media_field(p: &Parser) -> bool {
    at_audio_rt_runtime_field(p, "media")
}

fn at_audio_rt_runtime_field(p: &Parser, family: &str) -> bool {
    p.at(SyntaxKind::At)
        && p.peek_nth_non_trivia(1) == SyntaxKind::Ident
        && p.peek_nth_non_trivia_text(1) == "audio_rt"
        && p.peek_nth_non_trivia(2) == SyntaxKind::Ident
        && p.peek_nth_non_trivia_text(2) == family
        && p.peek_nth_non_trivia(3) == SyntaxKind::Ident
        && p.peek_nth_non_trivia_text(3) == "field"
}

pub fn input_map_def(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(
        p,
        "input_map",
        "expected 'input_map' to start an input map declaration",
    );
    p.expect_with_message(
        SyntaxKind::Ident,
        "expected input map name after 'input_map'",
    );
    expect_block_intro(p, "expected '{' after input_map declaration");
    parse_input_map_body(p);
    m.complete(p, SyntaxKind::InputMapDef);
}

pub fn body_def(p: &mut Parser) {
    parse_named_runtime_block(
        p,
        "body",
        SyntaxKind::BodyDef,
        "expected body name after 'body'",
        "expected '{' after body declaration",
        true,
    );
}

pub fn move_def(p: &mut Parser) {
    parse_named_runtime_block(
        p,
        "move",
        SyntaxKind::MoveDef,
        "expected move name after 'move'",
        "expected '{' after move declaration",
        true,
    );
}

pub fn moveset_def(p: &mut Parser) {
    parse_named_runtime_block(
        p,
        "moveset",
        SyntaxKind::MovesetDef,
        "expected moveset name after 'moveset'",
        "expected '{' after moveset declaration",
        false,
    );
}

pub fn audio_field_decl(p: &mut Parser) {
    let m = p.start();
    parse_optional_audio_rt_marker(p);
    expect_ident_text(
        p,
        "audio",
        "expected 'audio' to start an audio field declaration",
    );
    expect_ident_text(p, "field", "expected 'field' after 'audio'");
    parse_runtime_field_signature_and_block(
        p,
        "expected audio field name after 'audio field'",
        "expected '(' after audio field name",
        "expected ')' after audio field parameters",
        "expected '{' after audio field signature",
    );
    m.complete(p, SyntaxKind::AudioFieldDecl);
}

pub fn voice_decl(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "voice", "expected 'voice' to start a voice declaration");
    p.expect_with_message(SyntaxKind::Ident, "expected voice name after 'voice'");
    p.expect_with_message(SyntaxKind::LParen, "expected '(' after voice name");
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, "expected ')' after voice parameters");
    expect_block_intro(p, "expected '{' after voice signature");
    parse_runtime_clause_body(p, "expected runtime clause inside voice declaration");
    m.complete(p, SyntaxKind::VoiceDecl);
}

pub fn media_field_decl(p: &mut Parser) {
    let m = p.start();
    parse_optional_audio_rt_marker(p);
    expect_ident_text(
        p,
        "media",
        "expected 'media' to start a media field declaration",
    );
    expect_ident_text(p, "field", "expected 'field' after 'media'");
    parse_runtime_field_signature_and_block(
        p,
        "expected media field name after 'media field'",
        "expected '(' after media field name",
        "expected ')' after media field parameters",
        "expected '{' after media field signature",
    );
    m.complete(p, SyntaxKind::MediaFieldDecl);
}

fn parse_named_runtime_block(
    p: &mut Parser,
    keyword: &str,
    node_kind: SyntaxKind,
    name_error: &str,
    block_error: &str,
    allow_params: bool,
) {
    let m = p.start();
    expect_ident_text(
        p,
        keyword,
        &format!("expected '{keyword}' to start declaration"),
    );
    p.expect_with_message(SyntaxKind::Ident, name_error);
    if allow_params && p.at(SyntaxKind::LParen) {
        p.bump();
        parse_param_list(p);
        p.expect_with_message(SyntaxKind::RParen, "expected ')' after runtime parameters");
    }
    expect_block_intro(p, block_error);
    parse_runtime_clause_body(p, "expected runtime clause inside declaration");
    m.complete(p, node_kind);
}

fn parse_optional_audio_rt_marker(p: &mut Parser) {
    if !(at_audio_rt_audio_field(p) || at_audio_rt_media_field(p)) {
        return;
    }
    let m = p.start();
    p.expect(SyntaxKind::At);
    p.expect_with_message(SyntaxKind::Ident, "expected 'audio_rt' after '@'");
    m.complete(p, SyntaxKind::Attribute);
    if p.at_stmt_boundary() {
        p.expect_stmt_boundary();
    }
}

fn parse_runtime_field_signature_and_block(
    p: &mut Parser,
    name_error: &str,
    lparen_error: &str,
    rparen_error: &str,
    block_error: &str,
) {
    p.expect_with_message(SyntaxKind::Ident, name_error);
    p.expect_with_message(SyntaxKind::LParen, lparen_error);
    parse_param_list(p);
    p.expect_with_message(SyntaxKind::RParen, rparen_error);
    p.expect_with_message(SyntaxKind::Arrow, "expected '->' and a return type");
    types::parse_type(p);

    expect_block_intro(p, block_error);
    parse_block(p);
}

fn parse_input_map_body(p: &mut Parser) {
    let m = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' to start input_map body");
    }

    p.consume_trivia();
    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        let cursor = p.cursor_pos();
        if p.at_ident_text("action") {
            parse_input_action(p);
        } else {
            p.error_with_message_no_bump("expected input_map action declaration");
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
            p.expect_stmt_boundary();
        }
        if p.cursor_pos() == cursor {
            p.error();
        }
        p.consume_trivia();
    }

    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::Block);
}

fn parse_input_action(p: &mut Parser) {
    let m = p.start();
    expect_ident_text(p, "action", "expected 'action' to start input_map action");
    p.expect_with_message(SyntaxKind::Ident, "expected action name after 'action'");
    p.expect_with_message(SyntaxKind::Equals, "expected '=' after action name");
    parse_input_binding(p);
    while p.at(SyntaxKind::Pipe) {
        p.bump();
        parse_input_binding(p);
    }
    m.complete(p, SyntaxKind::InputMapAction);
    p.expect_stmt_boundary();
}

fn parse_input_binding(p: &mut Parser) {
    let m = p.start();
    parse_dotted_ident_path(p, "expected input binding source");
    if matches!(
        p.peek(),
        SyntaxKind::Less
            | SyntaxKind::LessEq
            | SyntaxKind::Greater
            | SyntaxKind::GreaterEq
            | SyntaxKind::EqEq
            | SyntaxKind::BangEq
    ) {
        p.bump();
        parse_input_binding_value(p);
    }
    m.complete(p, SyntaxKind::InputBinding);
}

fn parse_dotted_ident_path(p: &mut Parser, message: &str) {
    p.expect_with_message(SyntaxKind::Ident, message);
    while p.at(SyntaxKind::Dot) {
        p.bump();
        p.expect_with_message(SyntaxKind::Ident, "expected identifier after '.'");
    }
}

fn parse_input_binding_value(p: &mut Parser) {
    if p.at(SyntaxKind::Minus) {
        p.bump();
    }
    if matches!(
        p.peek(),
        SyntaxKind::IntNumber
            | SyntaxKind::FloatNumber
            | SyntaxKind::StringLiteral
            | SyntaxKind::TrueKw
            | SyntaxKind::FalseKw
            | SyntaxKind::Ident
    ) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected input binding comparison value");
    }
}

fn parse_runtime_clause_body(p: &mut Parser, item_error: &str) {
    let m = p.start();
    if p.at(SyntaxKind::LBrace) {
        p.bump();
    } else {
        p.error_with_message_no_bump("expected '{' to start runtime declaration body");
    }

    p.consume_trivia();
    while !p.at(SyntaxKind::RBrace) && !p.is_at_eof() {
        let cursor = p.cursor_pos();
        if is_name_like_label_token(p.peek()) {
            parse_runtime_clause(p);
        } else {
            p.error_with_message_no_bump(item_error);
            p.recover_until(&[SyntaxKind::Newline, SyntaxKind::RBrace]);
            p.expect_stmt_boundary();
        }
        if p.cursor_pos() == cursor {
            p.error();
        }
        p.consume_trivia();
    }

    p.expect(SyntaxKind::RBrace);
    m.complete(p, SyntaxKind::Block);
}

fn parse_runtime_clause(p: &mut Parser) {
    let m = p.start();
    parse_runtime_clause_name(p);
    if p.at(SyntaxKind::Equals) || p.at(SyntaxKind::Colon) {
        p.bump();
        expr::expr(p);
    } else if p.at(SyntaxKind::LBrace) {
        parse_runtime_clause_body(p, "expected nested runtime clause");
    } else {
        let consumed_tail = parse_runtime_clause_tail(p);
        if p.at(SyntaxKind::LBrace) {
            parse_runtime_clause_body(p, "expected nested runtime clause");
        } else if !consumed_tail {
            p.error_with_message_no_bump("expected ':', '=', or '{' after runtime clause name");
        }
    }
    m.complete(p, SyntaxKind::RuntimeClause);
    p.expect_stmt_boundary();
}

fn parse_runtime_clause_name(p: &mut Parser) {
    if is_name_like_label_token(p.peek()) {
        p.bump();
    } else {
        p.expect_with_message(SyntaxKind::Ident, "expected runtime clause name");
    }
}

fn parse_runtime_clause_tail(p: &mut Parser) -> bool {
    let mut consumed = false;
    while !p.at(SyntaxKind::LBrace) && !p.at_stmt_boundary() && !p.is_at_eof() {
        if p.at(SyntaxKind::StringStart) {
            expr::expr(p);
            consumed = true;
            continue;
        }
        p.bump();
        consumed = true;
    }
    consumed
}

fn expect_ident_text(p: &mut Parser, expected: &str, message: &str) {
    if p.at_ident_text(expected) {
        p.bump();
        return;
    }
    if p.at(SyntaxKind::Ident) {
        p.error_with_message_no_bump(message);
        p.bump();
        return;
    }
    p.expect_with_message(SyntaxKind::Ident, message);
}
