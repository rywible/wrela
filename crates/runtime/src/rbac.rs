use crate::actor::{pending_new, resolve_pending, runtime_spawn};
use crate::list;
use crate::storage_helpers::{storage_get_json, storage_get_json_vec, storage_set_json, value_to_string};
use crate::string;
use crate::value::Value;
use crate::wr_rc_dec;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
struct StoredRole {
    id: String,
    scope: String,
    name: String,
    permissions: Vec<String>,
}

fn list_to_strings(list_val: Value) -> Vec<String> {
    let Some(list_ptr) = list::as_list_ref(list_val) else { return Vec::new() };
    let mut out = Vec::new();
    unsafe {
        for val in (&(*list_ptr).data).iter() {
            if let Some(s) = value_to_string(*val) {
                out.push(s);
            }
        }
    }
    out
}

pub fn rbac_create_role(storage: Value, scope: Value, name: Value, permissions: Value) -> Value {
    let (pending, state) = pending_new();
    if storage.is_nil() {
        resolve_pending(state, Value::nil());
        return pending;
    }
    let scope = match value_to_string(scope) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let name = match value_to_string(name) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    let perms = list_to_strings(permissions);
    runtime_spawn(async move {
        let id = Uuid::new_v4().to_string();
        let role = StoredRole {
            id: id.clone(),
            scope,
            name,
            permissions: perms,
        };
        let key = format!("rbac:role:{id}");
        if !storage_set_json(&key, &role).await {
            resolve_pending(state, Value::nil());
            return;
        }
        resolve_pending(state, string::str_from_bytes(id.as_bytes()));
    });
    pending
}

pub fn rbac_assign_role(storage: Value, user_id: Value, role_id: Value, scope_id: Value) -> Value {
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
    let role_id = match value_to_string(role_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let scope_id = match value_to_string(scope_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        let role_key = format!("rbac:role:{role_id}");
        if storage_get_json::<StoredRole>(&role_key).await.is_none() {
            resolve_pending(state, Value::from_bool(false));
            return;
        }
        let assign_key = format!("rbac:assign:{scope_id}:{user_id}");
        let mut roles = storage_get_json_vec::<String>(&assign_key).await;
        if !roles.contains(&role_id) {
            roles.push(role_id);
        }
        let ok = storage_set_json(&assign_key, &roles).await;
        resolve_pending(state, Value::from_bool(ok));
    });
    pending
}

pub fn rbac_check(storage: Value, user_id: Value, permission: Value, scope_id: Value) -> Value {
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
    let permission = match value_to_string(permission) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    let scope_id = match value_to_string(scope_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::from_bool(false));
            return pending;
        }
    };
    runtime_spawn(async move {
        let assign_key = format!("rbac:assign:{scope_id}:{user_id}");
        let role_ids = storage_get_json_vec::<String>(&assign_key).await;
        let mut allowed = false;
        for role_id in role_ids {
            let role_key = format!("rbac:role:{role_id}");
            if let Some(role) = storage_get_json::<StoredRole>(&role_key).await {
                if role.permissions.iter().any(|p| p == &permission) {
                    allowed = true;
                    break;
                }
            }
        }
        resolve_pending(state, Value::from_bool(allowed));
    });
    pending
}

pub fn rbac_permissions_for(storage: Value, user_id: Value, scope_id: Value) -> Value {
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
    let scope_id = match value_to_string(scope_id) {
        Some(v) => v,
        None => {
            resolve_pending(state, Value::nil());
            return pending;
        }
    };
    runtime_spawn(async move {
        let assign_key = format!("rbac:assign:{scope_id}:{user_id}");
        let role_ids = storage_get_json_vec::<String>(&assign_key).await;
        let mut perms = Vec::new();
        for role_id in role_ids {
            let role_key = format!("rbac:role:{role_id}");
            if let Some(role) = storage_get_json::<StoredRole>(&role_key).await {
                for perm in &role.permissions {
                    if !perms.contains(perm) {
                        perms.push(perm.clone());
                    }
                }
            }
        }
        let list_val = list::list_new(0);
        for perm in perms {
            let val = string::str_from_bytes(perm.as_bytes());
            list::list_push(list_val, val);
            unsafe { wr_rc_dec(val) };
        }
        resolve_pending(state, list_val);
    });
    pending
}
