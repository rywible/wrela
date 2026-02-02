use tower_lsp::lsp_types::Position;
use wrela_lsp::{DocumentState, build_document_state};

pub struct TestFixture {
    pub state: DocumentState,
}

impl TestFixture {
    pub fn new(text: &str) -> Self {
        let (state, _) = build_document_state(text.to_string());
        Self { state }
    }

    #[allow(dead_code)]
    pub fn position(&self, line: u32, character: u32) -> Position {
        Position { line, character }
    }
}
