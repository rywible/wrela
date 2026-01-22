use super::cursor::{Cursor, EOF_CHAR};

/// Consumes a "so:" comment block and returns its content.
pub fn consume_so_comment(cursor: &mut Cursor, base_indent: usize) -> String {
    let mut content = String::new();

    // 1. Skip the rest of the current line (anything after "so:")
    while cursor.first() != '\n' && cursor.first() != '\r' && !cursor.is_eof() {
        if let Some(c) = cursor.bump() {
            content.push(c);
        }
    }

    // 2. Process subsequent lines
    let mut block_indent = None;

    loop {
        // Peek at the start of the next line
        if cursor.first() == '\n' || cursor.first() == '\r' {
            if cursor.first() == '\r' && cursor.second() == '\n' {
                cursor.bump();
            }
            cursor.bump();
            content.push('\n');
        } else if cursor.is_eof() {
            break;
        }

        // Count leading spaces of the new line
        let mut temp_cursor = Cursor::new(cursor.as_str());
        let mut spaces = 0;
        while temp_cursor.first() == ' ' {
            temp_cursor.bump();
            spaces += 1;
        }

        let next = temp_cursor.first();

        // Blank lines or lines with only spaces are considered part of the comment
        if next == '\n' || next == '\r' || next == EOF_CHAR {
            cursor.eat_while(|c| c == ' ');
            continue;
        }

        // If the line is indented further than the base, it's part of the comment
        if spaces > base_indent {
            // Determine block indent from the first indented line
            if block_indent.is_none() {
                block_indent = Some(spaces);
            }

            let to_strip = block_indent.unwrap();
            for _ in 0..to_strip {
                if cursor.first() == ' ' {
                    cursor.bump();
                }
            }

            let line = cursor.take_while(|c| c != '\n' && c != '\r' && c != EOF_CHAR);
            content.push_str(line);
        } else {
            break;
        }
    }
    content
}
