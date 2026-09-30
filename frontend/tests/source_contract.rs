use wrela_frontend::{
    parse,
    source::{ByteRange, EditorPosition, PieceKind, Source},
    token::TokenId,
};

/// Deliberately independent of Source::render and Source::validate.
fn audit_tape(source: &Source, original: &[u8]) -> Result<Vec<u8>, String> {
    let mut cursor = 0;
    let mut rendered = Vec::new();
    let unicode = std::str::from_utf8(original).ok();
    for piece in &source.pieces {
        if piece.range.start != cursor
            || piece.range.end < piece.range.start
            || piece.range.end > original.len()
        {
            return Err("source pieces do not form an ordered partition".into());
        }
        if piece.bytes.len() != piece.range.end - piece.range.start
            || piece.bytes != original[piece.range.start..piece.range.end]
        {
            return Err("piece bytes disagree with source positions".into());
        }
        if let Some(text) = unicode
            && (!text.is_char_boundary(piece.range.start)
                || !text.is_char_boundary(piece.range.end))
        {
            return Err("piece bisects a Unicode scalar".into());
        }
        rendered.extend_from_slice(&piece.bytes);
        cursor = piece.range.end;
    }
    if cursor != original.len() {
        return Err("source suffix was dropped".into());
    }
    Ok(rendered)
}

#[test]
fn independent_render_covers_empty_trivia_unicode_and_malformed_bytes() {
    let cases: &[&[u8]] = &[
        b"",
        b" \t\r\n// only trivia\r\n",
        b"/* outer /* inner */ end */\r\n",
        "fn café() -> Unit {\r\n // 😀\r\n return\r\n}\r\n".as_bytes(),
        b"fn broken() -> Unit {\n \xff\xfe;\n}\n",
        b"\xf0\x9f",
        b"a\0b",
    ];
    for bytes in cases {
        let parsed = parse(bytes);
        assert_eq!(audit_tape(&parsed.source, bytes).unwrap(), *bytes);
        assert_eq!(parsed.source.render(), *bytes);
        assert_eq!(parsed.syntax.range, ByteRange::new(0, bytes.len()));
        for diagnostic in &parsed.diagnostics {
            assert!(diagnostic.range.start <= diagnostic.range.end);
            assert!(diagnostic.range.end <= bytes.len());
            if let Ok(text) = std::str::from_utf8(bytes) {
                assert!(text.is_char_boundary(diagnostic.range.start));
                assert!(text.is_char_boundary(diagnostic.range.end));
            }
        }
    }
}

#[test]
fn tape_audit_rejects_ownership_and_position_corruption() {
    let bytes = "// café\nfn f() -> Unit { return }\n".as_bytes();
    let source = parse(bytes).source;
    assert!(audit_tape(&source, bytes).is_ok());
    let mut dropped = source.clone();
    dropped.pieces.pop();
    assert!(audit_tape(&dropped, bytes).is_err());
    let mut duplicated = source.clone();
    duplicated.pieces.insert(1, duplicated.pieces[0].clone());
    assert!(audit_tape(&duplicated, bytes).is_err());
    let mut reversed = source.clone();
    reversed.pieces.swap(0, 1);
    assert!(audit_tape(&reversed, bytes).is_err());
    let mut impossible = source.clone();
    impossible.pieces[0].range.end = bytes.len() + 1;
    assert!(audit_tape(&impossible, bytes).is_err());
    let mut altered = source.clone();
    altered.pieces[0].bytes[0] = b'!';
    assert!(audit_tape(&altered, bytes).is_err());
}

