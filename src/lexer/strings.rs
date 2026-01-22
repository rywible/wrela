use super::cursor::Cursor;
use super::errors::LexError;
use miette::SourceSpan;
use smol_str::SmolStr;

pub enum StringSegment {
    /// Found a closing quote. Contains the text accumulated so far.
    End(SmolStr),
    /// Found an interpolation start `{`. Contains the text accumulated so far.
    Interpolate(SmolStr),
    /// Reached EOF before closing quote or interpolation.
    Unterminated,
}

/// Lexes a string segment. A segment ends at either a `"` (end of string) or a `{` (start of interpolation).
pub fn lex_string_segment(
    cursor: &mut Cursor,
    base_offset: usize,
) -> (StringSegment, Vec<LexError>) {
    let mut content = String::new();
    let mut errors = Vec::new();

    while let Some(c) = cursor.bump() {
        let current_pos = base_offset + content.len(); // Approximate for error reporting

        match c {
            '"' => return (StringSegment::End(SmolStr::from(content)), errors),
            '{' => return (StringSegment::Interpolate(SmolStr::from(content)), errors),
            '\\' => {
                if let Some(next) = cursor.bump() {
                    match next {
                        'n' => content.push('\n'),
                        'r' => content.push('\r'),
                        't' => content.push('\t'),
                        '"' => content.push('"'),
                        '\\' => content.push('\\'),
                        '{' => content.push('{'),
                        '}' => content.push('}'),
                        _ => {
                            // The span should be from the backslash to the invalid char
                            let span = SourceSpan::new(current_pos.into(), 2usize.into());
                            errors.push(LexError::InvalidEscapeSequence { span, char: next });
                            content.push(next);
                        }
                    }
                } else {
                    return (StringSegment::Unterminated, errors);
                }
            }
            _ => content.push(c),
        }
    }

    (StringSegment::Unterminated, errors)
}
