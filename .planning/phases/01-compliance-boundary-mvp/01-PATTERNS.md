# Phase 1: Compliance Boundary MVP - Pattern Map

**Mapped:** 2026-05-13
**Files analyzed:** 4
**Analogs found:** 4 / 4

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/core/component/compliance/src/audit_records.rs` | model, utility | transform | `crates/core/component/compliance/src/scanner/types.rs` + `crates/core/component/compliance/src/audit_validation.rs` | exact |
| `crates/core/component/compliance/src/audit.rs` | service, facade | CRUD, file-I/O, transform | `crates/core/component/compliance/src/scanner/storage.rs` + existing `audit.rs` | exact |
| `crates/core/component/compliance/src/lib.rs` | config, facade | request-response export surface | `crates/core/component/compliance/src/scanner/mod.rs` + existing `lib.rs` | exact |
| `crates/core/component/compliance/src/audit_records.rs` inline `#[cfg(test)]` module | test | transform | `crates/core/component/compliance/src/audit_validation.rs` tests | exact |
| `crates/core/component/compliance/src/audit.rs` inline `#[cfg(test)]` module | test | CRUD, transform | existing `crates/core/component/compliance/src/audit.rs` tests | exact |

## Pattern Assignments

### `crates/core/component/compliance/src/audit_records.rs` (model, utility; transform)

**Analog:** `crates/core/component/compliance/src/scanner/types.rs`

**Imports and typed-record pattern** (lines 1-16):

```rust
use penumbra_sdk_asset::asset;

use crate::transfer::TransferComplianceCiphertext;
pub use crate::{ActionRef, BlockRef, OutputRef, TxRef};

pub const FLOW_TYPE_PRIVATE_TRANSFER: &str = "private_transfer";
pub const FLOW_TYPE_SHIELD: &str = "shield";
pub const FLOW_TYPE_WITHDRAW: &str = "withdraw";
pub const DECRYPTED_VIA_ISSUER_DK: &str = "issuer_dk";
pub const DECRYPTED_VIA_ORBIS_PRE: &str = "orbis_pre";
pub const DECRYPTED_VIA_PUBLIC: &str = "public";
pub const AUDIT_STATUS_PENDING: &str = "pending";
pub const AUDIT_STATUS_EVIDENCE_VALID: &str = "evidence_valid";
pub const AUDIT_STATUS_EVIDENCE_INVALID: &str = "evidence_invalid";
pub const AUDIT_STATUS_DECRYPT_FAILED: &str = "decrypt_failed";
pub const AUDIT_STATUS_AUDIT_COMPLETE: &str = "audit_complete";
```

**Core record shape** (lines 68-98):

```rust
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditRowKey {
    pub height: u64,
    #[serde(rename = "tx_hash")]
    pub tx_hash_hex: String,
    pub action_index: u32,
    pub output_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditLedgerRow {
    pub height: u64,
    #[serde(rename = "block_hash")]
    pub block_hash_hex: String,
    pub tx_index: u32,
    #[serde(rename = "tx_hash")]
    pub tx_hash_hex: String,
    pub action_index: u32,
    pub output_index: u32,
    pub flow_type: String,
    pub asset_id: String,
    pub is_flagged: bool,
    pub amount: Option<String>,
    pub self_address: Option<String>,
    pub self_alias: Option<String>,
    pub counterparty_address: Option<String>,
    pub counterparty_alias: Option<String>,
    pub public_address: Option<String>,
    pub decrypted_via: Option<String>,
    pub audited_subjects: Vec<String>,
}
```

**Pure classification pattern** from `audit_validation.rs` (lines 13-48):

```rust
pub struct AuditValidationInput {
    pub evidence: ComplianceEvidenceObject,
    pub upload_bundle: Option<TransferOrbisUploadBundle>,
    pub ring_pk: Element,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuditValidationStatus {
    Valid,
    MissingUploadBundle,
    InvalidEvidence(String),
    InvalidOrbisPackage(String),
}

pub fn validate_audit_evidence(input: AuditValidationInput) -> AuditValidationStatus {
    if let Err(error) = validate_evidence_shape(&input.evidence, &input.ring_pk) {
        return AuditValidationStatus::InvalidEvidence(error.to_string());
    }

    match (
        &input.evidence.orbis_upload_bundle_hash,
        &input.upload_bundle,
    ) {
        (Some(_), None) => AuditValidationStatus::MissingUploadBundle,
        (_, Some(bundle)) => match validate_upload_bundle(&input.evidence, bundle) {
            Ok(()) => AuditValidationStatus::Valid,
            Err(error) => AuditValidationStatus::InvalidOrbisPackage(error.to_string()),
        },
        (None, None) => AuditValidationStatus::Valid,
    }
}
```

