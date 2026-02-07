use crate::result;
use crate::string;
use crate::value::Value;

fn unavailable(op: &str) -> Value {
    let message = format!("runtime module `search` is unavailable: {op}");
    let err = string::str_from_utf8(message.as_ptr(), message.len());
    result::result_err(err)
}

pub fn search_index(
    _storage: Value,
    _collection: Value,
    _id: Value,
    _text: Value,
    _fields: Value,
) -> Value {
    unavailable("search_index")
}
pub fn search_remove(_storage: Value, _collection: Value, _id: Value) -> Value {
    unavailable("search_remove")
}
pub fn search_query(_storage: Value, _collection: Value, _query: Value, _opts: Value) -> Value {
    unavailable("search_query")
}
