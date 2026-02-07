use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `rate_limit` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn rate_check(_storage: Value, _key: Value, _opts: Value) -> Value {
    unavailable("rate_check")
}
pub fn rate_ip(_request: Value) -> Value {
    unavailable("rate_ip")
}