**Apply to new module:** Move `AuditDetectedRef`, `AuditScanExport`, `OrbisAuditEntry`, default flow helpers, import-eligibility classification, and row projection helpers here if extracted. Keep helpers pure: inputs should be typed row/status data, outputs should be DTOs or enum decisions. Do not open SQLite connections in this module.

---

### `crates/core/component/compliance/src/audit.rs` (service, facade; CRUD, file-I/O, transform)

**Analog:** existing `crates/core/component/compliance/src/audit.rs`

**Imports pattern** (lines 1-20):

```rust
use anyhow::{anyhow, Context, Result};
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::scanner::storage::SqliteScannerStore;
use crate::scanner::types::{
    AuditLedgerRow, AUDIT_STATUS_AUDIT_COMPLETE, AUDIT_STATUS_DECRYPT_FAILED,
    AUDIT_STATUS_EVIDENCE_INVALID, AUDIT_STATUS_EVIDENCE_VALID, AUDIT_STATUS_PENDING,
    DECRYPTED_VIA_ISSUER_DK, DECRYPTED_VIA_ORBIS_PRE, FLOW_TYPE_PRIVATE_TRANSFER,
};
```

**Keep facade and SQLite edge pattern** (lines 225-263):

```rust
pub fn export_orbis_pending_scan(store: &SqliteScannerStore) -> Result<AuditScanExport> {
    let conn = store.lock_conn()?;
    let mut rows = conn.prepare(
        "SELECT height, tx_hash, action_index, output_index, asset_id, is_flagged, ?1
         FROM scanner_detections
         WHERE is_flagged = 0
           AND audit_status = ?2
         ORDER BY height, tx_hash, action_index, output_index",
    )?;
    let detected = rows
        .query_map(
            params![FLOW_TYPE_PRIVATE_TRANSFER, AUDIT_STATUS_EVIDENCE_VALID],
            |row| {
                let height: i64 = row.get(0)?;
                let tx_hash: Vec<u8> = row.get(1)?;
                let action_index: i64 = row.get(2)?;
                let output_index: i64 = row.get(3)?;
                let asset_id: String = row.get(4)?;
                let is_flagged: i64 = row.get(5)?;
                let flow_type: String = row.get(6)?;
                Ok(AuditDetectedRef {
                    height: height as u64,
                    tx_hash: hex::encode(tx_hash),
                    action_index: action_index as u32,
                    output_index: output_index as u32,
                    asset_id,
                    is_flagged: is_flagged != 0,
                    flow_type,
                })
            },
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(rows);
    drop(conn);
    Ok(AuditScanExport {
        scan_info: scan_info(store)?,
        detected,
    })
}
```

**Import validation and failure persistence pattern** (lines 265-318):

```rust
pub fn import_orbis_audit_entries(
    store: &SqliteScannerStore,
    entries: &[OrbisAuditEntry],
    subject: Option<&str>,
) -> Result<u64> {
    let conn = store.lock_conn()?;
    let tx = conn.unchecked_transaction()?;
    let mut updated = 0u64;
    for entry in entries {
        let tx_hash = decode_tx_hash(&entry.tx_hash)?;
        let row_status: Option<(String, i64)> = tx
            .query_row(
                "SELECT audit_status, is_flagged
                 FROM scanner_detections
                 WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
                params![
                    entry.height as i64,
                    tx_hash.as_slice(),
                    entry.action_index as i64,
                    entry.output_index as i64,
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row_status {
            Some((status, 0))
                if status == AUDIT_STATUS_EVIDENCE_VALID
                    || status == AUDIT_STATUS_DECRYPT_FAILED
                    || status == AUDIT_STATUS_AUDIT_COMPLETE => {}
            Some((status, _)) => {
                record_evidence_failure_tx(
                    &tx,
                    entry.height,
                    tx_hash.as_slice(),
                    entry.action_index,
                    entry.output_index,
                    EVIDENCE_STAGE_ORBIS_IMPORT,
                    &format!("row is not an evidence-valid unflagged detection: {status}"),
                )?;
                continue;
            }
            None => {
                record_evidence_failure_tx(
                    &tx,
                    entry.height,
                    tx_hash.as_slice(),
                    entry.action_index,
                    entry.output_index,
                    EVIDENCE_STAGE_ORBIS_IMPORT,
                    "detected row not found",
                )?;
                continue;
            }
        }
```

