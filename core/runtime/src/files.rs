use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `files` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn files_upload_stream(_storage: Value, _stream: Value, _opts: Value) -> Value {
    unavailable("files_upload_stream")
}
pub fn files_signed_url(_storage: Value, _file_id: Value, _opts: Value) -> Value {
    unavailable("files_signed_url")
}
pub fn files_metadata(_storage: Value, _file_id: Value) -> Value {
    unavailable("files_metadata")
}
pub fn files_delete(_storage: Value, _file_id: Value) -> Value {
    unavailable("files_delete")
}
pub fn files_set_acl(_storage: Value, _file_id: Value, _acl: Value) -> Value {
    unavailable("files_set_acl")
}
