use crate::result;
use crate::string;
use crate::value::Value;
use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::sha2::Sha256;
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

struct AuthRegistry {
    keys: BTreeMap<String, AuthKeyMaterial>,
    active_key_id: String,
}

struct AuthKeyMaterial {
    private_key: RsaPrivateKey,
    public_key: RsaPublicKey,
}

#[derive(Debug, Deserialize)]
struct JwtHeader {
    alg: String,
    kid: String,
}

#[derive(Debug, Serialize)]
struct JwtHeaderOut<'a> {
    alg: &'a str,
    typ: &'a str,
    kid: &'a str,
}

fn auth_registry() -> &'static Mutex<AuthRegistry> {
    static REGISTRY: OnceLock<Mutex<AuthRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(AuthRegistry {
            keys: BTreeMap::new(),
            active_key_id: std::env::var("WRELA_AUTH_ACTIVE_KEY_ID")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "wrela-default-key".to_string()),
        })
    })
}

fn err(message: &str) -> Value {
    let error = string::str_from_utf8(message.as_ptr(), message.len());
    let out = result::result_err(error);
    unsafe {
        crate::wr_rc_dec(error);
    }
    out
}

fn ok(value: Value) -> Value {
    result::result_ok(value)
}

fn str_value(text: &str) -> Value {
    string::str_from_utf8(text.as_ptr(), text.len())
}

fn value_to_string(value: Value) -> Option<String> {
    string::with_string_bytes(value, |bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn normalize_key_id(input_key_id: &str, active_key_id: &str) -> String {
    let trimmed = input_key_id.trim();
    if trimmed.is_empty() {
        active_key_id.to_string()
    } else {
        trimmed.to_string()
    }
}

fn ensure_key_locked<'a>(
    registry: &'a mut AuthRegistry,
    key_id: &str,
) -> Result<&'a AuthKeyMaterial, String> {
    if !registry.keys.contains_key(key_id) {
        let mut rng = OsRng;
        let private_key = RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|error| format!("failed to generate RSA key '{}': {error}", key_id))?;
        let public_key = RsaPublicKey::from(&private_key);
        registry.keys.insert(
            key_id.to_string(),
            AuthKeyMaterial {
                private_key,
                public_key,
            },
        );
    }

    registry
        .keys
        .get(key_id)
        .ok_or_else(|| format!("missing key material for key id '{}'", key_id))
}

fn resolve_key_locked<'a>(
    registry: &'a AuthRegistry,
    key_id: &str,
) -> Result<&'a AuthKeyMaterial, String> {
    registry
        .keys
        .get(key_id)
        .ok_or_else(|| format!("auth_verify_jwt unknown key id '{}'", key_id))
}

fn current_unix_timestamp_seconds() -> Result<i64, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("auth_verify_jwt failed to read system clock: {error}"))?;
    Ok(now.as_secs() as i64)
}

fn validate_registered_time_claims(payload: &JsonValue) -> Result<(), String> {
    let JsonValue::Object(payload_map) = payload else {
        return Err("auth_verify_jwt payload must be a JSON object".to_string());
    };

    let now_seconds = current_unix_timestamp_seconds()?;

    if let Some(expiration_value) = payload_map.get("exp")
        && let Some(expiration_seconds) = expiration_value.as_i64()
        && now_seconds >= expiration_seconds
    {
        return Err("auth_verify_jwt token expired (exp claim)".to_string());
    }

    if let Some(not_before_value) = payload_map.get("nbf")
        && let Some(not_before_seconds) = not_before_value.as_i64()
        && now_seconds < not_before_seconds
    {
        return Err("auth_verify_jwt token is not yet valid (nbf claim)".to_string());
    }

    Ok(())
}

pub(crate) fn auth_hash_password(password: Value) -> Value {
    let Some(password) = value_to_string(password) else {
        return err("auth_hash_password expects String password");
    };

    let mut rng = OsRng;
    let salt = SaltString::generate(&mut rng);
    let argon2 = Argon2::default();
    match argon2.hash_password(password.as_bytes(), &salt) {
        Ok(hash) => ok(str_value(hash.to_string().as_str())),
        Err(error) => err(&format!("auth_hash_password failed: {error}")),
    }
}

pub(crate) fn auth_verify_password_hash(password: Value, hashed_password: Value) -> Value {
    let Some(password) = value_to_string(password) else {
        return err("auth_verify_password_hash expects String password");
    };
    let Some(hashed_password) = value_to_string(hashed_password) else {
        return err("auth_verify_password_hash expects String hashed_password");
    };

    let parsed_hash = match PasswordHash::new(hashed_password.as_str()) {
        Ok(parsed_hash) => parsed_hash,
        Err(error) => {
            return err(&format!(
                "auth_verify_password_hash failed to parse password hash: {error}"
            ));
        }
    };

    let verified = Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok();
    ok(Value::from_bool(verified))
}

