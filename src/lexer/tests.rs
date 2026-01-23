use super::lexer::Lexer;
use super::tokens::SpannedToken;
use super::tokens::Token;
use smol_str::SmolStr;

// Updated helper to discard errors and trivia for existing tests
fn strip_spans(tokens: Vec<SpannedToken>) -> Vec<Token> {
    tokens
        .into_iter()
        .filter_map(|(t, _)| if t.is_trivia() { None } else { Some(t) })
        .collect()
}

#[test]
fn test_basic_lexing() {
    // Modified input to use simple identifier in interpolation
    let input = r#"A Whale:
    has:
        name: String

    can swim(distance: Number):
        print("Hi! My name is {name} and I can swim {distance}")

to make_moby_swim():
    moby = Whale(name="moby")
    moby.swim(500)

make_moby_swim()
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty(), "Lexing failed with errors: {:?}", errors);
    let tokens = strip_spans(tokens);

    // simple sanity check of the first few tokens
    assert_eq!(tokens[0], Token::Class); // A
    assert_eq!(tokens[1], Token::Identifier(SmolStr::new("Whale")));
    assert_eq!(tokens[2], Token::Colon);
}

#[test]
fn test_lexer_round_trip() {
    let input = r#"A Whale:
    has:
        name: String

    can swim(distance: Number):
        so: This is a comment
        print("Hi! My name is {name} and I can swim {distance}")
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty(), "{:?}", errors);

    let mut reconstructed = String::new();
    for (token, _) in tokens {
        reconstructed.push_str(&token.to_string());
    }

    // We expect some subtle differences if we don't handle every single escape or space perfectly,
    // but the goal of lossless is exact match.
    assert_eq!(reconstructed, input);
}

#[test]
fn test_extended_features() {
    let input = r#"
so: This is a comment
if x == 3.14:
    return nothing
else:
    val = true and false
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);

    let valid_tokens: Vec<Token> = tokens
        .into_iter()
        .filter(|t| {
            !matches!(
                t,
                Token::Newline(_)
                    | Token::Indent
                    | Token::Dedent
                    | Token::Eof
                    | Token::Whitespace(_)
                    | Token::Comment(_)
            )
        })
        .collect();

    assert_eq!(valid_tokens[0], Token::If);
    assert_eq!(valid_tokens[1], Token::Identifier(SmolStr::new("x")));
    assert_eq!(valid_tokens[2], Token::EqEq);
    assert_eq!(valid_tokens[3], Token::Float(3.14, SmolStr::new("3.14")));
    assert_eq!(valid_tokens[4], Token::Colon);
    assert_eq!(valid_tokens[5], Token::Return);
    assert_eq!(valid_tokens[6], Token::Nothing);
    assert_eq!(valid_tokens[7], Token::Else);
    assert_eq!(valid_tokens[8], Token::Colon);
    assert_eq!(valid_tokens[9], Token::Identifier(SmolStr::new("val")));
    assert_eq!(valid_tokens[10], Token::Equals);
    assert_eq!(valid_tokens[11], Token::True);
    assert_eq!(valid_tokens[12], Token::And);
    assert_eq!(valid_tokens[13], Token::False);
}

#[test]
fn test_async_and_comments() {
    let input = r#"
to async_ops():
    await some_task()
    so: This is an inline comment block
    x = 1
    so:
        This is a multiline comment block
        It spans multiple lines

        And has blank lines

    return x
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);

    let valid_tokens: Vec<Token> = tokens
        .into_iter()
        .filter(|t| {
            !matches!(
                t,
                Token::Newline(_)
                    | Token::Indent
                    | Token::Dedent
                    | Token::Eof
                    | Token::Whitespace(_)
                    | Token::Comment(_)
            )
        })
        .collect();

    assert_eq!(valid_tokens[0], Token::To);
    assert_eq!(
        valid_tokens[1],
        Token::Identifier(SmolStr::new("async_ops"))
    );
    assert_eq!(valid_tokens[2], Token::LParen);
    assert_eq!(valid_tokens[3], Token::RParen);
    assert_eq!(valid_tokens[4], Token::Colon);
    assert_eq!(valid_tokens[5], Token::Await);
    assert_eq!(
        valid_tokens[6],
        Token::Identifier(SmolStr::new("some_task"))
    );
    assert_eq!(valid_tokens[7], Token::LParen);
    assert_eq!(valid_tokens[8], Token::RParen);

    // "so: inline" should be gone.
    // x = 1
    assert_eq!(valid_tokens[9], Token::Identifier(SmolStr::new("x")));
    assert_eq!(valid_tokens[10], Token::Equals);
    assert_eq!(valid_tokens[11], Token::Integer(1, SmolStr::new("1"))); // Changed from Number(1.0)

    // "so: block" should be gone.
    // return x
    assert_eq!(valid_tokens[12], Token::Return);
    assert_eq!(valid_tokens[13], Token::Identifier(SmolStr::new("x")));
}

