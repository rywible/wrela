use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::class::class_get;
use crate::map;
use crate::storage_helpers::{
    storage_delete, storage_get_json, storage_get_string, storage_set_json, storage_set_string,
    value_to_string,
};
use crate::string;
use crate::value::{Value, int_value};
use crate::wr_rc_dec;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use password_hash::SaltString;
use password_hash::rand_core::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue, json};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Clone)]
struct OAuthConfig {
    client_id: String,
    client_secret: String,
    redirect_uri: Option<String>,
}

#[derive(Clone)]
struct AuthConfig {
    jwt_secret: String,
    github: Option<OAuthConfig>,
    google: Option<OAuthConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: "wrela-dev-secret".to_string(),
            github: None,
            google: None,
        }
    }
}

static AUTH_CONFIG: OnceLock<Mutex<AuthConfig>> = OnceLock::new();

fn auth_config() -> AuthConfig {
    AUTH_CONFIG
        .get_or_init(|| Mutex::new(AuthConfig::default()))
        .lock()
        .expect("auth config lock")
        .clone()
}

fn set_auth_config(config: AuthConfig) {
    *AUTH_CONFIG
        .get_or_init(|| Mutex::new(AuthConfig::default()))
        .lock()
        .expect("auth config lock") = config;
}

#[derive(Clone, Serialize, Deserialize)]
struct UserRecord {
    id: String,
    email: String,
    username: String,
    pw_hash: String,
    verified: bool,
    created_at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct EmailToken {
    user_id: String,
    exp: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn map_set_string(map_val: Value, key: &str, value: &str) {
    let key_val = string::str_from_bytes(key.as_bytes());
    let val = string::str_from_bytes(value.as_bytes());
    map::map_set(map_val, key_val, val);
    unsafe {
        wr_rc_dec(key_val);
        wr_rc_dec(val);
    }
}

fn map_set_int(map_val: Value, key: &str, value: i64) {
    let key_val = string::str_from_bytes(key.as_bytes());
    map::map_set(map_val, key_val, Value::from_int(value));
    unsafe { wr_rc_dec(key_val) };
}

fn map_set_bool(map_val: Value, key: &str, value: bool) {
    let key_val = string::str_from_bytes(key.as_bytes());
    map::map_set(map_val, key_val, Value::from_bool(value));
    unsafe { wr_rc_dec(key_val) };
}

fn user_to_map(user: &UserRecord) -> Value {
    let map_val = map::map_new();
    map_set_string(map_val, "id", &user.id);
    map_set_string(map_val, "email", &user.email);
    map_set_string(map_val, "username", &user.username);
    map_set_bool(map_val, "verified", user.verified);
    map_set_int(map_val, "created_at", user.created_at as i64);
    map_val
}

fn json_from_map(val: Value) -> Option<JsonValue> {
    let map_ptr = map::as_map_ref(val)?;
    let mut out = JsonMap::new();
    unsafe {
        let mut iter = map::map_iter(map_ptr);
        while let Some((key, value)) = iter.next() {
            let Some(key_str) = value_to_string(key.0) else {
                continue;
            };
            if value.is_nil() {
                out.insert(key_str, JsonValue::Null);
                continue;
            }
            if value.is_bool() {
                out.insert(key_str, JsonValue::Bool(value.as_bool()));
                continue;
            }
            if let Some(i) = int_value(value) {
                out.insert(key_str, JsonValue::Number(i.into()));
                continue;
            }
            if value.is_float() {
                if let Some(num) = serde_json::Number::from_f64(value.as_float()) {
                    out.insert(key_str, JsonValue::Number(num));
                }
                continue;
            }
            if let Some(s) = value_to_string(value) {
                out.insert(key_str, JsonValue::String(s));
                continue;
            }
        }
    }
    Some(JsonValue::Object(out))
}

fn map_from_json(value: &JsonValue) -> Value {
    let map_val = map::map_new();
    let JsonValue::Object(obj) = value else {
        return map_val;
    };
    for (key, val) in obj {
        let key_val = string::str_from_bytes(key.as_bytes());
        let out_val = match val {
            JsonValue::Null => Value::nil(),
            JsonValue::Bool(b) => Value::from_bool(*b),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::from_int(i)
                } else if let Some(f) = n.as_f64() {
                    Value::from_float(f)
                } else {
                    Value::nil()
                }
            }
            JsonValue::String(s) => string::str_from_bytes(s.as_bytes()),
            JsonValue::Array(_) | JsonValue::Object(_) => Value::nil(),
        };
        map::map_set(map_val, key_val, out_val);
        unsafe {
            wr_rc_dec(key_val);
            if out_val.is_ptr() {
                wr_rc_dec(out_val);
            }
        }
    }
    map_val
}

