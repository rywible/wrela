use crate::parser::event::Event;
use crate::parser::kind::SyntaxKind;
use crate::parser::source::TokenSource;
use rowan::GreenNode;
use rowan::GreenNodeBuilder;

pub struct TreeSink<'a> {
    source: TokenSource<'a>,
    events: Vec<Event>,
    builder: GreenNodeBuilder<'static>,
}

impl<'a> TreeSink<'a> {
    pub fn new(source_text: &'a str, events: Vec<Event>) -> Self {
        Self::from_tokens(source_text, Vec::new(), events)
    }

    pub fn from_tokens(
        source_text: &'a str,
        tokens: Vec<crate::lexer::SpannedToken>,
        events: Vec<Event>,
    ) -> Self {
        let source = if tokens.is_empty() {
            TokenSource::new(source_text)
        } else {
            TokenSource::from_tokens(source_text, tokens)
        };
        Self {
            source,
            events,
            builder: GreenNodeBuilder::new(),
        }
    }

    pub fn finish(mut self) -> GreenNode {
        for i in 0..self.events.len() {
            match std::mem::replace(&mut self.events[i], Event::Placeholder) {
                Event::StartNode {
                    kind,
                    forward_parent,
                } => {
                    let mut kinds = vec![kind];
                    let mut fp = forward_parent;
                    let mut idx = i;
                    while let Some(distance) = fp {
                        idx += distance;
                        fp = match std::mem::replace(&mut self.events[idx], Event::Placeholder) {
                            Event::StartNode {
                                kind,
                                forward_parent,
                            } => {
                                kinds.push(kind);
                                forward_parent
                            }
                            _ => unreachable!(),
                        };
                    }

                    for kind in kinds.into_iter().rev() {
                        self.builder.start_node(rowan::SyntaxKind(kind.into()));
                    }
                }
                Event::AddToken => {
                    self.eat_trivia();
                    self.do_token();
                }
                Event::FinishNode => {
                    if i + 1 == self.events.len() {
                        self.eat_trivia();
                    }
                    self.builder.finish_node();
                }
                Event::Placeholder => {}
            }
        }

        self.builder.finish()
    }

    fn do_token(&mut self) {
        if let Some(token) = self.source.peek_token() {
            let kind = SyntaxKind::from(token.clone());
            let text = self.source.text();
            self.builder.token(rowan::SyntaxKind(kind.into()), text);
            self.source.bump();
        }
    }

    fn eat_trivia(&mut self) {
        while let Some(token) = self.source.peek_token() {
            let kind = SyntaxKind::from(token.clone());
            if !kind.is_trivia() {
                break;
            }
            let text = self.source.text();
            self.builder.token(rowan::SyntaxKind(kind.into()), text);
            self.source.bump();
        }
    }
}
