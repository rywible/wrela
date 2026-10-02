//! The human-readable text rendering, modelled on rustc's:
//!
//! ```text
//! error[E0001]: unexpected character `$`
//!  --> main.wrela:2:14
//!   |
//! 2 |     cost * 2 $
//!   |              ^ not part of any token
//!   |
//!   = note: ...
//! ```
//!
//! Tabs show as four spaces. Characters that aren't visible glyphs (see [`is_visible`]) show as
//! their code point, `<U+00A0>`, so an invisible character can be seen and a control or
//! bidirectional override can't rearrange the terminal; file names are shown the same way (see
//! [`display_name`]). Columns assume every other character is one cell wide.

use std::fmt::Write as _;

use crate::{Diagnostic, Edit, FileId, Label, SourceFile, SourceMap, display_name, is_visible};

const TAB_WIDTH: usize = 4;

/// Renders every diagnostic, each followed by a blank line.
pub fn render_all(diagnostics: &[Diagnostic], sources: &SourceMap) -> String {
    let mut out = String::new();
    for diagnostic in diagnostics {
        out.push_str(&render(diagnostic, sources));
        out.push('\n');
    }
    out
}

/// Renders one diagnostic.
pub fn render(diagnostic: &Diagnostic, sources: &SourceMap) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}[{}]: {}",
        diagnostic.severity, diagnostic.code, diagnostic.message
    );

    let groups = group_labels(diagnostic);
    let hunks: Vec<(&str, Vec<Hunk>)> = diagnostic
        .help
        .iter()
        .filter(|help| !help.edits.is_empty())
        .map(|help| (help.message.as_str(), hunks(&help.edits, sources)))
        .collect();

    let max_line = groups
        .iter()
        .flat_map(|(file, labels)| {
            let source = sources.get(*file);
            labels
                .iter()
                .filter_map(move |label| source.map(|s| last_line(s, label) + 1))
        })
        .chain(
            hunks
                .iter()
                .flat_map(|(_, hunks)| hunks.iter().map(Hunk::last_line_number)),
        )
        .max()
        .unwrap_or(1);
    let width = max_line.to_string().len();
    let pad = " ".repeat(width);

    for (i, (file, labels)) in groups.iter().enumerate() {
        let arrow = if i == 0 { "-->" } else { ":::" };
        let Some(source) = sources.get(*file) else {
            let _ = writeln!(out, "{pad}{arrow} <unknown file #{}>", file.index());
            continue;
        };
        let anchor = labels[0].span.start();
        let at = source.line_col(anchor);
        let _ = writeln!(
            out,
            "{pad}{arrow} {}:{}:{}",
            display_name(source.name()),
            at.line,
            at.column
        );
        let _ = writeln!(out, "{pad} |");
        render_snippet(&mut out, source, labels, diagnostic, &pad);
    }

    let plain_help = diagnostic.help.iter().filter(|help| help.edits.is_empty());
    let footer: Vec<(&str, &str)> = diagnostic
        .notes
        .iter()
        .map(|note| ("note", note.as_str()))
        .chain(plain_help.map(|help| ("help", help.message.as_str())))
        .collect();
    if !footer.is_empty() || !hunks.is_empty() {
        let _ = writeln!(out, "{pad} |");
    }
    if !footer.is_empty() {
        for (kind, text) in footer {
            let indent = format!("{pad}   {}", " ".repeat(kind.len() + 2));
            let text = text.replace('\n', &format!("\n{indent}"));
            let _ = writeln!(out, "{pad} = {kind}: {text}");
        }
    }

    for (message, hunks) in &hunks {
        let _ = writeln!(out, "help: {message}");
        for hunk in hunks {
            let Some(source) = sources.get(hunk.file) else {
                continue;
            };
            if hunk.file != diagnostic.primary.span.file() {
                let _ = writeln!(out, "{pad}::: {}", display_name(source.name()));
            }
            let _ = writeln!(out, "{pad} |");
            for (n, line) in hunk.text.split('\n').enumerate() {
                let number = hunk.first_line + n + 1;
                let _ = writeln!(out, "{number:>width$} | {}", display_line(line).trim_end());
            }
        }
    }
    out
}