fn hash_password(password: &str) -> Option<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    argon
        .hash_password(password.as_bytes(), &salt)
        .ok()
        .map(|hash| hash.to_string())
}

fn verify_password_hash(hash: &str, password: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn jwt_secret() -> String {
    auth_config().jwt_secret
}

fn auth_config_from_value(config: Value) -> AuthConfig {
    let jwt_secret = config_field_string(config, "jwt_secret")
        .unwrap_or_else(|| "wrela-dev-secret".to_string());
    let github_client_id = config_field_string(config, "github_client_id");
    let github_client_secret = config_field_string(config, "github_client_secret");
    let github_redirect_uri = config_field_string(config, "github_redirect_uri");
    let google_client_id = config_field_string(config, "google_client_id");
    let google_client_secret = config_field_string(config, "google_client_secret");
    let google_redirect_uri = config_field_string(config, "google_redirect_uri");

    let github = match (github_client_id, github_client_secret) {
        (Some(client_id), Some(client_secret)) => Some(OAuthConfig {
            client_id,
            client_secret,
            redirect_uri: github_redirect_uri,
        }),
        _ => None,
    };
    let google = match (google_client_id, google_client_secret) {
        (Some(client_id), Some(client_secret)) => Some(OAuthConfig {
            client_id,
            client_secret,
            redirect_uri: google_redirect_uri,
        }),
        _ => None,
    };

    AuthConfig {
        jwt_secret,
        github,
        google,
    }
}

fn config_field_string(config: Value, field: &str) -> Option<String> {
    let val = class_get(config, field.as_ptr(), field.len());
    if val.is_nil() {
        unsafe { wr_rc_dec(val) };
        return None;
    }
    let out = value_to_string(val).and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    unsafe { wr_rc_dec(val) };
    out
}

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    sub: String,
    exp: usize,
    iat: usize,
    jti: String,
    #[serde(flatten)]
    extra: HashMap<String, JsonValue>,
}

pub fn auth_create_user(storage: Value, email: Value, username: Value, password: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let email = match value_to_string(email) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let username = match value_to_string(username) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let password = match value_to_string(password) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    runtime_spawn(async move {
        let email_key = format!("auth:email:{email}");
        if storage_get_string(&email_key).await.is_some() {
            resolve_pending(state, Value::nil());
            return;
        }
        let Some(hash) = hash_password(&password) else {
            resolve_pending(state, Value::nil());
            return;
        };
        let id = Uuid::new_v4().to_string();
        let user = UserRecord {
            id: id.clone(),
            email: email.clone(),
            username,
            pw_hash: hash,
            verified: false,
            created_at: now_secs(),
        };
        let user_key = format!("auth:user:{id}");
        let stored =
            storage_set_json(&user_key, &user).await && storage_set_string(&email_key, &id).await;
        if !stored {
            resolve_pending(state, Value::nil());
            return;
        }
        resolve_pending(state, user_to_map(&user));
    });
    pending
}

pub fn auth_configure(config: Value) -> Value {
    let new_config = auth_config_from_value(config);
    set_auth_config(new_config);
    Value::nil()
}

