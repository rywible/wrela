use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `schedule` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn schedule_cron(_storage: Value, _expr: Value, _job: Value) -> Value {
    unavailable("schedule_cron")
}
pub fn schedule_every(_storage: Value, _seconds: Value, _job: Value) -> Value {
    unavailable("schedule_every")
}
pub fn schedule_at(_storage: Value, _timestamp: Value, _job: Value) -> Value {
    unavailable("schedule_at")
}
