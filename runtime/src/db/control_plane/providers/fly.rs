use crate::db::control_plane::environment::{EnvironmentProvider, NodeInfo, NodeLifecycleState};
use serde_json::Value;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct FlyMachinesProvider {
    pub app_name: String,
    pub region: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlyProviderError {
    MissingEnv(&'static str),
    Command(String),
    Parse(String),
    NoReplacementCandidate,
}

impl FlyMachinesProvider {
    pub fn from_env() -> Result<Self, FlyProviderError> {
        let app_name = std::env::var("FLY_APP_NAME")
            .map_err(|_| FlyProviderError::MissingEnv("FLY_APP_NAME"))?;
        let region = std::env::var("WRELADB_REGION")
            .ok()
            .and_then(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                if normalized.is_empty() {
                    None
                } else {
                    Some(normalized)
                }
            })
            .ok_or(FlyProviderError::MissingEnv("WRELADB_REGION"))?;
        Ok(Self { app_name, region })
    }

    fn run_flyctl(&self, args: &[&str]) -> Result<Vec<u8>, FlyProviderError> {
        let output = Command::new("flyctl")
            .args(args)
            .output()
            .map_err(|err| FlyProviderError::Command(err.to_string()))?;
        if output.status.success() {
            Ok(output.stdout)
        } else {
            Err(FlyProviderError::Command(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }

    fn parse_machine_id(machine: &Value) -> Option<String> {
        machine
            .get("id")
            .or_else(|| machine.get("ID"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }

    fn parse_machine_state(machine: &Value) -> Option<String> {
        machine
            .get("state")
            .or_else(|| machine.get("State"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }
}

impl EnvironmentProvider for FlyMachinesProvider {
    type Error = FlyProviderError;

    fn list_nodes(&self) -> Result<Vec<NodeInfo>, Self::Error> {
        let bytes = self.run_flyctl(&["machines", "list", "-a", &self.app_name, "--json"])?;
        let parsed: Value = serde_json::from_slice(&bytes)
            .map_err(|err| FlyProviderError::Parse(err.to_string()))?;
        let mut out = Vec::new();
        let Some(items) = parsed.as_array() else {
            return Err(FlyProviderError::Parse(
                "fly machines list did not return array".to_string(),
            ));
        };
        for machine in items {
            let Some(machine_id) = Self::parse_machine_id(machine) else {
                continue;
            };
            let raw_state =
                Self::parse_machine_state(machine).unwrap_or_else(|| "unknown".to_string());
            let state = map_fly_state(&raw_state);
            out.push(NodeInfo {
                node_id: machine_id.clone(),
                machine_id,
                slot: None,
                state,
            });
        }
        Ok(out)
    }

    fn create_replacement_node(&self, replace_node_id: &str) -> Result<NodeInfo, Self::Error> {
        let bytes = self.run_flyctl(&[
            "machine",
            "clone",
            replace_node_id,
            "-a",
            &self.app_name,
            "--region",
            &self.region,
            "--json",
        ])?;
        let parsed: Value = serde_json::from_slice(&bytes)
            .map_err(|err| FlyProviderError::Parse(err.to_string()))?;
        let machine_id =
            Self::parse_machine_id(&parsed).ok_or(FlyProviderError::NoReplacementCandidate)?;
        Ok(NodeInfo {
            node_id: machine_id.clone(),
            machine_id,
            slot: None,
            state: NodeLifecycleState::Degraded,
        })
    }

    fn drain_node(&self, node_id: &str) -> Result<(), Self::Error> {
        let _ = self.run_flyctl(&["machine", "cordon", node_id, "-a", &self.app_name]);
        Ok(())
    }

    fn delete_node(&self, node_id: &str) -> Result<(), Self::Error> {
        let _ = self.run_flyctl(&[
            "machine",
            "destroy",
            node_id,
            "-a",
            &self.app_name,
            "--force",
            "--yes",
        ])?;
        Ok(())
    }
}

pub fn map_fly_state(raw: &str) -> NodeLifecycleState {
    match raw {
        "started" => NodeLifecycleState::Healthy,
        "starting" | "stopping" => NodeLifecycleState::Degraded,
        _ => NodeLifecycleState::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        app: Option<String>,
        region: Option<String>,
        primary_region: Option<String>,
    }

    impl EnvGuard {
        fn set(app: Option<&str>, region: Option<&str>, primary_region: Option<&str>) -> Self {
            let guard = Self {
                app: std::env::var("FLY_APP_NAME").ok(),
                region: std::env::var("WRELADB_REGION").ok(),
                primary_region: std::env::var("PRIMARY_REGION").ok(),
            };
            match app {
                Some(value) => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::set_var("FLY_APP_NAME", value) };
                }
                None => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::remove_var("FLY_APP_NAME") };
                }
            }
            match region {
                Some(value) => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::set_var("WRELADB_REGION", value) };
                }
                None => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::remove_var("WRELADB_REGION") };
                }
            }
            match primary_region {
                Some(value) => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::set_var("PRIMARY_REGION", value) };
                }
                None => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::remove_var("PRIMARY_REGION") };
                }
            }
            guard
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.app {
                Some(value) => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::set_var("FLY_APP_NAME", value) };
                }
                None => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::remove_var("FLY_APP_NAME") };
                }
            }
            match &self.region {
                Some(value) => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::set_var("WRELADB_REGION", value) };
                }
                None => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::remove_var("WRELADB_REGION") };
                }
            }
            match &self.primary_region {
                Some(value) => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::set_var("PRIMARY_REGION", value) };
                }
                None => {
                    // SAFETY: guarded by ENV_LOCK for test-local env mutation.
                    unsafe { std::env::remove_var("PRIMARY_REGION") };
                }
            }
        }
    }

    #[test]
    fn state_mapping_is_fail_closed() {
        assert_eq!(map_fly_state("started"), NodeLifecycleState::Healthy);
        assert_eq!(map_fly_state("stopping"), NodeLifecycleState::Degraded);
        assert_eq!(map_fly_state("unknown"), NodeLifecycleState::Failed);
    }

    #[test]
    fn from_env_fails_without_region() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let _guard = EnvGuard::set(Some("app"), None, None);
        let err = FlyMachinesProvider::from_env().expect_err("missing region must fail");
        assert_eq!(err, FlyProviderError::MissingEnv("WRELADB_REGION"));
    }

    #[test]
    fn from_env_uses_normalized_wreladb_region() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let _guard = EnvGuard::set(Some("app"), Some("IAD"), Some("ord"));
        let provider = FlyMachinesProvider::from_env().expect("provider");
        assert_eq!(provider.app_name, "app");
        assert_eq!(provider.region, "iad");
    }

    #[test]
    fn from_env_does_not_fallback_to_primary_region() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let _guard = EnvGuard::set(Some("app"), None, Some("ord"));
        let err = FlyMachinesProvider::from_env().expect_err("missing WRELADB_REGION must fail");
        assert_eq!(err, FlyProviderError::MissingEnv("WRELADB_REGION"));
    }
}
