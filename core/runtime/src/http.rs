use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `http` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn register_class(_name_ptr: *const u8, _len: usize, _class_id: u32) {}
pub fn register_method_name(_name_ptr: *const u8, _len: usize, _class_id: u32, _method_id: u32) {}

pub fn serve_get_requests(_path: Value, _handler: Value) -> Value {
    unavailable("serve_get_requests")
}
pub fn serve_post_requests(_path: Value, _handler: Value) -> Value {
    unavailable("serve_post_requests")
}
pub fn serve_requests(_method: Value, _path: Value, _handler: Value) -> Value {
    unavailable("serve_requests")
}
pub fn http_server_configure(_config: Value) -> Value {
    unavailable("http_server_configure")
}
pub fn serve_on(_addr: Value) -> Value {
    unavailable("serve_on")
}
pub fn stop() -> Value {
    unavailable("stop")
}
