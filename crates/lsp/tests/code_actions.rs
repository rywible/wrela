use crate::common::TestFixture;
use insta::assert_debug_snapshot;

mod common;

#[test]
fn test_unused_variable_diagnostic() {
    let code = r#"
    fn main() {
        let unused = 1
        let used = 2
        print(used)
    }
    "#;
    let fixture = TestFixture::new(code);
    let diagnostics = wrela_lsp::check_unused_variables(&fixture.state);
    assert_debug_snapshot!(diagnostics);
}