/// Labels grouped by file: the primary span's file first, then others in order of appearance.
fn group_labels(diagnostic: &Diagnostic) -> Vec<(FileId, Vec<&Label>)> {
    let mut groups: Vec<(FileId, Vec<&Label>)> = Vec::new();
    for label in std::iter::once(&diagnostic.primary).chain(&diagnostic.secondary) {
        match groups
            .iter_mut()
            .find(|(file, _)| *file == label.span.file())
        {
            Some((_, labels)) => labels.push(label),
            None => groups.push((label.span.file(), vec![label])),
        }
    }
    groups
}

fn render_snippet(
    out: &mut String,
    source: &SourceFile,
    labels: &[&Label],
    diagnostic: &Diagnostic,
    pad: &str,
) {
    // The lines to show: each label's first line and, for a multi-line label, its last.
    let mut lines: Vec<usize> = labels
        .iter()
        .flat_map(|label| {
            [
                source.line_index(label.span.start()),
                last_line(source, label),
            ]
        })
        .collect();
    lines.sort_unstable();
    lines.dedup();

    let width = pad.len();
    let mut previous: Option<usize> = None;
    for &line in &lines {
        if previous.is_some_and(|p| line > p + 1) {
            let _ = writeln!(out, "...");
        }
        previous = Some(line);
        let text = source.line_text(line);
        let _ = writeln!(
            out,
            "{:>width$} | {}",
            line + 1,
            display_line(text).trim_end()
        );

        let mut marks: Vec<(usize, usize, char, Option<&str>)> = Vec::new();
        for label in labels {
            let is_primary = std::ptr::eq(*label, &diagnostic.primary);
            let mark = if is_primary { '^' } else { '-' };
            let start_line = source.line_index(label.span.start());
            let end_line = last_line(source, label);
            let message = label.message.as_deref();
            let col = |at: u32| display_col(source, line, at as usize);
            if start_line == line && end_line == line {
                let from = col(label.span.start());
                let to = col(label.span.end());
                marks.push((from, to.saturating_sub(from).max(1), mark, message));
            } else if start_line == line {
                // A multi-line label underlines to the end of its first line...
                let from = col(label.span.start());
                let to = display_width(text);
                marks.push((from, to.saturating_sub(from).max(1), mark, None));
            } else if end_line == line {
                // ...and from its last line's indentation to its end, where the message goes.
                let indent = text.len() - text.trim_start().len();
                let from = display_col(source, line, source.line_text_range(line).start + indent);
                let to = col(label.span.end());
                marks.push((from, to.saturating_sub(from).max(1), mark, message));
            }
        }
        marks.sort_by_key(|&(from, ..)| from);
        for (from, len, mark, message) in marks {
            let underline = mark.to_string().repeat(len);
            let message = message.map(|m| format!(" {m}")).unwrap_or_default();
            let _ = writeln!(out, "{pad} | {}{underline}{message}", " ".repeat(from));
        }
    }
}

/// The 0-based line of a label's last character, so a span ending in a newline stays on its line.
fn last_line(source: &SourceFile, label: &Label) -> usize {
    let span = label.span;
    source.line_index(if span.end() > span.start() {
        span.end() - 1
    } else {
        span.start()
    })
}

/// One contiguous run of edited lines, with the edits applied.
struct Hunk {
    file: FileId,
    /// The 0-based index of the first line shown.
    first_line: usize,
    text: String,
}

impl Hunk {
    fn last_line_number(&self) -> usize {
        self.first_line + self.text.split('\n').count()
    }
}