**Shared failure write pattern** (lines 854-893):

```rust
fn record_evidence_failure_tx(
    tx: &rusqlite::Transaction<'_>,
    height: u64,
    tx_hash: &[u8],
    action_index: u32,
    output_index: u32,
    stage: &str,
    reason: &str,
) -> Result<()> {
    tx.execute(
        "INSERT OR REPLACE INTO audit_evidence_failures
         (height, tx_hash, action_index, output_index, stage, reason, failed_at_unix)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            height as i64,
            tx_hash,
            action_index as i64,
            output_index as i64,
            stage,
            reason,
            now_unix(),
        ],
    )?;
    tx.execute(
        "UPDATE scanner_detections
         SET audit_status = ?1
         WHERE height = ?2 AND tx_hash = ?3 AND action_index = ?4 AND output_index = ?5
           AND audit_status IN (?6, ?7)",
        params![
            AUDIT_STATUS_EVIDENCE_INVALID,
            height as i64,
            tx_hash,
            action_index as i64,
            output_index as i64,
            AUDIT_STATUS_PENDING,
            AUDIT_STATUS_EVIDENCE_INVALID,
        ],
    )?;
    Ok(())
}
```

**Apply to modified module:** Keep SQL queries, transactions, `SqliteScannerStore::lock_conn`, and public functions in `audit.rs`. Replace inline DTO construction and status eligibility branches with calls into pure helpers from `audit_records.rs`. Do not introduce an `AuditStore` trait.

---

### `crates/core/component/compliance/src/lib.rs` (config, facade; export surface)

**Analog:** `crates/core/component/compliance/src/scanner/mod.rs` and existing `lib.rs`

**Module facade pattern** from `scanner/mod.rs` (lines 1-15):

```rust
pub mod advice;
pub mod screener;
pub mod storage;
pub mod sync;
pub mod types;
pub mod worker;

pub use advice::{AuditAdviceProvider, NoopAuditAdviceProvider, RingInfo, RpcAuditAdviceProvider};
pub use screener::{ComplianceScreener, ScreeningResult};
pub use storage::{ScannerStore, SqliteScannerStore, MAX_INVALID_CIPHERTEXTS_PER_BLOCK};
pub use sync::{extract_clear_flows, extract_compliance_ciphertexts};
pub use types::{
    ActionRef, AuditLedgerRow, AuditRowKey, BlockRef, ClearFlowEvent, ClearFlowKind,
    DetectionEvent, ExtractedComplianceCiphertext, InvalidCiphertext, OutputRef, TxRef,
};
```

**Compliance audit export surface** from `lib.rs` (lines 97-108):

```rust
pub mod audit_validation;
pub use audit_validation::{validate_audit_evidence, AuditValidationInput, AuditValidationStatus};

#[cfg(feature = "component")]
pub mod audit;
#[cfg(feature = "component")]
pub use audit::{
    decrypt_flagged_rows, export_detected_refs, export_ledger_rows, export_ledger_rows_json,
    export_orbis_pending_scan, export_scan_json, import_orbis_audit_entries, mark_row_audited,
    record_address_alias, record_evidence_failure, scanner_health_json,
    validate_and_save_evidence_object, AuditDetectedRef, AuditScanExport, OrbisAuditEntry,
};
```

**Apply to modified module:** Add `audit_records.rs` behind the same `#[cfg(feature = "component")]` gate as `audit.rs`. Move pure public DTOs there unconditionally, then keep public exports stable by re-exporting `AuditDetectedRef`, `AuditScanExport`, and `OrbisAuditEntry` from `lib.rs`; do not add alias modules or compatibility names.

---

### Focused Tests (test; transform, CRUD)

**Analog:** `crates/core/component/compliance/src/audit_validation.rs` tests

**Pure helper test pattern** (lines 191-207):

