//! Bounded prospective type recognition and generic-call punctuation selection.
use crate::token::{Tok, TokenId, TokenKind, TokenKind::*};

/// A bounded syntax-only recognizer for prospective type lists. No symbol lookup.
pub(crate) struct ProspectiveTypes<'a> {
    pub(crate) kinds: &'a [TokenKind],
    pub(crate) at: usize,
    pub(crate) visited: usize,
    pub(crate) limit: usize,
    pub(crate) depth_limit: usize,
    pub(crate) angles: Vec<(usize, usize)>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Failure {
    NotType,
    Tokens,
    Depth,
}
impl ProspectiveTypes<'_> {
    pub(crate) fn recognize(&mut self) -> Result<(), Failure> {
        if self.depth_limit == 0 {
            return Err(Failure::Depth);
        }
        let mut stack = Vec::new();
        let mut done = false;
        let mut type_depth = crate::type_syntax::TypeDepth::bounded(self.depth_limit);
        let input = std::iter::from_fn(|| {
            if done {
                return None;
            }
            let kind = *self.kinds.get(self.at)?;
            if self.visited > self.limit {
                done = true;
                return Some(Err("prospective tokens"));
            }
            let index = self.at;
            self.at += 1;
            self.visited += 1;
            let kind = match kind {
                Less => {
                    stack.push(index);
                    TypeOpen
                }
                Greater => {
                    if let Some(open) = stack.pop() {
                        self.angles.push((open, index));
                        done = stack.is_empty();
                    }
                    TypeClose
                }
                other => other,
            };
            Some(Ok((index, Tok::new(kind, TokenId(index)), index + 1)))
        });
        let result = crate::parsing::type_probe::ProspectiveArgumentsParser::new()
            .parse(&mut type_depth, input);
        match result {
            Ok(_) if self.visited <= self.limit => Ok(()),
            Ok(_) => Err(Failure::Tokens),
            Err(lalrpop_util::ParseError::User {
                error: "prospective depth",
            }) => Err(Failure::Depth),
            Err(lalrpop_util::ParseError::User { .. }) => Err(Failure::Tokens),
            Err(error) => {
                let type_start = match &error {
                    lalrpop_util::ParseError::UnrecognizedEof { expected, .. }
                    | lalrpop_util::ParseError::UnrecognizedToken { expected, .. } => {
                        type_starts().iter().all(|start| expected.contains(start))
                    }
                    _ => false,
                };
                // The adopted budget excludes non-type kind peeks, but counts a
                // failed required terminal such as a missing closing angle.
                let visits = match error {
                    lalrpop_util::ParseError::UnrecognizedToken { .. } if type_start => {
                        self.visited.saturating_sub(1)
                    }
                    lalrpop_util::ParseError::UnrecognizedEof { .. } if !type_start => {
                        self.visited + 1
                    }
                    _ => self.visited,
                };
                if visits > self.limit {
                    Err(Failure::Tokens)
                } else if type_start && type_depth.current + 1 > type_depth.maximum {
                    Err(Failure::Depth)
                } else {
                    Err(Failure::NotType)
                }
            }
        }
    }
}

// Ask the generated type parser for FIRST(Type) once. The authored productions
// remain authoritative when new type forms are added; there is no token table.
fn type_starts() -> &'static [std::string::String] {
    static STARTS: std::sync::OnceLock<Vec<std::string::String>> = std::sync::OnceLock::new();
    STARTS.get_or_init(|| {
        let mut depth = crate::type_syntax::TypeDepth::unlimited();
        let error = crate::parsing::type_probe::TypeParser::new()
            .parse(
                &mut depth,
                std::iter::empty::<Result<(usize, Tok, usize), &'static str>>(),
            )
            .expect_err("an empty input is not a type");
        let starts = crate::parsing::expected(&error);
        assert!(!starts.is_empty(), "generated FIRST(Type) must be nonempty");
        starts
    })
}
