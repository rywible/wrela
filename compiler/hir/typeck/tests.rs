#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::lower::lower;
    use crate::parser::ast::AstNode;
    use crate::parser::{ast, parse};

    #[test]
    fn test_type_error_binary() {
        let input = r#"fn f() -> Integer {
    return 1 + true
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_type_error_unary() {
        let input = r#"fn f() -> Boolean {
    return not 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidUnaryOperand { .. }))
        );
    }

    #[test]
    fn test_param_type_used() {
        let input = r#"fn f(x: Integer) -> Integer {
    return x + 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_param_type_mismatch() {
        let input = r#"fn f(x: Integer) -> Integer {
    return x + true
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_match_without_otherwise_all_variants_enum_ok() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending: return 1
        Status.Done: return 2
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_or_pattern_pipe_all_variants_enum_ok() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending | Status.Done: return 1
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_structural_pattern_binds_class_fields() {
        let input = r#"class User {
    has {
        id: Integer
        name: String

    }
}
fn f(user: User) -> Integer {
    match user {
        User { id }: return id
        otherwise: return 0
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_structural_pattern_covers_enum_variant() {
        let input = r#"enum Status {
    Pending
    Processing(worker_id: Integer)

}
fn f(status: Status) -> Integer {
    match status {
        Status.Pending: return 0
        Status.Processing { worker_id }: return worker_id
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_match_guard_must_be_boolean() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending if 1: return 1
        Status.Done: return 2
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchGuardNotBoolean { .. }))
        );
    }

    #[test]
    fn test_match_guarded_cases_are_not_exhaustive_without_otherwise() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status, is_ready: Boolean) -> Integer {
    match s {
        Status.Pending | Status.Done if is_ready: return 1
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchNonExhaustive { .. }))
        );
    }

    #[test]
    fn test_match_case_unreachable_after_wildcard() {
        let input = r#"fn f(r: Result[Integer]) -> Integer {
    match r {
        _: return 0
        Ok(value): return value
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchCaseUnreachable { .. }))
        );
    }

    #[test]
    fn test_match_case_unreachable_after_full_enum_coverage() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending: return 1
        Status.Done: return 2
        Status.Pending: return 3
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchCaseUnreachable { .. }))
        );
    }

    #[test]
    fn test_match_without_otherwise_non_exhaustive_enum_error() {
        let input = r#"enum Status {
    Pending
    Done

}
fn f(s: Status) -> Integer {
    match s {
        Status.Pending: return 1
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchNonExhaustive { .. }))
        );
    }

    #[test]
    fn test_match_without_otherwise_ok_err_result_ok() {
        let input = r#"fn f(r: Result[Integer]) -> Integer {
    match r {
        Ok(x): return x
        Err(_): return 0
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_match_without_otherwise_non_exhaustive_result_error() {
        let input = r#"fn f(r: Result[Integer]) -> Integer {
    match r {
        Ok(x): return x
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MatchNonExhaustive { .. }))
        );
    }

    #[test]
    fn test_string_concat_allowed() {
        let input = r#"fn f() -> String {
    return "a" + "b"
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_assignment_type_mismatch() {
        let input = r#"fn f(x: String) -> Nothing {
    x += 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAssignment { .. }))
        );
    }

    #[test]
    fn test_return_type_mismatch() {
        let input = r#"fn f() -> Boolean {
    return 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ReturnTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_if_condition_must_be_boolean() {
        let input = r#"fn f() -> Integer {
    if 1 {
        return 1
    }
    return 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::IfConditionNotBoolean { .. }))
        );
    }

    #[test]
    fn test_while_condition_must_be_boolean() {
        let input = r#"fn f() -> Integer {
    while 1 {
        return 1
    }
    return 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::WhileConditionNotBoolean { .. }))
        );
    }

    #[test]
    fn test_logical_and_requires_boolean_rhs() {
        let input = r#"fn f() -> Boolean {
    flag = true
    return flag and 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
        );
    }

    #[test]
    fn test_field_access_type() {
        let input = r#"class Whale {
    name: String
}
fn f(w: Whale) -> String {
    return w.name
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_unknown_member() {
        let input = r#"class Whale {
    has {
        name: String

    }
}
fn f(w: Whale) -> Integer {
    return w.age
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnknownMember { .. }))
        );
    }

    #[test]
    fn test_method_call_checked() {
        let input = r#"class Whale {
    fn swim(distance: Integer) -> Boolean {
        return true

    }
}
fn f(w: Whale) -> Boolean {
    return w.swim(true)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_multi_param_method_call_requires_named_args() {
        let input = r#"class Whale {
    fn swim(distance: Integer, speed: Integer) -> Boolean {
        return true

    }
}
fn f(w: Whale) -> Boolean {
    return w.swim(1, 2)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::NamedArgsRequired { .. }))
        );
    }

    #[test]
    fn test_missing_type_args_on_class_init() {
        let input = r#"class Box[T] {
    has {
        value: T

    }
}
fn f() -> Integer {
    b = Box(value=1)
    return b.value
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MissingTypeArgs { .. }))
        );
    }

    #[test]
    fn test_unexpected_type_args_on_class_init() {
        let input = r#"class Box {
    has {
        value: Integer

    }
}
fn f() -> Integer {
    b = Box[Integer](value=1)
    return b.value
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnexpectedTypeArgs { .. }))
        );
    }

    #[test]
    fn test_interface_missing_method() {
        let input = r#"class Printable {
    must show() -> String

}
class Foo {
    is a Printable
    fn other() -> String {
        return "x"

    }
}
fn f() -> String {
    foo = Foo()
    return foo.other()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::MissingInterfaceMethod { .. }))
        );
    }

    #[test]
    fn test_interface_method_name_overlap() {
        let input = r#"class Printable {
    must render() -> String

}
class Jsonable {
    must render() -> String

}
class Report {
    is a Printable
    name: String
    fn render() -> String {
        return self.name

    }
}
class Blob {
    is a Jsonable
    fn render() -> String {
        return "blob"

    }
}
fn f(p: Printable) -> String {
    return p.render()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_interface_boolean_method_allows_direct_call() {
        let input = r#"class Pred {
    must ready() -> Boolean

}
class Foo {
    is a Pred
    fn ready() -> Boolean {
        return true

    }
}
fn f(p: Pred) -> Boolean {
    return p.ready()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_interface_boolean_method_allows_call_without_legacy_given() {
        let input = r#"class Pred {
    must ready() -> Boolean

}
class Foo {
    is a Pred
    fn ready() -> Boolean {
        return true

    }
}
fn f(p: Pred) -> Boolean {
    return p.ready()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_interface_must_check_requires_checks_impl() {
        let input = r#"class Pred {
    must check ready() -> Boolean

}
class Foo {
    is a Pred
    fn ready() -> Boolean {
        return true
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InterfaceMethodMismatch { .. }))
        );
    }

    #[test]
    fn test_given_call_records_boolean_expr_type() {
        let input = r#"
fn is_positive(value: Integer) -> Boolean {
    return value > 0

}
fn f() -> Boolean {
    return is_positive(3)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");

        let (func_id, func) = module
            .functions
            .iter()
            .find(|(_, func)| func.name.as_str() == "f")
            .expect("missing function f");
        let body = func.body.as_ref().expect("missing function body");
        let call_expr = body
            .exprs
            .iter()
            .find_map(|(id, expr)| match expr {
                Expr::Call { .. } => Some(id.into_raw()),
                _ => None,
            })
            .expect("missing call");
        let fn_info = info
            .function(func_id)
            .expect("missing type info for function");
        assert_eq!(fn_info.expr_types.get(&call_expr), Some(&Type::Boolean));
    }

    #[test]
    fn test_given_call_aliases_normal_call_for_non_check_function() {
        let input = r#"
fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn f() -> Integer {
    return add(a=2, b=3)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");

        let (func_id, func) = module
            .functions
            .iter()
            .find(|(_, func)| func.name.as_str() == "f")
            .expect("missing function f");
        let body = func.body.as_ref().expect("missing function body");
        let call_expr = body
            .exprs
            .iter()
            .find_map(|(id, expr)| match expr {
                Expr::Call { .. } => Some(id.into_raw()),
                _ => None,
            })
            .expect("missing call");
        let fn_info = info
            .function(func_id)
            .expect("missing type info for function");
        assert_eq!(fn_info.expr_types.get(&call_expr), Some(&Type::Integer));
    }

    #[test]
    fn test_match_result_bindings_flow() {
        let input = r#"
fn f() -> Integer {
    match __wr_fs_read_bytes("x") {
        Ok(v): return __wr_bytes_len(v)
        Err(e): return 0
        otherwise: return 2
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_nested_pattern_bindings() {
        let input = r#"
enum Status {
    Pending
    Failed(error: String)

}
fn f(s: Status) -> String {
    match s {
        Status.Failed(e): return e
        Status.Pending: return "ok"
        otherwise: return "bad"
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let (errors, _info) = check_module_with_info(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_function_call_checked() {
        let input = r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn f() -> Integer {
    return add(1, true)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_multi_param_function_call_requires_named_args() {
        let input = r#"fn add(a: Integer, b: Integer) -> Integer {
    return a + b

}
fn f() -> Integer {
    return add(1, 2)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::NamedArgsRequired { .. }))
        );
    }

    #[test]
    fn test_calling_non_callable_errors() {
        let input = r#"fn f() -> Nothing {
    x = 1
    x(2)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidCallee { .. }))
        );
    }

    #[test]
    fn test_method_return_type_flow() {
        let input = r#"class Ocean {
    depth: Integer
}
class Whale {
    fn ocean() -> Ocean {
        return Ocean()

    }
}
fn f(w: Whale) -> Integer {
    return w.ocean().depth
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_equality_allows_structural_class_types() {
        let input = r#"class User {
    has {
        id: Integer

    }
}
fn same(a: User, b: User) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            !errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_rejects_class_with_actor_field() {
        let input = r#"class Worker {
    id: Integer
}
class Job {
    worker: Actor[Worker]
}
fn same(a: Job, b: Job) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_allows_structural_nested_class_types() {
        let input = r#"class User {
    has {
        id: Integer
    }
}
class Wrapper {
    has {
        user: User
    }
}
fn same(a: Wrapper, b: Wrapper) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            !errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_rejects_list_of_non_eq_class() {
        let input = r#"class Worker {
    id: Integer
}
class User {
    worker: Actor[Worker]
}
fn same(a: List[User], b: List[User]) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors.iter().any(|err| matches!(
                err,
                TypeError::EqualityRequiresEq { left, right, .. }
                    if left == "List[User]" && right == "List[User]"
            )),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_allows_structural_enum_types() {
        let input = r#"enum Status {
    Pending
    Done

}
fn same(a: Status, b: Status) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            !errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_equality_rejects_nested_enum_with_pending_payload() {
        let input = r#"class Worker {
    id: Integer
}
enum Status {
    Pending
    Running(task: Pending[Result[Worker]])
}
class Ticket {
    status: Status
}
fn same(a: Ticket, b: Ticket) -> Boolean {
    return a == b
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::EqualityRequiresEq { .. })),
            "{errors:?}"
        );
    }

    #[test]
    fn test_actor_call_requires_await_or_fire() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Nothing {
    w = detach Whale() * 1
    w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::PendingNotAwaited { .. }))
        );
    }

    #[test]
    fn test_error_requires_result_function() {
        let input = r#"fn f() -> Integer {
    error "nope"
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ErrOutsideResult { .. }))
        );
    }

    #[test]
    fn test_try_unwraps_result_in_result_function() {
        let input = r#"fn source() -> Result[Integer] {
    return 1

}
fn f() -> Result[Integer] {
    value = source()?
    return value
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_try_requires_result_returning_function() {
        let input = r#"fn source() -> Result[Integer] {
    return 1

}
fn f() -> Integer {
    return source()?
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::TryOutsideResult { .. }))
        );
    }

    #[test]
    fn test_try_requires_result_operand() {
        let input = r#"fn f() -> Result[Integer] {
    return 1?
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidTryOperand { .. }))
        );
    }

    #[test]
    fn test_try_then_or_else_is_invalid() {
        let input = r#"fn source() -> Result[Integer] {
    return 1

}
fn f() -> Result[Integer] {
    return source()? ?? 0
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidOtherwiseOperand { .. }))
        );
    }

    #[test]
    fn test_result_fallback_handles_result() {
        let input = r#"fn f() -> Result[Integer, RuntimeError] {
    return error "nope" ?? 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_or_else_handles_result() {
        let input = r#"fn f() -> Result[Integer, RuntimeError] {
    return error "nope" ?? 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_invalid_result_fallback_operand() {
        let input = r#"fn f() -> Integer {
    return 1 ?? 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidOtherwiseOperand { .. }))
        );
    }

    #[test]
    fn test_invalid_or_else_operand() {
        let input = r#"fn f() -> Integer {
    return 1 ?? 0
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidOtherwiseOperand { .. }))
        );
    }

    #[test]
    fn test_boundary_list_requires_type_args() {
        let input = r#"fn f(items: List) -> Integer {
    return 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(
            |err| matches!(err, TypeError::BoundaryMissingTypeArgs { name, .. } if name == "List")
        ));
    }

    #[test]
    fn test_boundary_result_requires_type_args() {
        let input = r#"fn f() -> Result {
    return error "nope"
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(
            |err| matches!(err, TypeError::BoundaryMissingTypeArgs { name, .. } if name == "Result")
        ));
    }

    #[test]
    fn test_boundary_pending_requires_type_args() {
        let input = r#"fn f(task: Pending) -> Integer {
    return 1
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(
            |err| matches!(err, TypeError::BoundaryMissingTypeArgs { name, .. } if name == "Pending")
        ));
    }

    #[test]
    fn test_invalid_unary_operand_span() {
        let input = r#"fn f() -> Integer {
    -true
}"#;
        let canonical = input.to_string();
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::InvalidUnaryOperand { .. }))
            .expect("missing invalid unary operand error");
        if let TypeError::InvalidUnaryOperand { span, .. } = err {
            let expected = canonical.rfind('-').unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 1);
        }
    }

    #[test]
    fn test_invalid_binary_operand_span() {
        let input = r#"fn f() -> Integer {
    true + 1
}"#;
        let canonical = input.to_string();
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::InvalidBinaryOperands { .. }))
            .expect("missing invalid binary operands error");
        if let TypeError::InvalidBinaryOperands { span, .. } = err {
            let expected = canonical.find('+').unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 1);
        }
    }

    #[test]
    fn test_unknown_member_span() {
        let input = r#"class Foo {
    has {
        x: Integer

    }
}
fn f() -> Nothing {
    foo = Foo(x=1)
    foo.bar
}
"#;
        let canonical = input.to_string();
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let err = errors
            .iter()
            .find(|err| matches!(err, TypeError::UnknownMember { .. }))
            .expect("missing unknown member error");
        if let TypeError::UnknownMember { span, .. } = err {
            let expected = canonical.find("bar").unwrap();
            assert_eq!(span.offset(), expected);
            assert_eq!(span.len(), 3);
        }
    }

    #[test]
    fn test_actor_call_with_await_ok() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Result[Boolean, Error] {
    w = detach Whale() * 1
    return await w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_builtin_fallible_requires_handling() {
        let input = r#"fn f() -> Nothing {
    __wr_fs_read_bytes("x")
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnhandledResult { .. }))
        );
    }

    #[test]
    fn test_builtin_fallible_or_else_ok() {
        let input = r#"fn f() -> Integer {
    return __wr_bytes_len(__wr_fs_read_bytes("x") ?? __wr_bytes_from_string("1"))
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_external_call_requires_handling() {
        let input = r#"fn f() -> Nothing {
    headers = __wr_map_new()
    __wr_external_call(service="svc", endpoint="ep", method="GET", url="https://example", headers=headers, body="", timeout_ms=10)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnhandledResult { .. }))
        );
    }

    #[test]
    fn test_builtin_external_call_or_else_ok() {
        let input = r#"fn f() -> String {
    headers = __wr_map_new()
    return __wr_external_call(service="svc", endpoint="ep", method="GET", url="https://example", headers=headers, body="", timeout_ms=10) ?? "fallback"
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_builtin_map_new_signature_ok() {
        let input = r#"fn f() -> Nothing {
    m = __wr_map_new()
    __wr_map_set(map=m, key="k", value="v")
    __wr_map_get(map=m, key="k")
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_collection_methods_and_index_typecheck() {
        let input = r#"fn f() -> Integer {
    xs = [1]
    m = {"a": 2}
    xs.push(3)
    m.set(key="b", value=4)
    left = xs[0]
    right = m["a"]
    xs[1] = 5
    m.set(key="b", value=6)
    return left + right + xs.len() + m.len()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_for_with_index_requires_list_or_range() {
        let input = r#"fn f() -> Nothing {
    m = {"k": 1}
    for value in m with index idx {
        nothing
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.iter().any(|err| {
            matches!(
                err,
                TypeError::ForWithIndexRequiresListOrRange { .. }
                    | TypeError::ForMapWithIndexUnsupported { .. }
            )
        }));
    }

    #[test]
    fn test_for_map_binding_requires_map_iterable() {
        let input = r#"fn f() -> Nothing {
    xs = [1]
    for key, value in xs {
        nothing
    }
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ForMapRequiresMap { .. }))
        );
    }

    #[test]
    fn test_index_type_mismatch_reports_error() {
        let input = r#"fn f() -> Integer {
    xs = [1]
    return xs["bad"]
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidIndexType { .. }))
        );
    }

    #[test]
    fn test_builtin_map_new_arg_count_mismatch() {
        let input = r#"fn f() -> Nothing {
    __wr_map_new(1)
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentCountMismatch { .. }))
        );
    }

    #[test]
    fn test_await_on_pending_value_ok() {
        let input = r#"fn f() -> Result[Nothing, Error] {
    return await __wr_sleep_ms(1)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn test_fire_on_pending_value_ok() {
        let input = r#"fn f() -> Nothing {
    fire __wr_sleep_ms(1)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_await_on_non_actor_call_errors() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f(w: Whale) -> Result[Boolean] {
    return await w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAwaitOperand { .. }))
        );
    }

    #[test]
    fn test_fire_actor_call_ok() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Nothing {
    w = detach Whale() * 1
    fire w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_fire_non_actor_call_errors() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f(w: Whale) -> Nothing {
    fire w.swim()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidFireOperand { .. }))
        );
    }

    #[test]
    fn test_class_init_field_type_checked() {
        let input = r#"class Whale {
    name: String
}
fn f() -> Nothing {
    Whale(name=1)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::ArgumentTypeMismatch { .. }))
        );
    }

    #[test]
    fn test_class_init_unknown_field() {
        let input = r#"class Whale {
    has {
        name: String

    }
}
fn f() -> Nothing {
    Whale(age="old")
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::UnknownArgument { .. }))
        );
    }

    #[test]
    fn test_multi_field_class_init_requires_named_args() {
        let input = r#"class Whale {
    name: String
    age: Integer
}
fn f() -> Nothing {
    Whale("orca", 7)
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::NamedArgsRequired { .. }))
        );
    }

    #[test]
    fn test_await_on_actor_value_errors() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Result {
    w = detach Whale() * 1
    return await w
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidAwaitOperand { .. }))
        );
    }

    #[test]
    fn test_fire_on_actor_value_errors() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn f() -> Nothing {
    w = detach Whale() * 1
    fire w
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::InvalidFireOperand { .. }))
        );
    }

    #[test]
    fn test_async_class_requires_actor() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
