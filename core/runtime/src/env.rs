use crate::string;
use crate::value::Value;

fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

pub fn env_get(key: Value) -> Value {
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::nil(),
    };
    match std::env::var(&key).ok() {
        Some(val) => string::str_from_bytes(val.as_bytes()),
        None => Value::nil(),
    }
}

pub fn env_set(key: Value, val: Value) -> Value {
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::from_bool(false),
    };
    let val = match value_to_string(val) {
        Some(val) => val,
        None => return Value::from_bool(false),
    };
    unsafe {
        std::env::set_var(key, val);
    }
    Value::from_bool(true)
}