#[test]
fn token_edits_preserve_every_unrelated_byte_and_reparse() {
    let before = b"// leading\r\nfn run() -> Number {\r\n let value /* name */ = 1 + /* operator */ 2\r\n return value // tail\r\n}\r\n";
    for (old, new) in [
        (b"value".as_slice(), b"renamed".as_slice()),
        (b"1".as_slice(), b"100".as_slice()),
        (b"+".as_slice(), b"*".as_slice()),
    ] {
        let parsed = parse(before);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let (index, piece) = parsed
            .source
            .pieces
            .iter()
            .enumerate()
            .find(|(_, p)| matches!(p.kind, PieceKind::Token(_)) && p.bytes == old)
            .unwrap();
        let mut expected = before[..piece.range.start].to_vec();
        expected.extend_from_slice(new);
        expected.extend_from_slice(&before[piece.range.end..]);
        let edited = parsed.source.replace_token(TokenId(index), new).unwrap();
        assert_eq!(edited, expected);
        let reparsed = parse(&edited);
        assert!(
            reparsed.diagnostics.is_empty(),
            "{:?}",
            reparsed.diagnostics
        );
        assert!(reparsed.admit().is_ok());
        assert_eq!(audit_tape(&parse(&edited).source, &edited).unwrap(), edited);
    }
    let source = parse(before).source;
    let trivia = source
        .pieces
        .iter()
        .position(|p| matches!(p.kind, PieceKind::Trivia(_)))
        .unwrap();
    assert!(source.replace_token(TokenId(trivia), b"bad").is_err());
    assert!(
        source
            .replace_token(TokenId(source.pieces.len()), b"bad")
            .is_err()
    );
}

#[test]
fn editor_coordinates_count_utf16_and_physical_line_endings() {
    let text = "a😀é\r\nδ\rZ\n";
    let source = parse(text.as_bytes()).source;
    for (offset, line, column) in [
        (0, 0, 0),
        (1, 0, 1),
        (5, 0, 3),
        (7, 0, 4),
        (8, 0, 4),
        (9, 1, 0),
        (11, 1, 1),
        (12, 2, 0),
        (13, 2, 1),
        (14, 3, 0),
    ] {
        assert_eq!(
            source.editor_position(offset),
            Some(EditorPosition { line, column }),
            "offset {offset}"
        );
    }
    for offset in [2, 3, 4, 6, 10, 15] {
        assert_eq!(source.editor_position(offset), None, "offset {offset}");
    }
    let invalid = parse(b"valid\n\xff");
    assert!(
        invalid
            .source
            .pieces
            .iter()
            .any(|p| p.kind == PieceKind::Invalid)
    );
    assert!(invalid.source.editor_position(0).is_none());
    assert!(
        invalid
            .source
            .editor_position(invalid.source.len())
            .is_none()
    );
    assert!(invalid.admit().is_err());
}

#[test]
fn external_source_decoding_rejects_unknown_kinds_and_non_integer_ranges() {
    let source = parse(b"fn f() -> Unit { return }").source;
    let value = serde_json::to_value(&source).unwrap();
    let mut invalid_kind = value.clone();
    invalid_kind["pieces"][0]["kind"] = serde_json::json!({"Token":"ImaginaryToken"});
    assert!(serde_json::from_value::<Source>(invalid_kind).is_err());
    let mut invalid_trivia = value.clone();
    invalid_trivia["pieces"][0]["kind"] = serde_json::json!({"Trivia":"ImaginaryTrivia"});
    assert!(serde_json::from_value::<Source>(invalid_trivia).is_err());
    for offset in [
        serde_json::json!(true),
        serde_json::json!(-1),
        serde_json::json!(1.5),
    ] {
        let mut invalid = value.clone();
        invalid["pieces"][0]["range"]["start"] = offset;
        assert!(serde_json::from_value::<Source>(invalid).is_err());
    }
}

#[test]
fn significant_syntax_excludes_exterior_trivia_while_full_range_owns_it() {
    let prefix = "// leading café\r\n  ";
    let function = "fn f() -> Unit { return }";
    let suffix = " /* trailing 😀 */\r\n\t";
    let text = format!("{prefix}{function}{suffix}");
    let parsed = parse(text.as_bytes());
    assert!(parsed.is_syntax_eligible());
    assert_eq!(parsed.syntax.range, ByteRange::new(0, text.len()));
    assert_eq!(
        parsed.syntax.syntax_range,
        ByteRange::new(prefix.len(), prefix.len() + function.len())
    );
    assert_eq!(
        parsed.syntax.declarations[0].range,
        parsed.syntax.syntax_range
    );
    assert_eq!(parsed.cst().range, parsed.syntax.syntax_range);
    for text in ["", " /* only comment */\r\n  "] {
        let parsed = parse(text.as_bytes());
        assert_eq!(parsed.syntax.range, ByteRange::new(0, text.len()));
        assert_eq!(parsed.syntax.syntax_range, ByteRange::empty(text.len()));
    }
}