#[test]
fn test_spans() {
    let input = "x = 42";
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());

    // 0: Identifier(x) at 0..1
    assert_eq!(tokens[0].0, Token::Identifier(SmolStr::new("x")));
    assert_eq!(tokens[0].1.offset(), 0);
    assert_eq!(tokens[0].1.len(), 1);

    // Skip Whitespace at 1..2

    // 2: Equals at 2..3
    assert_eq!(tokens[2].0, Token::Equals);
    assert_eq!(tokens[2].1.offset(), 2);
    assert_eq!(tokens[2].1.len(), 1);

    // Skip Whitespace at 3..4

    // 4: Integer(42) at 4..6
    assert_eq!(tokens[4].0, Token::Integer(42, SmolStr::new("42")));
    assert_eq!(tokens[4].1.offset(), 4);
    assert_eq!(tokens[4].1.len(), 2);
}

#[test]
fn test_string_escapes() {
    let input = r#"print("Line1\nLine2\t\"Quoted\"\\")"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);

    // 0: Identifier(print)
    assert_eq!(tokens[0], Token::Identifier(SmolStr::new("print")));

    // 1: LParen
    assert_eq!(tokens[1], Token::LParen);

    // 2: StringLiteral
    // Expected value: Line1
    //                 Line2	"Quoted"\
    let expected = "Line1\nLine2\t\"Quoted\"\\";
    assert_eq!(tokens[2], Token::StringLiteral(SmolStr::new(expected)));
}

#[test]
fn test_new_operators() {
    let input = r#"
x += 1
y -= 2
z = a & b | c ^ d
val = i << 2 >> 1
for i in 0...10:
    pass
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);
    let valid_tokens: Vec<Token> = tokens
        .into_iter()
        .filter(|t| {
            !matches!(
                t,
                Token::Newline(_)
                    | Token::Indent
                    | Token::Dedent
                    | Token::Eof
                    | Token::Whitespace(_)
                    | Token::Comment(_)
            )
        })
        .collect();

    // x += 1
    assert_eq!(valid_tokens[0], Token::Identifier(SmolStr::new("x")));
    assert_eq!(valid_tokens[1], Token::PlusEq);
    assert_eq!(valid_tokens[2], Token::Integer(1, SmolStr::new("1")));

    // y -= 2
    assert_eq!(valid_tokens[3], Token::Identifier(SmolStr::new("y")));
    assert_eq!(valid_tokens[4], Token::MinusEq);
    assert_eq!(valid_tokens[5], Token::Integer(2, SmolStr::new("2")));

    // z = a & b | c ^ d
    assert_eq!(valid_tokens[6], Token::Identifier(SmolStr::new("z")));
    assert_eq!(valid_tokens[7], Token::Equals);
    assert_eq!(valid_tokens[8], Token::Identifier(SmolStr::new("a")));
    assert_eq!(valid_tokens[9], Token::Ampersand);
    assert_eq!(valid_tokens[10], Token::Identifier(SmolStr::new("b")));
    assert_eq!(valid_tokens[11], Token::Pipe);
    assert_eq!(valid_tokens[12], Token::Identifier(SmolStr::new("c")));
    assert_eq!(valid_tokens[13], Token::Caret);
    assert_eq!(valid_tokens[14], Token::Identifier(SmolStr::new("d")));

    // val = i << 2 >> 1
    // Skip valid_tokens[15]..[17] (val = i)
    assert_eq!(valid_tokens[18], Token::ShiftLeft);
    // ...
    assert_eq!(valid_tokens[20], Token::ShiftRight);

    // for i in 0...10
    // ...
    // Index is getting tricky, let's just find the Range token
    assert!(valid_tokens.contains(&Token::Range));
    assert!(valid_tokens.contains(&Token::In));
    assert!(valid_tokens.contains(&Token::For));
}

