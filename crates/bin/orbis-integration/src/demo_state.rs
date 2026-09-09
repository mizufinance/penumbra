use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shieldd_sdk_keys::Address;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditDemoState {
    pub setup: SetupState,
    #[serde(default)]
    pub users: Vec<UserState>,
    #[serde(default)]
    pub ring: Option<RingState>,
}

impl AuditDemoState {
    pub fn new() -> Self {
        Self {
            setup: SetupState {
                initialized: false,
                updated_at: Some(now_string()),
            },
            users: Vec::new(),
            ring: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub initialized: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserState {
    pub name: String,
    pub slug: String,
    pub home: String,
    pub addresses: Vec<UserAddress>,
    pub default: bool,
    pub created_at: String,
}

impl UserState {
    pub fn new(name: &str, slug: &str, home: String, address: String) -> Result<Self> {
        Ok(Self {
            name: name.to_string(),
            slug: slug.to_string(),
            home,
            addresses: vec![UserAddress::new(0, address)?],
            default: true,
            created_at: now_string(),
        })
    }

    pub fn add_address(&mut self, index: u64, address: String) -> Result<()> {
        self.addresses.push(UserAddress::new(index, address)?);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAddress {
    pub index: u64,
    pub address: String,
    #[serde(default)]
    pub transmission_key_hex: String,
}

impl UserAddress {
    fn new(index: u64, address: String) -> Result<Self> {
        let parsed: Address = address
            .parse()
            .with_context(|| format!("failed to parse demo address {address}"))?;
        Ok(Self {
            index,
            address,
            transmission_key_hex: hex::encode(parsed.transmission_key().0),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RingState {
    pub ring_pk_hex: String,
    pub ring_id: String,
    pub policy_id: String,
    pub resource: String,
    pub permission: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusDocument {
    pub state: String,
    pub step: String,
    pub message: String,
    pub updated_at: String,
}

impl StatusDocument {
    pub fn new(state: &str, step: &str, message: &str) -> Self {
        Self {
            state: state.to_string(),
            step: step.to_string(),
            message: message.to_string(),
            updated_at: now_string(),
        }
    }
}

pub fn now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

pub fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    serde_json::from_slice(
        &std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AuditDemoState, RingState, UserState};

    #[test]
    fn typed_state_roundtrips() {
        let mut state = AuditDemoState::new();
        state.setup.initialized = true;
        state.ring = Some(RingState {
            ring_pk_hex: "abcd".into(),
            ring_id: "ring".into(),
            policy_id: "policy".into(),
            resource: "document".into(),
            permission: "read".into(),
        });
        state.users.push(UserState::new(
            "Alice",
            "alice",
            ".localnet/audit-demo/wallets/alice".to_string(),
            "shieldd1u29dhz4vxgnek6a3vzxlejg0l83wegpu7hgs3yphdvljcnnnh89dvs6lc9hxxw94w464t7lh5x36cxnxyx0".to_string(),
        ).expect("test address should be valid"));
        let encoded = serde_json::to_vec(&state).expect("state should encode");
        let decoded: AuditDemoState =
            serde_json::from_slice(&encoded).expect("state should decode");
        assert_eq!(decoded, state);
    }

    #[test]
    fn malformed_state_fails() {
        let error = serde_json::from_str::<AuditDemoState>(r#"{"setup": "bad"}"#)
            .expect_err("malformed state should fail");
        assert!(error.to_string().contains("invalid type"));
    }
}
