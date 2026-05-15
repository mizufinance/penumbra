use std::{collections::BTreeMap, path::Path};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditDemoState {
    pub setup: SetupState,
    pub endpoints: EndpointState,
    pub asset: AssetState,
    #[serde(default)]
    pub users: Vec<UserState>,
    pub scan: ScanState,
    pub scanner: ScannerState,
    #[serde(default)]
    pub ledger_rows: Vec<LedgerRow>,
    #[serde(default)]
    pub audits: Vec<AuditRecord>,
    #[serde(default)]
    pub events: Vec<Value>,
    #[serde(default)]
    pub ring: Option<RingState>,
    #[serde(default)]
    pub issuer: Option<IssuerState>,
}

impl AuditDemoState {
    pub fn new(penumbra_grpc: String, denom: String, threshold: String) -> Self {
        Self {
            setup: SetupState {
                initialized: false,
                asset_registered: false,
                updated_at: Some(now_string()),
            },
            endpoints: EndpointState { penumbra_grpc },
            asset: AssetState { denom, threshold },
            users: Vec::new(),
            scan: ScanState::default(),
            scanner: ScannerState::default(),
            ledger_rows: Vec::new(),
            audits: Vec::new(),
            events: Vec::new(),
            ring: None,
            issuer: None,
        }
    }

