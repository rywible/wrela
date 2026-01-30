use crate::common::TestFixture;
use insta::assert_debug_snapshot;

mod common;

#[test]
fn test_hover_doc_comment() {
    let code = r#"
    /// This is a documentation comment
    /// It has multiple lines
    to foo():
        return nothing
    "#;
    let fixture = TestFixture::new(code);
    // Position of 'foo'
    let hover = wrela_lsp::hover_at_position(&fixture.state, fixture.position(3, 7));
    assert_debug_snapshot!(hover);
}

#[test]
fn test_hover_inferred_variable() {
    let code = r#"
    to main():
        x = 10
    "#;
    let fixture = TestFixture::new(code);
    // Position of 'x'
    let hover = wrela_lsp::hover_at_position(&fixture.state, fixture.position(2, 8));
    assert_debug_snapshot!(hover);
}