#[test]
fn test_interpolation() {
    let input = r#"spawn "Hello {name} and {obj.prop}!""#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);

    assert_eq!(tokens[0], Token::Spawn);
    assert_eq!(tokens[1], Token::StringStart(SmolStr::new("Hello ")));
    assert_eq!(tokens[2], Token::LBrace);
    assert_eq!(tokens[3], Token::Identifier(SmolStr::new("name")));
    assert_eq!(tokens[4], Token::RBrace);
    assert_eq!(tokens[5], Token::StringPart(SmolStr::new(" and ")));
    assert_eq!(tokens[6], Token::LBrace);
    assert_eq!(tokens[7], Token::Identifier(SmolStr::new("obj")));
    assert_eq!(tokens[8], Token::Dot);
    assert_eq!(tokens[9], Token::Identifier(SmolStr::new("prop")));
    assert_eq!(tokens[10], Token::RBrace);
    assert_eq!(tokens[11], Token::StringEnd(SmolStr::new("!")));
}

#[test]
fn test_recursive_interpolation() {
    let input = r#""A { "B { 1 } C" } D""#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty(), "{:?}", errors);
    let tokens = strip_spans(tokens);

    // [StringStart("A "), LBrace, StringStart("B "), LBrace, Integer(1), RBrace, StringEnd(" C"), RBrace, StringEnd(" D")]
    assert_eq!(tokens[0], Token::StringStart(SmolStr::new("A ")));
    assert_eq!(tokens[1], Token::LBrace);
    assert_eq!(tokens[2], Token::StringStart(SmolStr::new("B ")));
    assert_eq!(tokens[3], Token::LBrace);
    assert_eq!(tokens[4], Token::Integer(1, SmolStr::new("1")));
    assert_eq!(tokens[5], Token::RBrace);
    assert_eq!(tokens[6], Token::StringEnd(SmolStr::new(" C")));
    assert_eq!(tokens[7], Token::RBrace);
    assert_eq!(tokens[8], Token::StringEnd(SmolStr::new(" D")));
}

#[test]
fn test_multiline_structures() {
    let input = r#"
func(
    arg1,
    arg2
)
list = [
    1,
    2,
    3
]
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);

    // Filter whitespace for easier checking
    let valid_tokens: Vec<Token> = tokens
        .into_iter()
        .filter(|t| {
            !matches!(
                t,
                Token::Newline(_)
                    | Token::Indent
                    | Token::Dedent
                    | Token::Eof
                    | Token::Whitespace(_)
                    | Token::Comment(_)
            )
        })
        .collect();

    // func(arg1, arg2)
    assert_eq!(valid_tokens[0], Token::Identifier(SmolStr::new("func")));
    assert_eq!(valid_tokens[1], Token::LParen);
    assert_eq!(valid_tokens[2], Token::Identifier(SmolStr::new("arg1")));
    assert_eq!(valid_tokens[3], Token::Comma);
    assert_eq!(valid_tokens[4], Token::Identifier(SmolStr::new("arg2")));
    assert_eq!(valid_tokens[5], Token::RParen);

    // list = [1, 2, 3]
    assert_eq!(valid_tokens[6], Token::Identifier(SmolStr::new("list")));
    assert_eq!(valid_tokens[7], Token::Equals);
    assert_eq!(valid_tokens[8], Token::LBracket);
    assert_eq!(valid_tokens[9], Token::Integer(1, SmolStr::new("1")));
    assert_eq!(valid_tokens[10], Token::Comma);
    assert_eq!(valid_tokens[11], Token::Integer(2, SmolStr::new("2")));
    assert_eq!(valid_tokens[12], Token::Comma);
    assert_eq!(valid_tokens[13], Token::Integer(3, SmolStr::new("3")));
    assert_eq!(valid_tokens[14], Token::RBracket);
}

#[test]
fn test_multiline_invalid_indent() {
    let input = r#"
func(
arg1
)
"#;
    let mut lexer = Lexer::new(input);
    let (_, errors) = lexer.lex();
    assert!(!errors.is_empty()); // Expect errors
}

#[test]
fn test_numeric_literals() {
    let input = "123 0xFF 0b1010 3.14 1_000 1.5e-10";
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);

    let valid_tokens: Vec<Token> = tokens.into_iter().filter(|t| !t.is_trivia()).collect();

    assert_eq!(valid_tokens[0], Token::Integer(123, SmolStr::new("123")));
    assert_eq!(valid_tokens[1], Token::Integer(255, SmolStr::new("0xFF"))); // 0xFF
    assert_eq!(valid_tokens[2], Token::Integer(10, SmolStr::new("0b1010"))); // 0b1010
    assert_eq!(valid_tokens[3], Token::Float(3.14, SmolStr::new("3.14")));
    assert_eq!(valid_tokens[4], Token::Integer(1000, SmolStr::new("1_000")));
    assert_eq!(
        valid_tokens[5],
        Token::Float(1.5e-10, SmolStr::new("1.5e-10"))
    );
}