pub fn auth_verify_password(storage: Value, user_id: Value, password: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::from_bool(false));
        return pending;
    }
    let user_id = match value_to_string(user_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let password = match value_to_string(password) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        let user_key = format!("auth:user:{user_id}");
        let ok = storage_get_json::<UserRecord>(&user_key)
            .await
            .map(|user| verify_password_hash(&user.pw_hash, &password))
            .unwrap_or(false);
        resolve_pending(state, Value::from_bool(ok));
    });
    pending
}

pub fn auth_issue_jwt(storage: Value, user_id: Value, claims: Value, ttl_secs: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let user_id = match value_to_string(user_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let ttl = int_value(ttl_secs).unwrap_or(3600).max(1) as u64;
    let claims_extra = if claims.is_nil() {
        HashMap::new()
    } else if let Some(json) = json_from_map(claims) {
        match json {
            JsonValue::Object(obj) => obj.into_iter().collect(),
            _ => HashMap::new(),
        }
    } else {
        HashMap::new()
    };
    runtime_spawn(async move {
        let now = now_secs();
        let token_claims = JwtClaims {
            sub: user_id,
            exp: (now + ttl) as usize,
            iat: now as usize,
            jti: Uuid::new_v4().to_string(),
            extra: claims_extra,
        };
        let header = Header::default();
        let key = EncodingKey::from_secret(jwt_secret().as_bytes());
        let token = jsonwebtoken::encode(&header, &token_claims, &key).ok();
        match token {
            Some(token) => {
                let val = string::str_from_bytes(token.as_bytes());
                resolve_pending(state, val);
            }
            None => resolve_pending(state, Value::nil()),
        }
    });
    pending
}

pub fn auth_verify_jwt(storage: Value, token: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let token = match value_to_string(token) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    runtime_spawn(async move {
        let key = DecodingKey::from_secret(jwt_secret().as_bytes());
        let mut validation = Validation::default();
        validation.validate_exp = true;
        validation.leeway = 0;
        let decoded = jsonwebtoken::decode::<JwtClaims>(&token, &key, &validation);
        let out = match decoded {
            Ok(data) => {
                let mut obj = JsonMap::new();
                obj.insert("sub".to_string(), JsonValue::String(data.claims.sub));
                obj.insert("exp".to_string(), json!(data.claims.exp));
                obj.insert("iat".to_string(), json!(data.claims.iat));
                obj.insert("jti".to_string(), JsonValue::String(data.claims.jti));
                for (k, v) in data.claims.extra {
                    obj.insert(k, v);
                }
                map_from_json(&JsonValue::Object(obj))
            }
            Err(_) => Value::nil(),
        };
        resolve_pending(state, out);
    });
    pending
}

pub fn auth_issue_email_token(storage: Value, user_id: Value, ttl_secs: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let user_id = match value_to_string(user_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let ttl = int_value(ttl_secs).unwrap_or(86_400).max(1) as u64;
    runtime_spawn(async move {
        let user_key = format!("auth:user:{user_id}");
        if storage_get_json::<UserRecord>(&user_key).await.is_none() {
            resolve_pending(state, Value::nil());
            return;
        }
        let token = Uuid::new_v4().to_string();
        let exp = now_secs() + ttl;
        let token_key = format!("auth:email_token:{token}");
        let stored = storage_set_json(&token_key, &EmailToken { user_id, exp }).await;
        if !stored {
            resolve_pending(state, Value::nil());
            return;
        }
        resolve_pending(state, string::str_from_bytes(token.as_bytes()));
    });
    pending
}

pub fn auth_verify_email_token(storage: Value, token: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let token = match value_to_string(token) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    runtime_spawn(async move {
        let token_key = format!("auth:email_token:{token}");
        let now = now_secs();
        let entry = storage_get_json::<EmailToken>(&token_key).await;
        let Some(entry) = entry else {
            resolve_pending(state, Value::nil());
            return;
        };
        if entry.exp <= now {
            resolve_pending(state, Value::nil());
            return;
        }
        let user_key = format!("auth:user:{}", entry.user_id);
        if let Some(mut user) = storage_get_json::<UserRecord>(&user_key).await {
            user.verified = true;
            let _ = storage_set_json(&user_key, &user).await;
        }
        let _ = storage_delete(&token_key).await;
        resolve_pending(state, string::str_from_bytes(entry.user_id.as_bytes()));
    });
    pending
}

async fn oauth_github(code: &str) -> Option<UserRecord> {
    let config = auth_config();
    let oauth = config.github?;
    let client_id = oauth.client_id;
    let client_secret = oauth.client_secret;
    let redirect_uri = oauth.redirect_uri;
    let client = reqwest::Client::new();
    let mut form = vec![
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code.to_string()),
    ];
    if let Some(uri) = redirect_uri {
        form.push(("redirect_uri", uri));
    }
    let token_resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
        .ok()?;
    let token_json: JsonValue = token_resp.json().await.ok()?;
    let access_token = token_json.get("access_token")?.as_str()?.to_string();

    let user_resp = client
        .get("https://api.github.com/user")
        .header("User-Agent", "wrela-runtime")
        .bearer_auth(&access_token)
        .send()
        .await
        .ok()?;
    let user_json: JsonValue = user_resp.json().await.ok()?;
    let username = user_json.get("login")?.as_str()?.to_string();
    let email = user_json
        .get("email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let email = if email.is_empty() {
        let emails_resp = client
            .get("https://api.github.com/user/emails")
            .header("User-Agent", "wrela-runtime")
            .bearer_auth(&access_token)
            .send()
            .await
            .ok()?;
        let emails_json: JsonValue = emails_resp.json().await.ok()?;
        emails_json
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|v| v.get("primary").and_then(|p| p.as_bool()) == Some(true))
                    .and_then(|v| v.get("email").and_then(|e| e.as_str()))
            })
            .unwrap_or("")
            .to_string()
    } else {
        email
    };
    if email.is_empty() {
        return None;
    }
    Some(UserRecord {
        id: Uuid::new_v4().to_string(),
        email,
        username,
        pw_hash: String::new(),
        verified: true,
        created_at: now_secs(),
    })
}

