use crate::lexer::cursor::Cursor;
use crate::lexer::tokens::Token;
use smol_str::SmolStr;

pub fn consume_number(cursor: &mut Cursor) -> Token {
    let start_ptr = cursor.as_str();

    // Check for Hex/Bin prefixes (0x, 0b, 0o)
    if cursor.first() == '0' {
        match cursor.second() {
            'x' | 'X' => {
                cursor.bump(); // 0
                cursor.bump(); // x
                let num_start = cursor.as_str();
                cursor.eat_while(|c| c.is_ascii_hexdigit() || c == '_');
                let len = num_start.len() - cursor.as_str().len();
                let text = &num_start[..len];

                let raw_len = start_ptr.len() - cursor.as_str().len();
                let raw_text = SmolStr::new(&start_ptr[..raw_len]);

                let cleaned = text.replace('_', "");
                if !cleaned.is_empty()
                    && let Ok(num) = i64::from_str_radix(&cleaned, 16)
                {
                    return Token::Integer(num, raw_text);
                }
                return Token::InvalidLiteral(raw_text);
            }
            'b' | 'B' => {
                cursor.bump(); // 0
                cursor.bump(); // b
                let num_start = cursor.as_str();
                cursor.eat_while(|c| c == '0' || c == '1' || c == '_');
                let len = num_start.len() - cursor.as_str().len();
                let text = &num_start[..len];

                let raw_len = start_ptr.len() - cursor.as_str().len();
                let raw_text = SmolStr::new(&start_ptr[..raw_len]);

                let cleaned = text.replace('_', "");
                if !cleaned.is_empty()
                    && let Ok(num) = i64::from_str_radix(&cleaned, 2)
                {
                    return Token::Integer(num, raw_text);
                }
                return Token::InvalidLiteral(raw_text);
            }
            'o' | 'O' => {
                cursor.bump(); // 0
                cursor.bump(); // o
                let num_start = cursor.as_str();
                cursor.eat_while(|c| ('0'..='7').contains(&c) || c == '_');
                let len = num_start.len() - cursor.as_str().len();
                let text = &num_start[..len];

                let raw_len = start_ptr.len() - cursor.as_str().len();
                let raw_text = SmolStr::new(&start_ptr[..raw_len]);

                let cleaned = text.replace('_', "");
                if !cleaned.is_empty()
                    && let Ok(num) = i64::from_str_radix(&cleaned, 8)
                {
                    return Token::Integer(num, raw_text);
                }
                return Token::InvalidLiteral(raw_text);
            }
            _ => {}
        }
    }

    // Decimal loop
    let mut is_float = false;
    cursor.eat_while(|c| c.is_ascii_digit() || c == '_');

    // Check for fractional part
    if cursor.first() == '.' && cursor.second() != '.' {
        is_float = true;
        cursor.bump(); // .
        cursor.eat_while(|c| c.is_ascii_digit() || c == '_');

        // If we see another dot, it's definitely an invalid numeric literal
        if cursor.first() == '.' && cursor.second() != '.' {
            cursor.bump();
            cursor.eat_while(|c| c.is_ascii_digit() || c == '_' || c == '.');
            let len = start_ptr.len() - cursor.as_str().len();
            return Token::InvalidLiteral(SmolStr::new(&start_ptr[..len]));
        }
    }

    // Check for exponent
    if cursor.first() == 'e' || cursor.first() == 'E' {
        let mut iter = cursor.as_str().chars();
        iter.next(); // e
        let next = iter.next().unwrap_or('\0');
        let is_exp = if next == '+' || next == '-' {
            iter.next().unwrap_or('\0').is_ascii_digit()
        } else {
            next.is_ascii_digit()
        };

        if is_exp {
            is_float = true;
            cursor.bump(); // e
            if cursor.first() == '+' || cursor.first() == '-' {
                cursor.bump();
            }
            cursor.eat_while(|c| c.is_ascii_digit() || c == '_');

            // Exponents cannot have decimals
            if cursor.first() == '.' && cursor.second() != '.' {
                cursor.bump();
                cursor.eat_while(|c| c.is_ascii_digit() || c == '_' || c == '.');
                let len = start_ptr.len() - cursor.as_str().len();
                return Token::InvalidLiteral(SmolStr::new(&start_ptr[..len]));
            }
        } else {
            // Malformed exponent? '1e' or '1e+'
            is_float = true;
            cursor.bump(); // e
            if cursor.first() == '+' || cursor.first() == '-' {
                cursor.bump();
            }
        }
    }

    let len = start_ptr.len() - cursor.as_str().len();
    let raw_text = SmolStr::new(&start_ptr[..len]);

    // Check for multiple dots or trailing junk that should have been errors
    // Since this is a simple lexer, if we encounter another dot, it's either an error or part of another token.
    // If we have "1.2.3", the first call gets "1.2", next call gets ".3".

    let cleaned = raw_text.replace('_', "");

    if is_float {
        if let Ok(num) = cleaned.parse::<f64>() {
            Token::Float(num, raw_text)
        } else {
            Token::InvalidLiteral(raw_text)
        }
    } else if let Ok(num) = cleaned.parse::<i64>() {
        Token::Integer(num, raw_text)
    } else {
        Token::InvalidLiteral(raw_text)
    }
}
