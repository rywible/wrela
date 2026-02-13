use crate::db::types::{DbError, MAX_KEY_BYTES};

pub fn validate_namespace(namespace: &[u8]) -> Result<(), DbError> {
    if namespace.is_empty() {
        return Err(DbError::invalid_argument("namespace must not be empty"));
    }
    if namespace.len() > MAX_KEY_BYTES {
        return Err(DbError::limit(format!(
            "namespace too large: {} > {} bytes",
            namespace.len(),
            MAX_KEY_BYTES
        )));
    }
    Ok(())
}

pub fn validate_key(key: &[u8]) -> Result<(), DbError> {
    if key.is_empty() {
        return Err(DbError::invalid_argument("key must not be empty"));
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(DbError::limit(format!(
            "key too large: {} > {} bytes",
            key.len(),
            MAX_KEY_BYTES
        )));
    }
    Ok(())
}

pub fn encode_user_key(namespace: &[u8], key: &[u8]) -> Result<Vec<u8>, DbError> {
    validate_namespace(namespace)?;
    validate_key(key)?;
    let mut out = Vec::with_capacity(2 + namespace.len() + 2 + key.len());
    out.extend_from_slice(&(namespace.len() as u16).to_be_bytes());
    out.extend_from_slice(namespace);
    out.extend_from_slice(&(key.len() as u16).to_be_bytes());
    out.extend_from_slice(key);
    Ok(out)
}

pub fn decode_user_key(input: &[u8]) -> Result<(Vec<u8>, Vec<u8>), DbError> {
    if input.len() < 4 {
        return Err(DbError::invalid_argument("encoded key too short"));
    }
    let ns_len = u16::from_be_bytes([input[0], input[1]]) as usize;
    let ns_start = 2;
    let ns_end = ns_start + ns_len;
    if ns_end + 2 > input.len() {
        return Err(DbError::invalid_argument(
            "encoded key has invalid namespace length",
        ));
    }
    let key_len = u16::from_be_bytes([input[ns_end], input[ns_end + 1]]) as usize;
    let key_start = ns_end + 2;
    let key_end = key_start + key_len;
    if key_end != input.len() {
        return Err(DbError::invalid_argument(
            "encoded key trailing bytes mismatch",
        ));
    }
    Ok((
        input[ns_start..ns_end].to_vec(),
        input[key_start..key_end].to_vec(),
    ))
}