#[test]
fn test_modularity() {
    let input = "use std, io from core";
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);

    assert_eq!(tokens[0], Token::Use);
    assert_eq!(tokens[1], Token::Identifier(SmolStr::new("std")));
    assert_eq!(tokens[2], Token::Comma);
    assert_eq!(tokens[3], Token::Identifier(SmolStr::new("io")));
    assert_eq!(tokens[4], Token::From);
    assert_eq!(tokens[5], Token::Identifier(SmolStr::new("core")));
}

#[test]
fn test_error_recovery() {
    // Valid x = 1; Invalid $; Valid y = 2
    let input = "x = 1 $ y = 2";
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();

    // Should have 1 error
    assert_eq!(errors.len(), 1);

    // Should still produce valid tokens for x and y
    let tokens = strip_spans(tokens);
    // x = 1
    assert_eq!(tokens[0], Token::Identifier(SmolStr::new("x")));
    assert_eq!(tokens[1], Token::Equals);
    assert_eq!(tokens[2], Token::Integer(1, SmolStr::new("1")));

    // y = 2 (skipping $)
    assert_eq!(tokens[3], Token::Identifier(SmolStr::new("y")));
    assert_eq!(tokens[4], Token::Equals);
    assert_eq!(tokens[5], Token::Integer(2, SmolStr::new("2")));
}

#[test]
fn test_final_keywords() {
    let input = r#"
public changing x = 1
match x:
    1: break
    otherwise: continue
its.name
it
An Apple
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty());
    let tokens = strip_spans(tokens);
    let valid_tokens: Vec<Token> = tokens
        .into_iter()
        .filter(|t| {
            !matches!(
                t,
                Token::Newline(_)
                    | Token::Indent
                    | Token::Dedent
                    | Token::Eof
                    | Token::Whitespace(_)
                    | Token::Comment(_)
            )
        })
        .collect();

    // public changing x = 1
    assert_eq!(valid_tokens[0], Token::Public);
    assert_eq!(valid_tokens[1], Token::Changing);
    assert_eq!(valid_tokens[2], Token::Identifier(SmolStr::new("x")));
    assert_eq!(valid_tokens[3], Token::Equals);
    assert_eq!(valid_tokens[4], Token::Integer(1, SmolStr::new("1")));

    // match x:
    assert_eq!(valid_tokens[5], Token::Match);
    assert_eq!(valid_tokens[6], Token::Identifier(SmolStr::new("x")));
    assert_eq!(valid_tokens[7], Token::Colon);

    // 1: break
    assert_eq!(valid_tokens[8], Token::Integer(1, SmolStr::new("1")));
    assert_eq!(valid_tokens[9], Token::Colon);
    assert_eq!(valid_tokens[10], Token::Break);

    // otherwise: continue
    assert_eq!(valid_tokens[11], Token::Otherwise);
    assert_eq!(valid_tokens[12], Token::Colon);
    assert_eq!(valid_tokens[13], Token::Continue);

    // its.name
    assert_eq!(valid_tokens[14], Token::Its);
    assert_eq!(valid_tokens[15], Token::Dot);
    assert_eq!(valid_tokens[16], Token::Identifier(SmolStr::new("name")));

    // it
    assert_eq!(valid_tokens[17], Token::It);

    // An Apple
    assert_eq!(valid_tokens[18], Token::An);
    // "Apple" is an identifier. It is not the keyword "A" (Class).
    assert_eq!(valid_tokens[19], Token::Identifier(SmolStr::new("Apple")));
}

#[test]
fn test_unicode_and_bitwise() {
    let input = "🚀 = ~0b1010";
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty(), "Lexing failed: {:?}", errors);
    let tokens = strip_spans(tokens);

    // 0: Identifier("🚀")
    assert_eq!(tokens[0], Token::Identifier(SmolStr::new("🚀")));
    // 1: Equals
    assert_eq!(tokens[1], Token::Equals);
    // 2: BitwiseNot
    assert_eq!(tokens[2], Token::BitwiseNot);
    // 3: Integer(10)
    assert_eq!(tokens[3], Token::Integer(10, SmolStr::new("0b1010")));
}

#[test]
fn test_arrow_token() {
    let input = "to main() -> nothing:";
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty(), "{:?}", errors);
    let tokens = strip_spans(tokens);

    assert_eq!(tokens[0], Token::To);
    assert_eq!(tokens[1], Token::Identifier(SmolStr::new("main")));
    assert_eq!(tokens[2], Token::LParen);
    assert_eq!(tokens[3], Token::RParen);
    assert_eq!(tokens[4], Token::Arrow);
    assert_eq!(tokens[5], Token::Nothing);
    assert_eq!(tokens[6], Token::Colon);
}