```rust
#[test]
fn validates_complete_evidence_and_upload_bundle() {
    assert_eq!(
        validate_audit_evidence(valid_input()),
        AuditValidationStatus::Valid
    );
}

#[test]
fn missing_upload_bundle_is_reported_without_panic() {
    let mut input = valid_input();
    input.upload_bundle = None;
    assert_eq!(
        validate_audit_evidence(input),
        AuditValidationStatus::MissingUploadBundle
    );
}
```

**Existing integration behavior tests** from `audit.rs` (lines 990-1029):

```rust
#[tokio::test]
async fn orbis_export_requires_valid_evidence() {
    let store = SqliteScannerStore::new(":memory:").unwrap();
    let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
    persist_evidence_detection(&store, &evidence, &bundle, false).await;

    assert_eq!(export_orbis_pending_scan(&store).unwrap().detected.len(), 0);

    validate_and_save_evidence_object(&store, &evidence, &bundle, &ring_pk).unwrap();
    let export = export_orbis_pending_scan(&store).unwrap();
    assert_eq!(export.detected.len(), 1);
    assert_eq!(
        export.detected[0].output_index,
        evidence.output_ref.output_index
    );
}

#[tokio::test]
async fn orbis_import_requires_valid_evidence() {
    let store = SqliteScannerStore::new(":memory:").unwrap();
    let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
    persist_evidence_detection(&store, &evidence, &bundle, false).await;
    let entry = orbis_entry(&evidence);

    assert_eq!(
        import_orbis_audit_entries(&store, std::slice::from_ref(&entry), Some("alice"))
            .unwrap(),
        0
    );
    assert_eq!(
        audit_status(&store, &evidence),
        AUDIT_STATUS_EVIDENCE_INVALID
    );

    validate_and_save_evidence_object(&store, &evidence, &bundle, &ring_pk).unwrap();
    assert_eq!(
        import_orbis_audit_entries(&store, &[entry], Some("alice")).unwrap(),
        1
    );
    assert_eq!(audit_status(&store, &evidence), AUDIT_STATUS_AUDIT_COMPLETE);
}
```

**Apply to tests:** Add pure unit tests for extracted import eligibility, detected-row projection, and ledger row projection without a SQLite fixture. Keep the existing SQLite tests to prove facade behavior is preserved.

## Shared Patterns

### Typed Records Before Effects

**Source:** `crates/core/component/compliance/src/scanner/types.rs`
**Apply to:** `audit_records.rs`, moved audit DTOs, pure projection helpers

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedComplianceCiphertext {
    pub output_ref: OutputRef,
    pub raw_bytes: Vec<u8>,
    pub upload_bundle_bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditRowKey {
    pub height: u64,
    #[serde(rename = "tx_hash")]
    pub tx_hash_hex: String,
    pub action_index: u32,
    pub output_index: u32,
}
```

### Pure Classification Before Persistence

**Source:** `crates/core/component/compliance/src/scanner/screener.rs`
**Apply to:** audit import eligibility and export projection helpers

```rust
#[derive(Clone, Debug)]
pub enum ScreeningResult {
    Irrelevant,
    Detected(DetectionEvent),
    InvalidCiphertext(InvalidCiphertext),
}

pub fn screen(&self, extracted: ExtractedComplianceCiphertext) -> ScreeningResult {
    let ciphertext = match TransferComplianceCiphertext::from_bytes(&extracted.raw_bytes) {
        Ok(ciphertext) => ciphertext,
        Err(error) => {
            return ScreeningResult::InvalidCiphertext(InvalidCiphertext {
                output_ref: extracted.output_ref,
                reason: error.to_string(),
                raw_bytes: extracted.raw_bytes,
            })
        }
    };
```

### SQLite Edge and Transactions

**Source:** `crates/core/component/compliance/src/scanner/storage.rs`
**Apply to:** `audit.rs`; do not move into `audit_records.rs`

```rust
pub struct SqliteScannerStore {
    conn: Arc<Mutex<Connection>>,
    pending: Arc<Mutex<PendingBlock>>,
}

impl SqliteScannerStore {
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::initialize_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            pending: Arc::new(Mutex::new(PendingBlock::default())),
        })
    }

    pub(crate) fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|e| anyhow!("scanner store connection mutex poisoned: {e}"))
    }
}
```

### Public Facade Stability

**Source:** `crates/core/component/compliance/src/lib.rs`
**Apply to:** moved audit DTOs and helper modules

```rust
#[cfg(feature = "component")]
pub mod audit;
#[cfg(feature = "component")]
pub use audit::{
    decrypt_flagged_rows, export_detected_refs, export_ledger_rows, export_ledger_rows_json,
    export_orbis_pending_scan, export_scan_json, import_orbis_audit_entries, mark_row_audited,
    record_address_alias, record_evidence_failure, scanner_health_json,
    validate_and_save_evidence_object, AuditDetectedRef, AuditScanExport, OrbisAuditEntry,
};
```

## Rejected / Contrast Context

### Registry/State Is Not the Primary Pattern for This MVP

`registry.rs` already has a state edge through `StateRead` / `StateWrite`, so use it as contrast rather than the main implementation analog.

**State read trait edge** from `registry.rs` (lines 72-83):

```rust
pub trait ComplianceRegistryRead: StateRead {
    /// Get the user compliance tree from state.
    async fn get_user_tree(&self) -> Result<QuadTree> {
        if let Some(tree) = self.object_get(state_key::cache::cached_user_tree()) {
            return Ok(tree);
        }

        match self.get_raw(state_key::user_tree()).await? {
            Some(bytes) => Ok(bincode::deserialize(&bytes)?),
            None => Ok(QuadTree::new()),
        }
    }
```

**State write edge** from `registry.rs` (lines 384-395, 450-495):

```rust
pub trait ComplianceRegistryWrite: StateWrite + ComplianceRegistryRead {
    /// Track that compliance trees were modified in this block.
    fn mark_compliance_trees_modified(&mut self) {
        self.object_put(state_key::cache::trees_modified(), true);
    }

