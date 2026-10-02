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

#[cfg(test)]
mod tests {
    use super::is_visible;

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
}
