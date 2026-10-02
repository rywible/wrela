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

/// Every token the lexer accepts, in code shaped like the language; no diagnostics and no unknown
/// tokens. (Lexing only: this isn't a valid program, so it isn't a conformance test.)
#[test]
fn every_kind_of_token_lexes_cleanly() {
    let text = "\
/// A doc comment.
@compute(64)
pub fn mix_all(a: f32, b: mut vec3, c: take Grid<u32, 4>) -> borrow f32 {
    let n = 1_000 + 0xFF + 0b1010 + 0o17
    var x = 1.5e-3 * 2.0e+9 / 3.25 % 7.0 ** 2.0
    x += 1; x -= 1; x *= 2; x /= 2; x %= 3; x **= 2; x ^= 1; x &= 1; x |= 1
    let bits = (n & 3) | (n ^ 5) << 2 >> 1; bits <<= 1; bits >>= 1
    let ok = !(a == b.x) && a != 0.0 || a <= 1.0 && a >= -1.0 && a < 2.0 && a > -2.0
    let t = (1, 2).0
    for i in 0..10 { }
    for i in 0..=10 { }
    let f = |p: f32| p * 2.0
    match t { 0 => a, _ => b.x }
    std::math::sqrt::<f32>(x)
}
";
    let (lexed, _) = lexed(text);
    assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
    assert!(lexed.tokens.iter().all(|t| t.kind != TokenKind::Unknown));
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
fn a_lone_carriage_return_is_e0001_and_reads_as_a_line_break() {
    let (lexed, _) = lexed("let a = 1\rlet b = 2\r");
    assert_eq!(codes("let a = 1\rlet b = 2\r"), ["E0001", "E0001"]);
    let d = &lexed.diagnostics[0];
    assert_eq!(d.message, "unexpected character U+000D");
    assert_eq!((d.primary.span.start(), d.primary.span.end()), (9, 10));
    assert_eq!(d.help[0].edits[0].replacement, "\n");
    let newlines = lexed.tokens.iter().filter(|t| t.kind == TokenKind::Newline);
    assert_eq!(newlines.count(), 2);
    // It ends a comment too, so what follows can't hide in the comment.
    assert_eq!(tokens("// a\rb"), ["LineComment:// a", "NL", "Ident:b"]);
    assert_eq!(codes("// a\rb"), ["E0001"]);
}

#[test]
fn a_leading_byte_order_mark_is_skipped() {
    let (lexed, _) = lexed("\u{FEFF}fn f() {}\n");
    assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
    assert_eq!(lexed.tokens[0].span.start(), 3);
    // Anywhere else it's an invisible character.
    assert_eq!(
        lexed_one("x\u{FEFF}").message,
        "unexpected character U+FEFF"
    );
}

#[test]
fn a_name_with_non_ascii_letters_is_one_identifier_and_one_error() {
    assert_eq!(
        tokens("fn café() {}"),
        [
            "Ident:fn",
            "Ident:café",
            "Punct:(",
            "Punct:)",
            "Punct:{",
            "Punct:}"
        ]
    );
    let d = lexed_one("fn café() {}");
    assert_eq!(d.message, "unexpected character `é`");
    assert_eq!((d.primary.span.start(), d.primary.span.end()), (6, 8));
    assert_eq!(d.notes, ["identifiers are ASCII: letters, digits and `_`"]);

    assert_eq!(
        tokens("let naïve_café = 1"),
        ["Ident:let", "Ident:naïve_café", "Punct:=", "Number:1"]
    );
    assert_eq!(
        lexed_one("let naïve_café = 1").message,
        "unexpected characters `ï` `é`"
    );
    assert_eq!(tokens("15µm"), ["Number:15µm"]);
    assert_eq!(lexed_one("15µm").message, "unexpected character `µ`");

    // An invisible letter (a Hangul filler) gets a fix that deletes it.
    let d = lexed_one("x\u{3164}");
    assert_eq!(d.message, "unexpected character U+3164");
    assert_eq!(d.help[0].edits[0].replacement, "");
    // Next to punctuation, a stray character is its own error, and the name its own.
    assert_eq!(codes("$π"), ["E0001", "E0001"]);
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
    assert_eq!((d.primary.span.start(), d.primary.span.end()), (0, 7));
    assert_eq!(d.help[0].edits[0].replacement, "// a");
    assert_eq!(
        lexed.tokens.len(),
        2,
        "the comment is skipped: {:?}",
        lexed.tokens
    );

    let doc = &lexed_one("/** docs */");
    assert_eq!(doc.help[0].edits[0].replacement, "/// docs");
    assert_eq!(
        lexed_one("\n  /** docs */").help[0].edits[0].replacement,
        "/// docs"
    );
    // After code, a doc comment would document nothing: a plain comment instead.
    let trailing_doc = lexed_one("let x = 1 /** doc */");
    assert_eq!(trailing_doc.help[0].edits[0].replacement, "// doc");
    assert_eq!(trailing_doc.help[0].message, "write it as a `//` comment");
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
    assert_eq!((d.primary.span.start(), d.primary.span.end()), (2, 4));
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
                t.span.start() >= end && t.span.end() > t.span.start(),
                "{text:?}: {lexed:?}"
            );
            end = t.span.end();
        }
    }
}