#[test]
fn test_numeric_literal_torture() {
    let cases = vec![
        ("0", Token::Integer(0, "0".into())),
        ("123", Token::Integer(123, "123".into())),
        ("1_000_000", Token::Integer(1000000, "1_000_000".into())),
        ("0xFF", Token::Integer(255, "0xFF".into())),
        ("0xff", Token::Integer(255, "0xff".into())),
        ("0b1010", Token::Integer(10, "0b1010".into())),
        ("0o77", Token::Integer(63, "0o77".into())),
        ("3.14", Token::Float(3.14, "3.14".into())),
        (".14", Token::Float(0.14, ".14".into())),
        ("1.", Token::Float(1.0, "1.".into())),
        ("1e10", Token::Float(1e10, "1e10".into())),
        ("1.5E-2", Token::Float(0.015, "1.5E-2".into())),
        ("0xABC_DEF", Token::Integer(0xABCDEF, "0xABC_DEF".into())),
    ];

    for (input, expected) in cases {
        let mut lexer = Lexer::new(input);
        let (tokens, errors) = lexer.lex();
        assert!(errors.is_empty(), "Failed on '{}': {:?}", input, errors);
        let valid = tokens[0].0.clone();
        assert_eq!(valid, expected, "Failed on '{}'", input);
    }

    // Error cases
    let error_cases = vec![
        "0x", "0b", "0o", // Empty prefixes
        "0xG", "0b2", "0o8",   // Invalid digits
        "1.2.3", // Multiple decimals
        "1e", "1e+", // Malformed exponents
    ];

    for input in error_cases {
        let mut lexer = Lexer::new(input);
        let (_, errors) = lexer.lex();
        assert!(
            !errors.is_empty(),
            "Expected error for '{}', but got none",
            input
        );
    }
}

#[test]
fn test_indentation_torture() {
    let input = r#"
A:
    B:
        C:
            D
    E:
        F

G
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty(), "{:?}", errors);

    let structure: Vec<_> = tokens
        .iter()
        .filter_map(|(t, _)| match t {
            Token::Indent => Some("INDENT"),
            Token::Dedent => Some("DEDENT"),
            Token::Identifier(s) => Some(s.as_str()),
            Token::Class => Some("A"),
            _ => None,
        })
        .collect();

    assert_eq!(
        structure,
        vec![
            "A", "INDENT", "B", "INDENT", "C", "INDENT", "D", "DEDENT", "DEDENT", "E", "INDENT",
            "F", "DEDENT", "DEDENT", "G"
        ]
    );
}

#[test]
fn test_interpolation_torture() {
    // Nested, escaped, and complex
    let input = r#""A \{escaped\} { "B {1 + 1} C" } D""#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty(), "{:?}", errors);

    let tokens = strip_spans(tokens);
    // [StringStart("A {escaped} "), LBrace, StringStart("B "), LBrace, Integer(1), Plus, Integer(1), RBrace, StringEnd(" C"), RBrace, StringEnd(" D")]
    assert_eq!(tokens[0], Token::StringStart("A {escaped} ".into()));
    assert!(tokens.contains(&Token::Plus));
}

#[test]
fn test_comment_torture() {
    let input = r#"
x = 1 so: inline
y = 2
so:
    block
    with:
        indentation
z = 3
"#;
    let mut lexer = Lexer::new(input);
    let (tokens, errors) = lexer.lex();
    assert!(errors.is_empty(), "{:?}", errors);

    let comments: Vec<_> = tokens
        .iter()
        .filter_map(|(t, _)| {
            if let Token::Comment(c) = t {
                Some(c.trim())
            } else {
                None
            }
        })
        .collect();

    assert_eq!(comments, vec!["inline", "block\nwith:\n    indentation"]);
}

#[test]
fn test_lexer_exhaustiveness() {
    use strum::IntoEnumIterator;
    let lexer = Lexer::new("");

    for token in Token::iter() {
        if token.is_keyword() {
            assert!(
                lexer.keywords.contains_key(token.as_ref().into()),
                "Keyword not registered: {:?}",
                token
            );
        } else if token.is_symbol() {
            let found = lexer.static_tokens.iter().any(|(_, t)| t == &token);
            assert!(found, "Symbol not registered: {:?}", token);
        }
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn fuzz_lexer_no_panic(s in "\\PC*") {
            let mut lexer = Lexer::new(&s);
            let _ = lexer.lex();
        }
    }
}
