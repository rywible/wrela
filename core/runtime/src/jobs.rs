use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `jobs` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn jobs_enqueue(_storage: Value, _queue: Value, _payload: Value, _opts: Value) -> Value {
    unavailable("jobs_enqueue")
}
pub fn jobs_process(_storage: Value, _queue: Value, _handler: Value) -> Value {
    unavailable("jobs_process")
}
pub fn jobs_dead_letter(_storage: Value, _queue: Value) -> Value {
    unavailable("jobs_dead_letter")
}
pub fn jobs_configure(_config: Value) -> Value {
    unavailable("jobs_configure")
}
