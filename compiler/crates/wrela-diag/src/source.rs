//! Source files, spans and line/column lookup.

use std::fmt;
use std::sync::Arc;

/// Identifies a file in a [`SourceMap`]. Only a source map mints these.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FileId(u32);

impl FileId {
    /// The file's position in its source map, in the order files were added.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// A half-open byte range `start..end` in one file, with `start <= end`.
///
/// The fields are private so the order holds: [`Span::new`] is the only way to make one.
/// Offsets are `u32`: [`SourceMap::add`] rejects files of 4 GiB or more, so every offset fits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Span {
    file: FileId,
    start: u32,
    end: u32,
}

impl Span {
    /// A span from `start` to `end`; the two are swapped if they're out of order.
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        Span {
            file,
            start: start.min(end),
            end: start.max(end),
        }
    }

    /// A span from a `usize` range, as produced by slicing source text.
    pub fn from_range(file: FileId, range: std::ops::Range<usize>) -> Self {
        Span::new(file, offset_u32(range.start), offset_u32(range.end))
    }

    pub fn file(self) -> FileId {
        self.file
    }

    pub fn start(self) -> u32 {
        self.start
    }

    pub fn end(self) -> u32 {
        self.end
    }

    pub fn len(self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// The smallest span covering both. If the files differ, `self` is returned unchanged.
    pub fn cover(self, other: Span) -> Span {
        if self.file != other.file {
            return self;
        }
        Span::new(
            self.file,
            self.start.min(other.start),
            self.end.max(other.end),
        )
    }

    /// The span's byte range, for slicing source text.
    pub fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// The byte-order mark some editors write at the start of a UTF-8 file.
const BOM: char = '\u{FEFF}';

fn offset_u32(offset: usize) -> u32 {
    u32::try_from(offset).unwrap_or(u32::MAX)
}

/// A 1-based line and column. Columns count Unicode scalar values (chars), not bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct LineCol {
    pub line: u32,
    pub column: u32,
}

/// The error when a file is too large for `u32` byte offsets.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SourceTooLarge {
    pub name: String,
    pub len: usize,
}

impl fmt::Display for SourceTooLarge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is {} bytes; source files must be smaller than 4 GiB",
            crate::display_name(&self.name),
            self.len
        )
    }
}

impl std::error::Error for SourceTooLarge {}

/// One file's name and text, with an index of where its lines start.
#[derive(Clone, Debug)]
pub struct SourceFile {
    name: String,
    text: Arc<str>,
    /// Byte offset of the start of each line. Always starts with 0, so it's never empty.
    line_starts: Vec<u32>,
}

