use crate::result;
use crate::string;
use crate::value::Value;

pub fn admin_enable(_opts: Value) -> Value {
    let message = "runtime module `admin` is unavailable: admin_enable";
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}
