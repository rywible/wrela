use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `rbac` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn rbac_create_role(
    _storage: Value,
    _scope: Value,
    _name: Value,
    _permissions: Value,
) -> Value {
    unavailable("rbac_create_role")
}
pub fn rbac_assign_role(
    _storage: Value,
    _user_id: Value,
    _role_id: Value,
    _scope_id: Value,
) -> Value {
    unavailable("rbac_assign_role")
}
pub fn rbac_check(_storage: Value, _user_id: Value, _permission: Value, _scope_id: Value) -> Value {
    unavailable("rbac_check")
}
pub fn rbac_permissions_for(_storage: Value, _user_id: Value, _scope_id: Value) -> Value {
    unavailable("rbac_permissions_for")
}