async fn oauth_google(code: &str) -> Option<UserRecord> {
    let config = auth_config();
    let oauth = config.google?;
    let client_id = oauth.client_id;
    let client_secret = oauth.client_secret;
    let redirect_uri = oauth.redirect_uri?;
    let client = reqwest::Client::new();
    let form = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code.to_string()),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code".to_string()),
    ];
    let token_resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&form)
        .send()
        .await
        .ok()?;
    let token_json: JsonValue = token_resp.json().await.ok()?;
    let access_token = token_json.get("access_token")?.as_str()?.to_string();

    let user_resp = client
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(&access_token)
        .send()
        .await
        .ok()?;
    let user_json: JsonValue = user_resp.json().await.ok()?;
    let email = user_json.get("email")?.as_str()?.to_string();
    let sub = user_json.get("sub")?.as_str()?.to_string();
    let name = user_json
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("google-user")
        .to_string();
    Some(UserRecord {
        id: sub,
        email,
        username: name,
        pw_hash: String::new(),
        verified: true,
        created_at: now_secs(),
    })
}

pub fn auth_oauth_login(storage: Value, provider: Value, code: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let provider = match value_to_string(provider) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let code = match value_to_string(code) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    runtime_spawn(async move {
        let user = match provider.as_str() {
            "github" => oauth_github(&code).await,
            "google" => oauth_google(&code).await,
            _ => None,
        };
        let Some(user) = user else {
            resolve_pending(state, Value::nil());
            return;
        };
        let email_key = format!("auth:email:{}", user.email);
        if let Some(existing_id) = storage_get_string(&email_key).await {
            if let Some(existing) =
                storage_get_json::<UserRecord>(&format!("auth:user:{existing_id}")).await
            {
                resolve_pending(state, user_to_map(&existing));
                return;
            }
        }
        let user_key = format!("auth:user:{}", user.id);
        let stored = storage_set_json(&user_key, &user).await
            && storage_set_string(&email_key, &user.id).await;
        if !stored {
            resolve_pending(state, Value::nil());
            return;
        }
        resolve_pending(state, user_to_map(&user));
    });
    pending
}
