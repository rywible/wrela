pub use crate::token::TokenId;
use crate::token::TokenKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}
impl ByteRange {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
    pub const fn empty(at: usize) -> Self {
        Self::new(at, at)
    }
    pub fn cover(self, other: Self) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }
    pub fn contains(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end && other.start <= other.end
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriviaKind {
    Whitespace,
    Newline,
    LineComment,
    BlockComment,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PieceKind {
    Token(TokenKind),
    Trivia(TriviaKind),
    Invalid,
    Unparsed,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Piece {
    pub kind: PieceKind,
    pub range: ByteRange,
    pub bytes: Vec<u8>,
}
/// Sole ownership of source bytes lives in this ordered tape. Syntax only refers to piece IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub pieces: Vec<Piece>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorPosition {
    pub line: usize,
    pub column: usize,
}
impl Source {
    pub fn len(&self) -> usize {
        self.pieces.last().map_or(0, |p| p.range.end)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn render(&self) -> Vec<u8> {
        self.pieces
            .iter()
            .flat_map(|p| p.bytes.iter().copied())
            .collect()
    }
    pub fn piece(&self, id: TokenId) -> &Piece {
        &self.pieces[id.0]
    }
    pub fn range(&self, id: TokenId) -> ByteRange {
        self.piece(id).range
    }
    pub fn token_bytes(&self, id: TokenId) -> &[u8] {
        &self.piece(id).bytes
    }
    pub fn token_text(&self, id: TokenId) -> Option<&str> {
        std::str::from_utf8(self.token_bytes(id)).ok()
    }
    /// Invalid UTF-8 anywhere makes editor coordinates unavailable. CRLF is one newline.
    /// An offset between CR and LF is anchored at the end of the preceding line.
    pub fn editor_position(&self, offset: usize) -> Option<EditorPosition> {
        if offset > self.len() {
            return None;
        }
        let bytes = self.render();
        let text = std::str::from_utf8(&bytes).ok()?;
        if !text.is_char_boundary(offset) {
            return None;
        }
        let mut line = 0;
        let mut column = 0;
        let mut chars = text.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            if i >= offset {
                break;
            }
            if c == '\r' {
                if let Some(&(j, '\n')) = chars.peek() {
                    if j >= offset {
                        break;
                    }
                    chars.next();
                }
                line += 1;
                column = 0;
            } else if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += c.len_utf16();
            }
        }
        Some(EditorPosition { line, column })
    }
    pub fn validate(&self) -> bool {
        let mut end = 0;
        for p in &self.pieces {
            if p.range.start != end || p.range.end < end || p.range.end - end != p.bytes.len() {
                return false;
            }
            end = p.range.end;
        }
        true
    }
    pub fn replace_token(&self, id: TokenId, replacement: &[u8]) -> Result<Vec<u8>, EditError> {
        if !self.validate() {
            return Err(EditError::InvalidSource);
        }
        if !matches!(
            self.pieces.get(id.0).ok_or(EditError::UnknownToken)?.kind,
            PieceKind::Token(_)
        ) {
            return Err(EditError::NotToken);
        }
        Ok(self
            .pieces
            .iter()
            .enumerate()
            .flat_map(|(i, p)| {
                if i == id.0 { replacement } else { &p.bytes }
                    .iter()
                    .copied()
            })
            .collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    UnknownToken,
    NotToken,
    InvalidSource,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{Limits, lex};
    #[test]
    fn utf16_coordinates_and_line_endings() {
        let out = lex("a😀\r\né\rx\nz".as_bytes(), Limits::default());
        assert_eq!(
            out.source.editor_position(5),
            Some(EditorPosition { line: 0, column: 3 })
        );
        assert_eq!(
            out.source.editor_position(7),
            Some(EditorPosition { line: 1, column: 0 })
        );
        assert_eq!(
            out.source.editor_position(10),
            Some(EditorPosition { line: 2, column: 0 })
        );
        assert_eq!(
            out.source.editor_position(12),
            Some(EditorPosition { line: 3, column: 0 })
        );
        assert_eq!(out.source.editor_position(2), None);
        assert_eq!(
            lex(b"a\xff", Limits::default()).source.editor_position(0),
            None
        );
    }
    #[test]
    fn edits_preserve_unrelated_bytes_and_recompute_positions_by_relexing() {
        let bytes = b"// c\r\nlet /* c */ x = 1\r\n";
        let out = lex(bytes, Limits::default());
        let id = out
            .tokens
            .iter()
            .find(|(_, t, _)| t.kind() == TokenKind::Ident)
            .unwrap()
            .1
            .id();
        let edited = out.source.replace_token(id, "π".as_bytes()).unwrap();
        assert_eq!(edited, "// c\r\nlet /* c */ π = 1\r\n".as_bytes());
        let again = lex(&edited, Limits::default());
        assert!(again.source.validate());
        assert_eq!(again.source.render(), edited);
        assert_eq!(
            out.source.replace_token(TokenId(9999), b"x"),
            Err(EditError::UnknownToken)
        );
        assert_eq!(
            out.source.replace_token(TokenId(0), b"x"),
            Err(EditError::NotToken)
        );
    }
}
