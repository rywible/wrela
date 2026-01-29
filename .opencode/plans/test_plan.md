# Test Plan

1. **Refactor**: Split `crates/lsp` into a library and binary.
    *   `src/lib.rs`: `Backend`, `DocumentState`, `SymbolIndex`, logic functions. Make them `pub`.
    *   `src/main.rs`: `tokio::main` entry point calling `Server::new`.
2.  **Dependencies**: Add `insta` for snapshot testing.
3.  **Test Infrastructure**:
    *   Helper to create a `Backend` with a mock client (or just ignore client calls if possible, though `publish_diagnostics` uses it).
    *   Actually, `tower_lsp::Client` is hard to mock easily without a real connection.
    *   However, for *logic* tests (Semantic Tokens, Hover, Inlay Hints), we don't need the `Client` if we test the pure functions `semantic_tokens`, `hover_at_position`, etc.
    *   We should expose these pure functions in `lib.rs`.
4.  **Test Coverage**:
    *   **Semantic Tokens**: Test comprehensive keywords, operators, literals, and complex nesting.
    *   **Hover**: Test all symbol kinds, doc comments, inferred types.
    *   **Inlay Hints**: Test params, missing return types.
    *   **Code Actions**: Test unused variable detection and removal.