pub(crate) fn auth_sign_jwt(claims_json: Value, key_id: Value) -> Value {
    let Some(claims_json) = value_to_string(claims_json) else {
        return err("auth_sign_jwt expects String claims_json");
    };
    let Some(input_key_id) = value_to_string(key_id) else {
        return err("auth_sign_jwt expects String key_id");
    };

    let claims = match serde_json::from_str::<JsonValue>(claims_json.as_str()) {
        Ok(claims @ JsonValue::Object(_)) => claims,
        Ok(_) => return err("auth_sign_jwt expects claims_json to be a JSON object"),
        Err(error) => {
            return err(&format!(
                "auth_sign_jwt failed to parse claims_json: {error}"
            ));
        }
    };

    let mut registry = auth_registry().lock().expect("auth registry lock");
    let key_id = normalize_key_id(&input_key_id, registry.active_key_id.as_str());
    let key_material = match ensure_key_locked(&mut registry, key_id.as_str()) {
        Ok(key_material) => key_material,
        Err(message) => return err(&message),
    };

    let header = JwtHeaderOut {
        alg: "RS256",
        typ: "JWT",
        kid: key_id.as_str(),
    };

    let header_json = match serde_json::to_vec(&header) {
        Ok(header_json) => header_json,
        Err(error) => return err(&format!("auth_sign_jwt failed to encode header: {error}")),
    };
    let payload_json = match serde_json::to_vec(&claims) {
        Ok(payload_json) => payload_json,
        Err(error) => return err(&format!("auth_sign_jwt failed to encode payload: {error}")),
    };

    let header_segment = URL_SAFE_NO_PAD.encode(header_json);
    let payload_segment = URL_SAFE_NO_PAD.encode(payload_json);
    let signing_input = format!("{header_segment}.{payload_segment}");

    let signing_key = SigningKey::<Sha256>::new(key_material.private_key.clone());
    let signature = signing_key.sign(signing_input.as_bytes());
    let signature_segment = URL_SAFE_NO_PAD.encode(signature.to_vec());

    let token = format!("{header_segment}.{payload_segment}.{signature_segment}");
    ok(str_value(token.as_str()))
}

pub(crate) fn auth_verify_jwt(token: Value) -> Value {
    let Some(token) = value_to_string(token) else {
        return err("auth_verify_jwt expects String token");
    };

    let mut segments = token.split('.');
    let Some(header_segment) = segments.next() else {
        return err("auth_verify_jwt token is malformed");
    };
    let Some(payload_segment) = segments.next() else {
        return err("auth_verify_jwt token is malformed");
    };
    let Some(signature_segment) = segments.next() else {
        return err("auth_verify_jwt token is malformed");
    };
    if segments.next().is_some() {
        return err("auth_verify_jwt token is malformed");
    }

    let header_bytes = match URL_SAFE_NO_PAD.decode(header_segment.as_bytes()) {
        Ok(header_bytes) => header_bytes,
        Err(error) => {
            return err(&format!(
                "auth_verify_jwt failed to decode token header segment: {error}"
            ));
        }
    };
    let payload_bytes = match URL_SAFE_NO_PAD.decode(payload_segment.as_bytes()) {
        Ok(payload_bytes) => payload_bytes,
        Err(error) => {
            return err(&format!(
                "auth_verify_jwt failed to decode token payload segment: {error}"
            ));
        }
    };
    let signature_bytes = match URL_SAFE_NO_PAD.decode(signature_segment.as_bytes()) {
        Ok(signature_bytes) => signature_bytes,
        Err(error) => {
            return err(&format!(
                "auth_verify_jwt failed to decode token signature segment: {error}"
            ));
        }
    };

    let header = match serde_json::from_slice::<JwtHeader>(&header_bytes) {
        Ok(header) => header,
        Err(error) => return err(&format!("auth_verify_jwt failed to parse header: {error}")),
    };
    if header.alg != "RS256" {
        return err("auth_verify_jwt expects token algorithm RS256");
    }

    let registry = auth_registry().lock().expect("auth registry lock");
    let key_material = match resolve_key_locked(&registry, header.kid.as_str()) {
        Ok(key_material) => key_material,
        Err(message) => return err(&message),
    };

    let signature = match Signature::try_from(signature_bytes.as_slice()) {
        Ok(signature) => signature,
        Err(error) => {
            return err(&format!(
                "auth_verify_jwt failed to parse signature: {error}"
            ));
        }
    };

    let signing_input = format!("{header_segment}.{payload_segment}");
    let verifying_key = VerifyingKey::<Sha256>::new(key_material.public_key.clone());
    if let Err(error) = verifying_key.verify(signing_input.as_bytes(), &signature) {
        return err(&format!(
            "auth_verify_jwt signature verification failed: {error}"
        ));
    }

    let payload_text = match String::from_utf8(payload_bytes) {
        Ok(payload_text) => payload_text,
        Err(_) => return err("auth_verify_jwt payload is not valid UTF-8"),
    };

    let payload_json = match serde_json::from_str::<JsonValue>(payload_text.as_str()) {
        Ok(payload_json) => payload_json,
        Err(error) => {
            return err(&format!(
                "auth_verify_jwt payload is not valid JSON: {error}"
            ));
        }
    };

    if let Err(message) = validate_registered_time_claims(&payload_json) {
        return err(&message);
    }

    ok(str_value(payload_text.as_str()))
}