impl SourceFile {
    fn new(name: String, text: Arc<str>) -> Result<Self, SourceTooLarge> {
        if u32::try_from(text.len()).is_err() {
            return Err(SourceTooLarge {
                name,
                len: text.len(),
            });
        }
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| offset_u32(i + 1)))
            .collect();
        Ok(SourceFile {
            name,
            text,
            line_starts,
        })
    }

    /// The name diagnostics show for this file, usually the path as the user gave it.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn text_arc(&self) -> &Arc<str> {
        &self.text
    }

    /// The number of lines. A file ending in a newline has an empty last line, as editors show it.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// The 0-based index of the line containing `offset`. Offsets past the end clamp to the end.
    pub fn line_index(&self, offset: u32) -> usize {
        let offset = offset.min(self.len());
        // The first line start is 0 <= offset, so the partition point is at least 1.
        self.line_starts.partition_point(|&start| start <= offset) - 1
    }

    /// The byte offset where line `index` (0-based) starts; clamps to the last line.
    pub fn line_start(&self, index: usize) -> u32 {
        self.line_starts[index.min(self.line_starts.len() - 1)]
    }

    /// The text of line `index` (0-based), without its line ending (`\n` or `\r\n`) and, on the
    /// first line, without a leading byte-order mark, as an editor shows it.
    pub fn line_text(&self, index: usize) -> &str {
        &self.text[self.line_text_range(index)]
    }

    /// The byte range of [`SourceFile::line_text`]. A `\r` is only part of the line ending
    /// when a `\n` follows it; a lone one stays in the text.
    pub fn line_text_range(&self, index: usize) -> std::ops::Range<usize> {
        let index = index.min(self.line_starts.len() - 1);
        let mut start = self.line_starts[index] as usize;
        if index == 0 && self.text.starts_with(BOM) {
            start = BOM.len_utf8();
        }
        let end = match self.line_starts.get(index + 1) {
            Some(&next) => {
                let line = &self.text[start..next as usize - 1];
                start + line.strip_suffix('\r').unwrap_or(line).len()
            }
            None => self.text.len(),
        };
        start..end
    }

    /// The 1-based line and column of `offset`.
    ///
    /// Offsets past the end clamp to the end. An offset inside a multi-byte character counts that
    /// character, so a span's end column stays after its start column. A leading byte-order mark
    /// isn't counted: editors don't show it.
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = offset.min(self.len());
        let index = self.line_index(offset);
        let start = self.line_text_range(index).start;
        let offset = offset as usize;
        let mut chars = 0;
        if offset > start {
            let floor = self.text.floor_char_boundary(offset);
            chars = self.text[start..floor].chars().count();
            if floor != offset {
                chars += 1;
            }
        }
        LineCol {
            line: offset_u32(index + 1),
            column: offset_u32(chars + 1),
        }
    }

    /// The file's length in bytes.
    pub fn len(&self) -> u32 {
        // `new` checked that the length fits.
        offset_u32(self.text.len())
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// The files of one compilation, by [`FileId`].
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        SourceMap::default()
    }

    /// Adds a file and returns its id. Fails if the text is 4 GiB or larger.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        text: impl Into<Arc<str>>,
    ) -> Result<FileId, SourceTooLarge> {
        let file = SourceFile::new(name.into(), text.into())?;
        let id = u32::try_from(self.files.len()).map_err(|_| SourceTooLarge {
            name: file.name.clone(),
            len: file.text.len(),
        })?;
        self.files.push(file);
        Ok(FileId(id))
    }

    /// Replaces a file's text, keeping its name and id. Returns `Ok(false)` for an unknown id.
    pub fn replace(
        &mut self,
        id: FileId,
        text: impl Into<Arc<str>>,
    ) -> Result<bool, SourceTooLarge> {
        let Some(slot) = self.files.get_mut(id.index()) else {
            return Ok(false);
        };
        *slot = SourceFile::new(slot.name.clone(), text.into())?;
        Ok(true)
    }

    pub fn get(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.index())
    }

    /// Every file, with its id, in the order they were added.
    pub fn files(&self) -> impl Iterator<Item = (FileId, &SourceFile)> {
        // Ids are minted from indices that fit in u32, so the conversion is lossless.
        self.files
            .iter()
            .enumerate()
            .map(|(i, file)| (FileId(offset_u32(i)), file))
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> SourceFile {
        SourceFile::new("t.wrela".into(), text.into()).unwrap()
    }

    #[test]
    fn line_col_is_one_based_and_counts_chars() {
        let f = file("ab\nπx\n");
        assert_eq!(f.line_col(0), LineCol { line: 1, column: 1 });
        assert_eq!(f.line_col(2), LineCol { line: 1, column: 3 }); // the newline itself
        assert_eq!(f.line_col(3), LineCol { line: 2, column: 1 });
        assert_eq!(f.line_col(5), LineCol { line: 2, column: 2 }); // after the 2-byte `π`
        assert_eq!(f.line_col(7), LineCol { line: 3, column: 1 }); // end of file
    }

    #[test]
    fn offsets_inside_a_char_and_past_the_end_clamp() {
        let f = file("π");
        assert_eq!(f.line_col(1), LineCol { line: 1, column: 2 });
        assert_eq!(f.line_col(99), LineCol { line: 1, column: 2 });
        assert_eq!(f.line_index(99), 0);
    }

    #[test]
    fn line_text_strips_line_endings() {
        let f = file("one\r\ntwo\nthree");
        assert_eq!(f.line_count(), 3);
        assert_eq!(f.line_text(0), "one");
        assert_eq!(f.line_text(1), "two");
        assert_eq!(f.line_text(2), "three");
        assert_eq!(f.line_text(9), "three");
    }

    #[test]
    fn a_lone_carriage_return_stays_in_the_line() {
        let f = file("a\rb\r\nc\r");
        assert_eq!(f.line_text(0), "a\rb");
        assert_eq!(f.line_text(1), "c\r");
    }

    #[test]
    fn a_byte_order_mark_is_not_shown_or_counted() {
        let f = file("\u{FEFF}ab\n\u{FEFF}");
        assert_eq!(f.line_text(0), "ab");
        assert_eq!(f.line_col(0), LineCol { line: 1, column: 1 });
        assert_eq!(f.line_col(1), LineCol { line: 1, column: 1 }); // inside the mark
        assert_eq!(f.line_col(3), LineCol { line: 1, column: 1 });
        assert_eq!(f.line_col(4), LineCol { line: 1, column: 2 });
        // Only at the start of the file.
        assert_eq!(f.line_text(1), "\u{FEFF}");
        assert_eq!(f.line_col(9), LineCol { line: 2, column: 2 });
    }

    #[test]
    fn empty_file_has_one_empty_line() {
        let f = file("");
        assert_eq!(f.line_count(), 1);
        assert_eq!(f.line_text(0), "");
        assert_eq!(f.line_col(0), LineCol { line: 1, column: 1 });
    }

    #[test]
    fn source_map_mints_ids_in_order_and_replaces_text() {
        let mut map = SourceMap::new();
        let a = map.add("a.wrela", "x").unwrap();
        let b = map.add("b.wrela", "y").unwrap();
        assert_eq!((a.index(), b.index()), (0, 1));
        assert!(map.replace(b, "y\nz").unwrap());
        assert_eq!(map.get(b).unwrap().name(), "b.wrela");
        assert_eq!(map.get(b).unwrap().line_count(), 2);
    }

    #[test]
    fn span_new_orders_its_ends() {
        let mut map = SourceMap::new();
        let f = map.add("a.wrela", "abc").unwrap();
        assert_eq!(Span::new(f, 3, 1), Span::new(f, 1, 3));
        assert_eq!(
            Span::new(f, 0, 1).cover(Span::new(f, 2, 3)),
            Span::new(f, 0, 3)
        );
    }

    #[test]
    fn too_large_spells_out_invisible_characters_in_the_name() {
        let error = SourceTooLarge {
            name: "a\u{1B}[2J\n.wrela".into(),
            len: 1 << 32,
        };
        assert_eq!(
            error.to_string(),
            "`a<U+001B>[2J<U+000A>.wrela` is 4294967296 bytes; source files must be smaller than 4 GiB"
        );
    }
}