    pub fn subject(&self, name_or_slug: &str) -> Option<AuditSubject> {
        let slug = slugify(name_or_slug);
        self.users.iter().find_map(|user| {
            if user.slug == slug || user.name.eq_ignore_ascii_case(name_or_slug) {
                user.subject()
            } else {
                None
            }
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupState {
    pub initialized: bool,
    pub asset_registered: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointState {
    pub penumbra_grpc: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetState {
    pub denom: String,
    pub threshold: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    pub fn new(name: &str, slug: &str, home: String, address: String) -> Self {
        Self {
            name: name.to_string(),
            slug: slug.to_string(),
            home,
            addresses: vec![UserAddress { index: 0, address }],
            default: true,
            created_at: now_string(),
        }
    }

    pub fn subject(&self) -> Option<AuditSubject> {
        Some(AuditSubject {
            name: self.name.clone(),
            slug: self.slug.clone(),
            address: self.addresses.first()?.address.clone(),
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserAddress {
    pub index: u64,
    pub address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanState {
    #[serde(default)]
    pub detected: Vec<DetectedRow>,
    #[serde(default)]
    pub scan_time: Option<Value>,
    #[serde(default)]
    pub detected_count: usize,
    #[serde(default)]
    pub flagged_count: usize,
    #[serde(default)]
    pub audited_count: usize,
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            detected: Vec::new(),
            scan_time: None,
            detected_count: 0,
            flagged_count: 0,
            audited_count: 0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScannerState {
    pub running: bool,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub last_height: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl Default for ScannerState {
    fn default() -> Self {
        Self {
            running: false,
            message: None,
            last_height: None,
            updated_at: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
pub struct IssuerState {
    pub dk_hex: String,
    pub dk_pub_hex: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRecord {
    pub user_slug: String,
    pub user_name: String,
    pub at: String,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectedScanFile {
    #[serde(default)]
    pub scan_info: Value,
    #[serde(default)]
    pub detected: Vec<DetectedRow>,
}

impl DetectedScanFile {
    pub fn empty() -> Self {
        Self {
            scan_info: Value::Object(Default::default()),
            detected: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectedRow {
    pub height: i64,
    pub tx_hash: String,
    pub action_index: i64,
    #[serde(default)]
    pub output_index: i64,
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub is_flagged: bool,
    #[serde(default)]
    pub flow_type: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl DetectedRow {
    pub fn row_ref(&self) -> RowRef {
        RowRef {
            height: self.height,
            tx_hash: self.tx_hash.clone(),
            action_index: self.action_index,
            output_index: self.output_index,
        }
    }

    pub fn is_private_transfer(&self) -> bool {
        self.flow_type.as_deref() == Some("private_transfer")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LedgerRow {
    #[serde(default)]
    pub height: Option<i64>,
    #[serde(default)]
    pub tx_hash: Option<String>,
    #[serde(default)]
    pub action_index: Option<i64>,
    #[serde(default)]
    pub output_index: Option<i64>,
    #[serde(default)]
    pub flow_type: Option<String>,
    #[serde(default)]
    pub is_flagged: bool,
    #[serde(default)]
    pub amount: Option<Value>,
    #[serde(default)]
    pub self_alias: Option<String>,
    #[serde(default)]
    pub counterparty_alias: Option<String>,
    #[serde(default)]
    pub audited_subjects: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl LedgerRow {
    pub fn matches_ref(&self, row_ref: &RowRef) -> bool {
        self.height == Some(row_ref.height)
            && self.tx_hash.as_deref() == Some(row_ref.tx_hash.as_str())
            && self.action_index == Some(row_ref.action_index)
            && self.output_index.unwrap_or_default() == row_ref.output_index
    }

    pub fn has_amount(&self) -> bool {
        self.amount.as_ref().is_some_and(|amount| !amount.is_null())
    }

    pub fn self_alias_matches(&self, name: &str) -> bool {
        self.self_alias
            .as_deref()
            .is_some_and(|alias| alias == name || alias.starts_with(&format!("{name} ")))
    }

    pub fn counterparty_alias_known(&self) -> bool {
        self.counterparty_alias
            .as_deref()
            .is_some_and(|alias| !alias.is_empty())
    }

    pub fn fully_known(&self) -> bool {
        let has_self = self
            .self_alias
            .as_deref()
            .is_some_and(|alias| !alias.is_empty());
        match self.flow_type.as_deref() {
            Some("shield" | "withdraw") => self.has_amount() && has_self,
            _ => self.has_amount() && has_self && self.counterparty_alias_known(),
        }
    }

    pub fn is_clear_flow_for(&self, name: &str) -> bool {
        matches!(self.flow_type.as_deref(), Some("shield" | "withdraw"))
            && self.self_alias_matches(name)
    }

    pub fn audited_for(&self, name: &str) -> bool {
        self.audited_subjects.iter().any(|subject| subject == name)
    }

    pub fn row_ref(&self) -> Option<RowRef> {
        Some(RowRef {
            height: self.height?,
            tx_hash: self.tx_hash.clone()?,
            action_index: self.action_index?,
            output_index: self.output_index.unwrap_or_default(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RowRef {
    pub height: i64,
    pub tx_hash: String,
    pub action_index: i64,
    pub output_index: i64,
}

impl RowRef {
    pub fn from_value(value: &Value) -> Option<Self> {
        Some(Self {
            height: value.get("height")?.as_i64()?,
            tx_hash: value.get("tx_hash")?.as_str()?.to_string(),
            action_index: value.get("action_index")?.as_i64()?,
            output_index: value
                .get("output_index")
                .and_then(Value::as_i64)
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AuditSubject {
    pub name: String,
    pub slug: String,
    pub address: String,
}

pub fn now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

pub fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in value.to_ascii_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug.chars().take(48).collect()
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

pub fn missing_ring() -> anyhow::Error {
    anyhow!("audit demo ring details missing; run audit-demo setup first")
}

#[cfg(test)]
mod tests {
    use super::{AuditDemoState, LedgerRow, UserState};

    #[test]
    fn typed_state_roundtrips() {
        let mut state = AuditDemoState::new(
            "http://127.0.0.1:8080".to_string(),
            "transfer/channel-0/ubrl".to_string(),
            "500000000".to_string(),
        );
        state.users.push(UserState::new(
            "Alice",
            "alice",
            ".localnet/audit-demo/wallets/alice".to_string(),
            "penumbra1example".to_string(),
        ));
        let encoded = serde_json::to_vec(&state).expect("state should encode");
        let decoded: AuditDemoState =
            serde_json::from_slice(&encoded).expect("state should decode");
        assert_eq!(decoded.users[0].name, "Alice");
    }

    #[test]
    fn malformed_state_fails() {
        let error = serde_json::from_str::<AuditDemoState>(r#"{"setup": "bad"}"#)
            .expect_err("malformed state should fail");
        assert!(error.to_string().contains("invalid type"));
    }

    #[test]
    fn ledger_row_tracks_completeness() {
        let row: LedgerRow = serde_json::from_value(serde_json::json!({
            "flow_type": "private_transfer",
            "amount": "17",
            "self_alias": "Alice",
            "counterparty_alias": "Bob"
        }))
        .expect("row should decode");
        assert!(row.fully_known());
    }
}
