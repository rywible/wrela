//! Fenced code blocks in Markdown, enough of CommonMark for docs and explanations.

/// A fenced code block.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Block {
    /// The info string after the opening fence, trimmed: `wrela imagined`.
    pub info: String,
    pub content: String,
    /// 1-based line of the opening fence.
    pub line: usize,
}

impl Block {
    /// The info string's words: `["wrela", "imagined"]`.
    pub fn tags(&self) -> Vec<&str> {
        self.info.split_whitespace().collect()
    }
}

/// Every fenced block (backticks or tildes, three or more). Content lines lose up to the opening
/// fence's indentation; an unclosed block runs to the end of the text.
pub fn fenced_blocks(text: &str) -> Vec<Block> {
    struct Open {
        fence: char,
        len: usize,
        indent: usize,
        info: String,
        line: usize,
        content: String,
    }

    let mut blocks = Vec::new();
    let mut open: Option<Open> = None;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        let fence = trimmed.chars().next().filter(|&c| c == '`' || c == '~');
        let run = fence.map_or(0, |f| trimmed.chars().take_while(|&c| c == f).count());

        if let Some(block) = &mut open {
            let rest = &trimmed[run.min(trimmed.len())..];
            if fence == Some(block.fence) && run >= block.len && rest.trim().is_empty() {
                let done = open.take().map(|b| Block {
                    info: b.info,
                    content: b.content,
                    line: b.line,
                });
                blocks.extend(done);
            } else {
                let strip = line.len() - line.trim_start_matches(' ').len();
                block.content.push_str(&line[strip.min(block.indent)..]);
                block.content.push('\n');
            }
        } else if let Some(f) = fence
            && run >= 3
            && indent <= 3
        {
            let info = trimmed[run..].trim().to_string();
            // A backtick fence's info string can't contain backticks (CommonMark).
            if f == '`' && info.contains('`') {
                continue;
            }
            open = Some(Open {
                fence: f,
                len: run,
                indent,
                info,
                line: index + 1,
                content: String::new(),
            });
        }
    }
    blocks.extend(open.map(|b| Block {
        info: b.info,
        content: b.content,
        line: b.line,
    }));
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_blocks_with_info_lines_and_content() {
        let text = "intro\n```wrela imagined\nlet x = 1\n```\n\n~~~~\na\n```\n~~~~\n  ```wrela\n  b\n  ```\n";
        let blocks = fenced_blocks(text);
        assert_eq!(blocks.len(), 3);
        assert_eq!(
            (blocks[0].tags(), blocks[0].line),
            (vec!["wrela", "imagined"], 2)
        );
        assert_eq!(blocks[0].content, "let x = 1\n");
        assert_eq!(blocks[1].content, "a\n```\n");
        assert_eq!(
            (blocks[2].info.as_str(), blocks[2].content.as_str()),
            ("wrela", "b\n")
        );
    }

    #[test]
    fn an_unclosed_block_runs_to_the_end() {
        let blocks = fenced_blocks("```wrela\nx\n");
        assert_eq!(blocks[0].content, "x\n");
    }
}
