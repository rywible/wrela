use crate::string;
use crate::value::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

struct EnvState {
    dotenv: HashMap<String, String>,
}

static STATE: OnceLock<Mutex<EnvState>> = OnceLock::new();
static INIT: OnceLock<()> = OnceLock::new();

fn state() -> &'static Mutex<EnvState> {
    STATE.get_or_init(|| {
        Mutex::new(EnvState {
            dotenv: HashMap::new(),
        })
    })
}

pub fn init() {
    if INIT.get().is_some() {
        return;
    }
    let _ = INIT.set(());
    let _ = load_env_path(".env");
}

fn parse_dotenv(contents: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for raw in contents.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("export ") {
            line = rest.trim();
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let mut value = value.trim().to_string();
        if value.len() >= 2 {
            let bytes = value.as_bytes();
            if (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            {
                value = value[1..value.len() - 1].to_string();
            }
        }
        map.insert(key.to_string(), value);
    }
    map
}

fn load_env_path(path: &str) -> Result<(), std::io::Error> {
    let contents = std::fs::read_to_string(path)?;
    let map = parse_dotenv(&contents);
    let mut guard = state().lock().expect("env state lock");
    guard.dotenv = map;
    Ok(())
}

fn value_to_string(val: Value) -> Option<String> {
    string::with_string_bytes(val, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn lookup(key: &str) -> Option<String> {
    if let Ok(val) = std::env::var(key) {
        return Some(val);
    }
    let guard = state().lock().expect("env state lock");
    guard.dotenv.get(key).cloned()
}

fn parse_bool(input: &str) -> Option<bool> {
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" => Some(true),
        "0" | "false" | "off" => Some(false),
        _ => None,
    }
}

pub fn env_get(key: Value) -> Value {
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::nil(),
    };
    match lookup(&key) {
        Some(val) => string::str_from_bytes(val.as_bytes()),
        None => Value::nil(),
    }
}

pub fn env_get_or(key: Value, default: Value) -> Value {
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::nil(),
    };
    if let Some(val) = lookup(&key) {
        return string::str_from_bytes(val.as_bytes());
    }
    match value_to_string(default) {
        Some(val) => string::str_from_bytes(val.as_bytes()),
        None => Value::nil(),
    }
}

pub fn env_get_as_bool(key: Value) -> Value {
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::nil(),
    };
    match lookup(&key).and_then(|val| parse_bool(&val)) {
        Some(flag) => Value::from_bool(flag),
        None => Value::nil(),
    }
}

pub fn env_get_as_int(key: Value) -> Value {
    let key = match value_to_string(key) {
        Some(key) => key,
        None => return Value::nil(),
    };
    match lookup(&key).and_then(|val| val.trim().parse::<i64>().ok()) {
        Some(num) => Value::from_int(num),
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

pub fn env_load(path: Value) -> Value {
    let path = match value_to_string(path) {
        Some(path) => path,
        None => return Value::from_bool(false),
    };
    Value::from_bool(load_env_path(&path).is_ok())
}
