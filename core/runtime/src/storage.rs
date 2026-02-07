use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `storage` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn storage_get(_key: Value) -> Value {
    unavailable("storage_get")
}
pub fn storage_get_with_version(_key: Value) -> Value {
    unavailable("storage_get_with_version")
}
pub fn storage_scan(_start: Value, _end: Value, _limit: Value) -> Value {
    unavailable("storage_scan")
}
pub fn storage_list_prefix(_prefix: Value, _limit: Value) -> Value {
    unavailable("storage_list_prefix")
}
pub fn storage_configure(_config: Value) -> Value {
    unavailable("storage_configure")
}
pub fn storage_set(_key: Value, _value: Value) -> Value {
    unavailable("storage_set")
}
pub fn storage_set_if_version(_key: Value, _value: Value, _version: Value) -> Value {
    unavailable("storage_set_if_version")
}
pub fn storage_delete_if_version(_key: Value, _version: Value) -> Value {
    unavailable("storage_delete_if_version")
}
pub fn storage_delete(_key: Value) -> Value {
    unavailable("storage_delete")
}
pub fn storage_batch_set(_items: Value) -> Value {
    unavailable("storage_batch_set")
}