class Boat {
    fn ride() -> Boolean {
        return await Whale().swim()

    }
}
fn f() -> Nothing {
    Boat()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_method_requires_actor() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
class Boat {
    fn ride() -> Boolean {
        return await Whale().swim()

    }
}
fn f() -> Boolean {
    b = Boat()
    return b.ride()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncMethodRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_chain_requires_actor() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn helper() -> Boolean {
    return await Whale().swim()

}
class Boat {
    fn ride() -> Boolean {
        return helper()

    }
}
fn f() -> Nothing {
    Boat()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_error_includes_chain_hint() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn helper() -> Boolean {
    return await Whale().swim()

}
class Boat {
    fn ride() -> Boolean {
        return helper()

    }
}
fn f() -> Boolean {
    b = Boat()
    return b.ride()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        let mut saw = false;
        for err in &errors {
            if let TypeError::AsyncMethodRequiresActor { help, .. } = err {
                assert!(help.contains("Async call chain:"));
                assert!(help.contains("Boat.ride"));
                assert!(help.contains("helper"));
                saw = true;
                break;
            }
        }
        assert!(saw, "expected AsyncMethodRequiresActor error");
    }

    #[test]
    fn test_fire_chain_requires_actor() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
fn helper() -> Boolean {
    fire Whale().swim()
    return true

}
class Boat {
    fn ride() -> Boolean {
        return helper()

    }
}
fn f() -> Nothing {
    Boat()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_async_class_allowed_with_detach() {
        let input = r#"class Whale {
    fn swim() -> Boolean {
        return true

    }
}
class Boat {
    fn ride() -> Boolean {
        return await Whale().swim()

    }
}
fn f() -> Result {
    b = detach Boat() * 1
    return await b.ride()
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .all(|err| !matches!(err, TypeError::AsyncClassRequiresActor { .. }))
        );
    }

    #[test]
    fn test_deterministic_game_module_rejects_float_literal() {
        let input = r#"node PositionNode profile world {
    x: Integer
}
system tick[stage=fixed, reads=[PositionNode], writes=[PositionNode]]() -> Nothing {
    value = 1.5
    return
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::DeterministicFloatLiteralForbidden { .. }))
        );
    }

    #[test]
    fn test_deterministic_game_module_rejects_float_type_refs() {
        let input = r#"class PositionNode {
    x: Float
}
system tick[stage=fixed, reads=[PositionNode], writes=[PositionNode]]() -> Nothing {
    return
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::DeterministicFloatTypeForbidden { .. }))
        );
    }

    #[test]
    fn test_node_only_module_is_still_deterministic() {
        let input = r#"resource PositionNode {
    x: Float
}
system tick[stage=fixed, reads=[PositionNode], writes=[PositionNode]]() -> Nothing {
    return
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .any(|err| matches!(err, TypeError::DeterministicFloatTypeForbidden { .. }))
        );
    }

    #[test]
    fn test_non_game_module_allows_float_type_and_literals() {
        let input = r#"fn lerp(a: Float, b: Float) -> Float {
    return 1.5
}
"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors
                .iter()
                .all(|err| !matches!(err, TypeError::DeterministicFloatTypeForbidden { .. }))
        );
        assert!(
            errors
                .iter()
                .all(|err| !matches!(err, TypeError::DeterministicFloatLiteralForbidden { .. }))
        );
    }

    #[test]
    fn test_generic_function_type_param_parsed() {
        // A generic function with a type parameter should lower and type-check without errors
        let input = r#"fn identity[T](x: T) -> T {
    return x
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        // Verify type_params were lowered
        let func = module.functions.iter().next().expect("expected a function").1;
        assert_eq!(func.type_params.len(), 1, "Expected 1 type param");
        assert_eq!(func.type_params[0].name, "T");
        let errors = check_module(&module);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_generic_function_multiple_type_params() {
        // A generic function with multiple type parameters
        let input = r#"fn swap[A, B](a: A, b: B) -> A {
    return a
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = module.functions.iter().next().expect("expected a function").1;
        assert_eq!(func.type_params.len(), 2, "Expected 2 type params");
        assert_eq!(func.type_params[0].name, "A");
        assert_eq!(func.type_params[1].name, "B");
        let errors = check_module(&module);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_generic_function_bound_syntax_parses() {
        // A generic function with a type bound should parse, lower, and store the bound
        let input = r#"fn constrained[T: Hashable](x: T) -> T {
    return x
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let func = module.functions.iter().next().expect("expected a function").1;
        assert_eq!(func.type_params.len(), 1, "Expected 1 type param, got {:?}", func.type_params);
        assert_eq!(func.type_params[0].name, "T");
        assert_eq!(func.type_params[0].bounds, vec!["Hashable"], "Expected bound 'Hashable'");
        let errors = check_module(&module);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_non_generic_function_unexpected_type_args() {
        // Passing explicit type args to a non-generic function should produce an error
        let input = r#"fn plain(x: Integer) -> Integer {
    return x
}
fn caller() -> Integer {
    return plain[Integer](1)
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors.iter().any(|err| matches!(err, TypeError::UnexpectedTypeArgs { .. })),
            "Expected UnexpectedTypeArgs error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_type_param_bound_violation() {
        // Calling a generic function with a bound, passing a type that does not satisfy it
        let input = r#"class Foo {
    x: Integer
}
fn bounded[T: Hashable](x: T) -> T {
    return x
}
fn caller() -> Foo {
    return bounded[Foo](Foo(x: 1))
}"#;
        let node = parse(input);
        let root = ast::Root::cast(node).unwrap();
        let module = lower(root);
        let errors = check_module(&module);
        assert!(
            errors.iter().any(|err| matches!(err, TypeError::TypeParamBoundNotSatisfied { .. })),
            "Expected TypeParamBoundNotSatisfied error, got: {:?}",
            errors
        );
    }
}
