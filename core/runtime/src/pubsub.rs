use crate::result;
use crate::string;
use crate::value::Value;

pub fn pubsub_configure(_config: Value) -> Value {
    let message = "runtime module `pubsub` is unavailable: pubsub_configure";
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}