/// Groups edits into hunks of whole lines and applies them. Overlapping edits after the first are
/// skipped rather than producing garbled text.
fn hunks(edits: &[Edit], sources: &SourceMap) -> Vec<Hunk> {
    let mut edits: Vec<&Edit> = edits.iter().collect();
    edits.sort_by_key(|edit| (edit.span.file(), edit.span.start(), edit.span.end()));

    let mut hunks = Vec::new();
    let mut i = 0;
    while i < edits.len() {
        let file = edits[i].span.file();
        let Some(source) = sources.get(file) else {
            i += 1;
            continue;
        };
        let first = source.line_index(edits[i].span.start());
        let mut last = source.line_index(edits[i].span.end());
        let mut group = vec![edits[i]];
        i += 1;
        while i < edits.len()
            && edits[i].span.file() == file
            && source.line_index(edits[i].span.start()) <= last
        {
            last = last.max(source.line_index(edits[i].span.end()));
            group.push(edits[i]);
            i += 1;
        }

        let start = source.line_text_range(first).start;
        let end = start_of_next_line(source, last);
        let text = source.text();
        let mut patched = String::new();
        let mut cursor = start;
        for edit in group {
            let edit_start = clamp(text, edit.span.start() as usize).max(start);
            let edit_end = clamp(text, edit.span.end() as usize).min(end);
            if edit_start < cursor {
                continue;
            }
            patched.push_str(&text[cursor..edit_start]);
            patched.push_str(&edit.replacement);
            cursor = edit_end.max(edit_start);
        }
        patched.push_str(&text[cursor..end]);
        let patched = patched.strip_suffix('\n').unwrap_or(&patched);
        let patched = patched.strip_suffix('\r').unwrap_or(patched);
        hunks.push(Hunk {
            file,
            first_line: first,
            text: patched.to_string(),
        });
    }
    hunks
}

/// The byte offset just past line `index`'s text and line ending.
fn start_of_next_line(source: &SourceFile, index: usize) -> usize {
    if index + 1 < source.line_count() {
        source.line_start(index + 1) as usize
    } else {
        source.text().len()
    }
}

fn clamp(text: &str, offset: usize) -> usize {
    text.floor_char_boundary(offset.min(text.len()))
}

/// The display column of byte offset `at` on line `line` (0-based), as the line is printed: an
/// offset in the line ending counts as the end of the line's text.
fn display_col(source: &SourceFile, line: usize, at: usize) -> usize {
    let text = source.text();
    let range = source.line_text_range(line);
    let to = clamp(text, at.clamp(range.start, range.end));
    display_width(&text[range.start..to])
}

fn display_width(text: &str) -> usize {
    let width = |c| match c {
        '\t' => TAB_WIDTH,
        c if c != ' ' && !is_visible(c) => display_char(c).len(),
        _ => 1,
    };
    text.chars().map(width).sum()
}

/// The line as it's printed: tabs expanded, invisible characters spelled out.
fn display_line(text: &str) -> String {
    text.chars().map(display_char).collect()
}

