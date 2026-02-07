use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `auth` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn auth_create_user(
    _storage: Value,
    _email: Value,
    _username: Value,
    _password: Value,
) -> Value {
    unavailable("auth_create_user")
}
pub fn auth_verify_password(_storage: Value, _user_id: Value, _password: Value) -> Value {
    unavailable("auth_verify_password")
}
pub fn auth_issue_jwt(_storage: Value, _user_id: Value, _claims: Value, _ttl_secs: Value) -> Value {
    unavailable("auth_issue_jwt")
}
pub fn auth_verify_jwt(_storage: Value, _token: Value) -> Value {
    unavailable("auth_verify_jwt")
}
pub fn auth_issue_email_token(_storage: Value, _user_id: Value, _ttl_secs: Value) -> Value {
    unavailable("auth_issue_email_token")
}
pub fn auth_verify_email_token(_storage: Value, _token: Value) -> Value {
    unavailable("auth_verify_email_token")
}
pub fn auth_oauth_login(_storage: Value, _provider: Value, _code: Value) -> Value {
    unavailable("auth_oauth_login")
}
pub fn auth_configure(_config: Value) -> Value {
    unavailable("auth_configure")
}