pub(crate) fn auth_generate_secure_token(byte_length: Value) -> Value {
    let byte_length = crate::value::int_value(byte_length).unwrap_or(0);
    if byte_length <= 0 {
        return err("auth_generate_secure_token expects byte_length > 0");
    }
    if byte_length > 4096 {
        return err("auth_generate_secure_token byte_length must be <= 4096");
    }

    let mut bytes = vec![0u8; byte_length as usize];
    rand::rngs::OsRng.fill_bytes(bytes.as_mut_slice());
    let token = URL_SAFE_NO_PAD.encode(bytes);
    ok(str_value(token.as_str()))
}

pub(crate) fn auth_render_jwks_document() -> Value {
    let mut registry = auth_registry().lock().expect("auth registry lock");
    let active_key_id = registry.active_key_id.clone();
    if let Err(message) = ensure_key_locked(&mut registry, active_key_id.as_str()) {
        return err(&message);
    }

    let keys = registry
        .keys
        .iter()
        .map(|(key_id, key_material)| {
            serde_json::json!({
                "kty": "RSA",
                "use": "sig",
                "alg": "RS256",
                "kid": key_id,
                "n": URL_SAFE_NO_PAD.encode(key_material.public_key.n().to_bytes_be()),
                "e": URL_SAFE_NO_PAD.encode(key_material.public_key.e().to_bytes_be()),
            })
        })
        .collect::<Vec<_>>();

    let payload = serde_json::json!({ "keys": keys });
    match serde_json::to_string(&payload) {
        Ok(payload) => ok(str_value(payload.as_str())),
        Err(error) => err(&format!("auth_render_jwks_document failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_password_round_trip_succeeds() {
        let password = str_value("top-secret-password");
        let hashed_result = auth_hash_password(password);
        let hashed_ok = result::result_is_ok(hashed_result);
        assert!(hashed_ok.is_bool() && hashed_ok.as_bool());

        let hashed_password = result::result_unwrap(hashed_result);
        let verified_result = auth_verify_password_hash(password, hashed_password);
        let verified_ok = result::result_is_ok(verified_result);
        assert!(verified_ok.is_bool() && verified_ok.as_bool());
        let verified_value = result::result_unwrap(verified_result);
        assert!(verified_value.is_bool() && verified_value.as_bool());

        unsafe {
            crate::wr_rc_dec(password);
            crate::wr_rc_dec(hashed_ok);
            crate::wr_rc_dec(hashed_password);
            crate::wr_rc_dec(hashed_result);
            crate::wr_rc_dec(verified_ok);
            crate::wr_rc_dec(verified_value);
            crate::wr_rc_dec(verified_result);
        }
    }

    #[test]
    fn sign_and_verify_jwt_round_trip_succeeds() {
        let claims = str_value("{\"sub\":\"user-123\",\"scope\":\"api:read\"}");
        let key_id = str_value("integration-test-key");

        let token_result = auth_sign_jwt(claims, key_id);
        let token_ok = result::result_is_ok(token_result);
        assert!(token_ok.is_bool() && token_ok.as_bool());
        let token = result::result_unwrap(token_result);

        let verified_result = auth_verify_jwt(token);
        let verified_ok = result::result_is_ok(verified_result);
        assert!(verified_ok.is_bool() && verified_ok.as_bool());
        let verified_claims = result::result_unwrap(verified_result);
        let verified_text = value_to_string(verified_claims).expect("claims text");
        assert!(verified_text.contains("sub"));
        assert!(verified_text.contains("user-123"));

        unsafe {
            crate::wr_rc_dec(claims);
            crate::wr_rc_dec(key_id);
            crate::wr_rc_dec(token_ok);
            crate::wr_rc_dec(token);
            crate::wr_rc_dec(token_result);
            crate::wr_rc_dec(verified_ok);
            crate::wr_rc_dec(verified_claims);
            crate::wr_rc_dec(verified_result);
        }
    }

    #[test]
    fn render_jwks_document_contains_active_rsa_key() {
        let jwks_result = auth_render_jwks_document();
        let jwks_ok = result::result_is_ok(jwks_result);
        assert!(jwks_ok.is_bool() && jwks_ok.as_bool());

        let jwks_text_value = result::result_unwrap(jwks_result);
        let jwks_text = value_to_string(jwks_text_value).expect("jwks text");
        let parsed = serde_json::from_str::<JsonValue>(&jwks_text).expect("valid jwks json");
        let keys = parsed
            .get("keys")
            .and_then(JsonValue::as_array)
            .expect("jwks keys array");
        assert!(!keys.is_empty(), "jwks keys should not be empty");
        let first_key = keys[0].as_object().expect("jwks key object");
        assert_eq!(
            first_key
                .get("kty")
                .and_then(JsonValue::as_str)
                .expect("kty string"),
            "RSA"
        );
        assert_eq!(
            first_key
                .get("alg")
                .and_then(JsonValue::as_str)
                .expect("alg string"),
            "RS256"
        );

        unsafe {
            crate::wr_rc_dec(jwks_ok);
            crate::wr_rc_dec(jwks_text_value);
            crate::wr_rc_dec(jwks_result);
        }
    }

    #[test]
    fn secure_token_generation_respects_byte_length_boundaries() {
        let invalid_zero = auth_generate_secure_token(Value::from_int(0));
        let invalid_zero_ok = result::result_is_ok(invalid_zero);
        assert!(invalid_zero_ok.is_bool() && !invalid_zero_ok.as_bool());

        let valid = auth_generate_secure_token(Value::from_int(32));
        let valid_ok = result::result_is_ok(valid);
        assert!(valid_ok.is_bool() && valid_ok.as_bool());
        let token_value = result::result_unwrap(valid);
        let token_text = value_to_string(token_value).expect("token string");
        assert!(
            token_text.len() >= 43,
            "expected URL-safe token to be long enough for 32 bytes of entropy"
        );

        unsafe {
            crate::wr_rc_dec(invalid_zero_ok);
            crate::wr_rc_dec(invalid_zero);
            crate::wr_rc_dec(valid_ok);
            crate::wr_rc_dec(token_value);
            crate::wr_rc_dec(valid);
        }
    }

    #[test]
    fn verify_jwt_rejects_expired_exp_claim() {
        let now_seconds = current_unix_timestamp_seconds().expect("clock");
        let claims =
            str_value(format!("{{\"sub\":\"user-123\",\"exp\":{}}}", now_seconds - 1).as_str());
        let key_id = str_value("expired-token-test-key");

        let token_result = auth_sign_jwt(claims, key_id);
        let token_ok = result::result_is_ok(token_result);
        assert!(token_ok.is_bool() && token_ok.as_bool());
        let token = result::result_unwrap(token_result);

        let verify_result = auth_verify_jwt(token);
        let verify_ok = result::result_is_ok(verify_result);
        assert!(verify_ok.is_bool());
        assert!(!verify_ok.as_bool());

        unsafe {
            crate::wr_rc_dec(claims);
            crate::wr_rc_dec(key_id);
            crate::wr_rc_dec(token_ok);
            crate::wr_rc_dec(token);
            crate::wr_rc_dec(token_result);
            crate::wr_rc_dec(verify_ok);
            crate::wr_rc_dec(verify_result);
        }
    }

    #[test]
    fn verify_jwt_rejects_unknown_key_identifier() {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT","kid":"unknown-key"}"#);
        let payload = URL_SAFE_NO_PAD.encode(br#"{"sub":"user-123"}"#);
        let signature = URL_SAFE_NO_PAD.encode([0u8; 256]);
        let token = str_value(format!("{header}.{payload}.{signature}").as_str());

        let verify_result = auth_verify_jwt(token);
        let verify_ok = result::result_is_ok(verify_result);
        assert!(verify_ok.is_bool());
        assert!(!verify_ok.as_bool());

        unsafe {
            crate::wr_rc_dec(token);
            crate::wr_rc_dec(verify_ok);
            crate::wr_rc_dec(verify_result);
        }
    }
}
