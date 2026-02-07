use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `realtime` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn realtime_on_connect(_handler: Value) -> Value {
    unavailable("realtime_on_connect")
}
pub fn realtime_join(_socket_id: Value, _room: Value) -> Value {
    unavailable("realtime_join")
}
pub fn realtime_leave(_socket_id: Value, _room: Value) -> Value {
    unavailable("realtime_leave")
}
pub fn realtime_broadcast(_room: Value, _message: Value) -> Value {
    unavailable("realtime_broadcast")
}
pub fn realtime_send(_socket_id: Value, _message: Value) -> Value {
    unavailable("realtime_send")
}
pub fn realtime_configure(_config: Value) -> Value {
    unavailable("realtime_configure")
}
