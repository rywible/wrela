use wrela_diag::{FileId, SourceMap};

use crate::{Lexed, TokenKind, lex};

fn lexed(text: &str) -> (Lexed, String) {
    let mut map = SourceMap::new();
    let file: FileId = map.add("t.wrela", text).unwrap();
    (lex(file, text), text.to_string())
}

/// The tokens as `kind:text` pairs, newlines as `NL`.
fn tokens(text: &str) -> Vec<String> {
    let (lexed, text) = lexed(text);
    lexed
        .tokens
        .iter()
        .map(|t| match t.kind {
            TokenKind::Newline => "NL".to_string(),
            kind => format!("{kind:?}:{}", &text[t.span.range()]),
        })
        .collect()
}

fn codes(text: &str) -> Vec<&'static str> {
    lexed(text)
        .0
        .diagnostics
        .iter()
        .map(|d| d.code.id())
        .collect()
}

#[test]
fn identifiers_numbers_and_punctuation() {
    assert_eq!(
        tokens("let x_1 = a.b::<T>(2.5e-3) **= 0xFF\n"),
        [
            "Ident:let",
            "Ident:x_1",
            "Punct:=",
            "Ident:a",
            "Punct:.",
            "Ident:b",
            "Punct:::",
            "Punct:<",
            "Ident:T",
            "Punct:>",
            "Punct:(",
            "Number:2.5e-3",
            "Punct:)",
            "Punct:**=",
            "Number:0xFF",
            "NL",
        ]
    );
    assert!(codes("fn f(x: mut T) -> u32 { x += 1; x != 2 && y >= 3 || !z }").is_empty());
}

#[test]
fn numbers_stop_at_ranges_and_tuple_indices() {
    assert_eq!(tokens("1..5"), ["Number:1", "Punct:..", "Number:5"]);
    assert_eq!(
        tokens("t.0.1"),
        ["Ident:t", "Punct:.", "Number:0", "Punct:.", "Number:1"]
    );
    assert_eq!(tokens("1.0.max"), ["Number:1.0", "Punct:.", "Ident:max"]);
    assert_eq!(
        tokens("15cm 1_000 0x1e-5"),
        [
            "Number:15cm",
            "Number:1_000",
            "Number:0x1e",
            "Punct:-",
            "Number:5"
        ]
    );
}

#[test]
fn comments_and_doc_comments() {
    assert_eq!(
        tokens("x // a $ π\n/// doc\n//// not doc"),
        [
            "Ident:x",
            "LineComment:// a $ π",
            "NL",
            "DocComment:/// doc",
            "NL",
            "LineComment://// not doc"
        ]
    );
}

#[test]
fn crlf_line_endings_stay_out_of_comments() {
    assert_eq!(tokens("// a\r\nx"), ["LineComment:// a", "NL", "Ident:x"]);
}

#[test]
fn unexpected_characters_are_e0001_one_per_run() {
    let (lexed, _) = lexed("a $$ b π\n");
    let messages: Vec<&str> = lexed
        .diagnostics
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(
        messages,
        ["unexpected characters `$$`", "unexpected character `π`"]
    );
    assert_eq!(
        lexed.diagnostics[1].notes,
        ["identifiers are ASCII: letters, digits and `_`"]
    );
    assert_eq!(tokens("a $$ b"), ["Ident:a", "Unknown:$$", "Ident:b"]);
}

#[test]
fn invisible_characters_are_named_by_code_point_with_a_fix() {
    let (lexed, _) = lexed("let\u{00A0}x = 1\u{200B}");
    let d = &lexed.diagnostics;
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].message, "unexpected character U+00A0");
    assert_eq!(d[0].help[0].message, "replace it with a space");
    assert_eq!(d[0].help[0].edits[0].replacement, " ");
    assert_eq!(d[1].message, "unexpected character U+200B");
    assert_eq!(d[1].help[0].edits[0].replacement, "");
    let tick = lexed_one("`");
    assert_eq!(tick.message, "unexpected character U+0060");
    assert!(
        tick.help.is_empty() && tick.primary.message.is_none(),
        "a backtick is visible"
    );
}

#[test]
fn block_comments_are_e0002_with_a_rewrite_when_mechanical() {
    let (lexed, _) = lexed("/* a */\nx");
    assert_eq!(lexed.diagnostics.len(), 1);
    let d = &lexed.diagnostics[0];
    assert_eq!(d.code.id(), "E0002");
    assert_eq!((d.primary.span.start, d.primary.span.end), (0, 7));
    assert_eq!(d.help[0].edits[0].replacement, "// a");
    assert_eq!(
        lexed.tokens.len(),
        2,
        "the comment is skipped: {:?}",
        lexed.tokens
    );

    let doc = &lexed_one("/** docs */");
    assert_eq!(doc.help[0].edits[0].replacement, "/// docs");
    // A line comment after it merges into the rewrite.
    let trailing = lexed_one("x /* a */ // b");
    assert_eq!(trailing.help[0].edits[0].replacement, "// a");
    // Code after the comment on its line: no mechanical rewrite.
    assert!(lexed_one("/* a */ x").help[0].edits.is_empty());
    // Multi-line: advice only.
    assert!(lexed_one("/* a\n b */").help[0].edits.is_empty());
}

fn lexed_one(text: &str) -> wrela_diag::Diagnostic {
    let (lexed, _) = lexed(text);
    assert_eq!(lexed.diagnostics.len(), 1, "{:?}", lexed.diagnostics);
    lexed.diagnostics[0].clone()
}

#[test]
fn block_comments_nest_and_unclosed_ones_point_at_the_opener() {
    assert_eq!(tokens("/* a /* b */ c */ x"), ["Ident:x"]);
    assert_eq!(codes("/* a /* b */ c */ x"), ["E0002"]);
    let d = lexed_one("x /* never closed\n y $");
    assert_eq!((d.primary.span.start, d.primary.span.end), (2, 4));
    assert_eq!(
        d.primary.message.as_deref(),
        Some("this comment is never closed")
    );
    assert_eq!(codes("/*/"), ["E0002"]);
}

#[test]
fn every_input_makes_progress_and_spans_tile_the_tokens() {
    for text in [
        "", "\n", "/", "*", "/*", "\u{FEFF}", "0", "0x", "1e", "1e-", "a\r\nb", "é",
    ] {
        let (lexed, _) = lexed(text);
        let mut end = 0;
        for t in &lexed.tokens {
            assert!(
                t.span.start >= end && t.span.end > t.span.start,
                "{text:?}: {lexed:?}"
            );
            end = t.span.end;
        }
    }
}
