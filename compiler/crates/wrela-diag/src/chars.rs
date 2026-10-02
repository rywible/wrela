//! Character classes that diagnostics care about.

/// Whether `c` shows up as a visible glyph. Controls, whitespace (the ASCII space included) and
/// the common zero-width, formatting and bidirectional-control characters don't, so messages and
/// snippets name them by code point instead.
pub fn is_visible(c: char) -> bool {
    !(c.is_control()
        || c.is_whitespace()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{115F}'
                | '\u{1160}'
                | '\u{17B4}'
                | '\u{17B5}'
                | '\u{180B}'..='\u{180F}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{3164}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{FFA0}'
                | '\u{FFF0}'..='\u{FFFB}'
                | '\u{E0000}'..='\u{E0FFF}'
        ))
}

/// A file name as it's shown in a diagnostic or an error message: the ASCII space and visible
/// characters as they are, every other character as its code point (`<U+001B>`). A name comes
/// from the file system, so it can hold a control or bidirectional character that would rewrite
/// the terminal, or a line break that would forge a line of output.
pub fn display_name(name: &str) -> String {
    let mut shown = String::with_capacity(name.len());
    for c in name.chars() {
        if c == ' ' || is_visible(c) {
            shown.push(c);
        } else {
            shown.push_str(&format!("<U+{:04X}>", u32::from(c)));
        }
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::{display_name, is_visible};

    #[test]
    fn invisible_characters() {
        for c in ['a', 'π', '$', '“', '😀'] {
            assert!(is_visible(c), "{c:?}");
        }
        for c in [
            ' ', '\t', '\0', '\u{1B}', '\u{A0}', '\u{200B}', '\u{202E}', '\u{FEFF}',
        ] {
            assert!(!is_visible(c), "{c:?}");
        }
    }

    #[test]
    fn names_show_invisible_characters_by_code_point() {
        assert_eq!(display_name("src/my file.wrela"), "src/my file.wrela");
        assert_eq!(
            display_name("a\u{1B}[31m\u{202E}\tb\n.wrela"),
            "a<U+001B>[31m<U+202E><U+0009>b<U+000A>.wrela"
        );
    }
}