fn display_char(c: char) -> std::borrow::Cow<'static, str> {
    match c {
        ' ' => " ".into(),
        '\t' => " ".repeat(TAB_WIDTH).into(),
        c if !is_visible(c) => format!("<U+{:04X}>", u32::from(c)).into(),
        c => c.to_string().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes::{BLOCK_COMMENT, UNEXPECTED_CHARACTER};
    use crate::{Diagnostic, Edit, Span};

    fn map(text: &str) -> (SourceMap, FileId) {
        let mut map = SourceMap::new();
        let file = map.add("main.wrela", text).unwrap();
        (map, file)
    }

    #[test]
    fn renders_header_location_snippet_and_label() {
        let (map, f) = map("fn f() {\n    cost * 2 $\n}\n");
        let d = Diagnostic::new(
            UNEXPECTED_CHARACTER,
            "unexpected character `$`",
            Span::new(f, 22, 23),
        )
        .with_label("not part of any token");
        let expected = "\
error[E0001]: unexpected character `$`
 --> main.wrela:2:14
  |
2 |     cost * 2 $
  |              ^ not part of any token
";
        assert_eq!(render(&d, &map), expected);
    }

    #[test]
    fn renders_notes_help_and_suggestions() {
        let (map, f) = map("/* hi */\nx\n");
        let d = Diagnostic::new(
            BLOCK_COMMENT,
            "`/* */` comments aren't supported",
            Span::new(f, 0, 8),
        )
        .with_note("a note")
        .with_help("plain help")
        .with_fix(
            "write a `//` comment",
            vec![Edit::new(Span::new(f, 0, 8), "// hi")],
        );
        let expected = "\
error[E0002]: `/* */` comments aren't supported
 --> main.wrela:1:1
  |
1 | /* hi */
  | ^^^^^^^^
  |
  = note: a note
  = help: plain help
help: write a `//` comment
  |
1 | // hi
";
        assert_eq!(render(&d, &map), expected);
    }

    #[test]
    fn multi_line_spans_mark_first_and_last_lines() {
        let (map, f) = map("a /* one\ntwo\n  three */ b\n");
        let d = Diagnostic::new(BLOCK_COMMENT, "m", Span::new(f, 2, 23)).with_label("here");
        let expected = "\
error[E0002]: m
 --> main.wrela:1:3
  |
1 | a /* one
  |   ^^^^^^
...
3 |   three */ b
  |   ^^^^^^^^ here
";
        assert_eq!(render(&d, &map), expected);
    }

    #[test]
    fn tabs_expand_and_invisible_characters_are_spelled_out() {
        let (map, f) = map("\tx\u{1b}[31m\u{a0}$\n");
        let d = Diagnostic::new(UNEXPECTED_CHARACTER, "m", Span::new(f, 7, 9));
        let rendered = render(&d, &map);
        assert!(
            rendered.contains("1 |     x<U+001B>[31m<U+00A0>$\n"),
            "{rendered}"
        );
        assert!(
            rendered.contains("  |                  ^^^^^^^^\n"),
            "{rendered}"
        );
    }

    #[test]
    fn crlf_line_endings_are_not_underlined() {
        let (map, f) = map("let x = foo(a, b)\r\nnext\r\n");
        let d = Diagnostic::new(UNEXPECTED_CHARACTER, "m", Span::new(f, 0, 19));
        let rendered = render(&d, &map);
        assert!(
            rendered.contains(&format!("1 | let x = foo(a, b)\n  | {}\n", "^".repeat(17))),
            "{rendered}"
        );
    }

    #[test]
    fn a_byte_order_mark_is_not_shown() {
        let (map, f) = map("\u{FEFF}fn $\n");
        let d = Diagnostic::new(UNEXPECTED_CHARACTER, "m", Span::new(f, 6, 7))
            .with_fix("delete it", vec![Edit::new(Span::new(f, 6, 7), "")]);
        let expected = "\
error[E0001]: m
 --> main.wrela:1:4
  |
1 | fn $
  |    ^
  |
help: delete it
  |
1 | fn
";
        assert_eq!(render(&d, &map), expected);
    }

    /// A file name comes from the file system, so it can hold anything but `/` and NUL.
    const HOSTILE_NAME: &str = "a\u{1B}[31m\u{202E}\nerror: forged.wrela";
    const HOSTILE_SHOWN: &str = "a<U+001B>[31m<U+202E><U+000A>error: forged.wrela";

    #[test]
    fn file_names_spell_out_invisible_characters() {
        let mut map = SourceMap::new();
        let f = map.add(HOSTILE_NAME, "$\n").unwrap();
        let d = Diagnostic::new(UNEXPECTED_CHARACTER, "m", Span::new(f, 0, 1));
        let rendered = render(&d, &map);
        assert!(
            rendered.contains(&format!(" --> {HOSTILE_SHOWN}:1:1\n")),
            "{rendered}"
        );
        assert!(!rendered.contains(['\u{1B}', '\u{202E}']), "{rendered}");
        assert_eq!(rendered.lines().count(), 5, "{rendered}");
    }

    #[test]
    fn a_fix_in_another_file_spells_out_its_name() {
        let mut map = SourceMap::new();
        let main = map.add("main.wrela", "$\n").unwrap();
        let other = map.add(HOSTILE_NAME, "x\n").unwrap();
        let d = Diagnostic::new(UNEXPECTED_CHARACTER, "m", Span::new(main, 0, 1))
            .with_fix("rename it", vec![Edit::new(Span::new(other, 0, 1), "y")]);
        let rendered = render(&d, &map);
        assert!(
            rendered.contains(&format!("\n ::: {HOSTILE_SHOWN}\n")),
            "{rendered}"
        );
        assert!(!rendered.contains(['\u{1B}', '\u{202E}']), "{rendered}");
    }

    #[test]
    fn spans_past_the_end_or_in_unknown_files_do_not_panic() {
        let (map, f) = map("π");
        let d = Diagnostic::new(UNEXPECTED_CHARACTER, "m", Span::new(f, 1, 99))
            .with_fix("x", vec![Edit::new(Span::new(f, 1, 50), "y")]);
        let _ = render(&d, &map);
        let empty = SourceMap::new();
        assert!(render(&d, &empty).contains("--> <unknown file #0>"));
    }
}
