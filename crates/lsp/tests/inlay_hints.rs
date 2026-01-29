use crate::common::TestFixture;
use insta::assert_debug_snapshot;

mod common;

#[test]
fn test_inlay_hints_params() {
    let code = r#"
    fn add(a: Int, b: Int) -> Int { a + b }
    fn main() {
        add(1, 2)
    }
    "#;
    let fixture = TestFixture::new(code);
    let hints = wrela_lsp::inlay_hints(&fixture.state);
    assert_debug_snapshot!(hints);
}

#[test]
fn test_inlay_hints_return_type() {
    let code = r#"
    fn simple() {
        return 42
    }
    "#;
    let fixture = TestFixture::new(code);
    let hints = wrela_lsp::inlay_hints(&fixture.state);
    assert_debug_snapshot!(hints);
}
