use crate::common::TestFixture;
use insta::assert_debug_snapshot;
mod common;

#[test]
fn test_semantic_tokens_basic() {
    let code = r#"
    class Foo {
        public field: Int
        
        fn bar(x: Int) -> Int {
            return x + 1
        }
    }
    "#;
    let fixture = TestFixture::new(code);
    let tokens = wrela_lsp::semantic_tokens(&fixture.state);

    // Transform tokens to a more readable format for snapshotting if needed
    // But debug snapshot is fine for now
    assert_debug_snapshot!(tokens);
}

#[test]
fn test_semantic_tokens_operators() {
    let code = "x += 1 + 2 * 3";
    let fixture = TestFixture::new(code);
    let tokens = wrela_lsp::semantic_tokens(&fixture.state);
    assert_debug_snapshot!(tokens);
}

#[test]
fn test_semantic_tokens_comments() {
    let code = r#"
    // Line comment
    /// Doc comment
    class A {}
    "#;
    let fixture = TestFixture::new(code);
    let tokens = wrela_lsp::semantic_tokens(&fixture.state);
    assert_debug_snapshot!(tokens);
}