    async fn add_compliance_leaf(&mut self, leaf: ComplianceLeaf) -> Result<u64> {
        let mut tree = self.get_user_tree_for_write().await?;
        let position = self.get_user_count().await?;
        let commitment = leaf.commit();
        tree.update(position, commitment)?;
        let new_count = position + 1;

        let tree_bytes = bincode::serialize(&tree)?;
        self.put_raw(state_key::user_tree().to_string(), tree_bytes);
        self.put_proto(state_key::user_count().to_string(), new_count);
        self.put(state_key::user_tree_root().to_string(), tree.root());
        self.write_user_tree_cache(tree);
        self.mark_compliance_trees_modified();

        let lookup_key = state_key::user_leaf_position(&leaf.address, &leaf.asset_id);
        self.put_proto(lookup_key, position);

        use penumbra_sdk_proto::DomainType;
        let leaf_data_key = state_key::user_leaf_data(&leaf.address, &leaf.asset_id);
        let leaf_bytes = leaf.encode_to_vec();
        self.put_raw(leaf_data_key, leaf_bytes);

        Ok(position)
    }
```

**Component handler contrast** from `component/state.rs` (lines 228-271):

```rust
async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
    anyhow::ensure!(
        state.is_asset_regulated(self.leaf.asset_id).await?,
        "cannot register user for unregulated asset {}",
        self.leaf.asset_id
    );

    if let Some(existing_position) = state
        .get_user_leaf_position(&self.leaf.address, self.leaf.asset_id)
        .await?
    {
        return Ok(());
    }

    let position = state.add_compliance_leaf(self.leaf.clone()).await?;
    let commitment = self.leaf.commit();
    let event = crate::event::EventUserRegistered {
        position,
        commitment,
        leaf: self.leaf.clone(),
    };
    state.record_pending_user_registration(event.clone());
    state.record_proto(event::user_registered(
        position,
        commitment,
        self.leaf.clone(),
    ));

    Ok(())
}
```

**Planner note:** A registry refactor should be rejected for this MVP unless implementation evidence identifies a small pure helper with tests. Do not implement deferred registration authorization or allowed-channel enforcement.

## No Analog Found

No target file lacks an analog. The only non-exact risk is naming: `audit_records.rs` is a recommended name from research, not an existing naming convention. A different focused module name is acceptable if it remains specific to audit DTOs/projection and avoids scanner-name mimicry.

## Metadata

**Analog search scope:** `crates/core/component/compliance/src/`, `crates/bin/orbis-integration/src/main.rs`, `crates/bin/orbis-audit` references via grep  
**Files scanned:** compliance source files under `crates/core/component/compliance/src/` plus binary caller grep  
**Pattern extraction date:** 2026-05-13
