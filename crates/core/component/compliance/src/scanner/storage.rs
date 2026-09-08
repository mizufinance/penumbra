use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::types::{
    BlockRef, OutputRef, DECRYPTED_VIA_PUBLIC, FLOW_TYPE_PRIVATE_TRANSFER, FLOW_TYPE_WITHDRAW,
};
use super::types::{CandidateEvidence, OutputOutcome, ScannedBlock};
use crate::audit_status::{AuditStatus, ScreenStatus};

pub const MAX_INVALID_CIPHERTEXTS_PER_BLOCK: usize = 256;
pub const HEARTBEAT_STALE_SECS: i64 = 30;
const READ_POOL_SIZE: usize = 4;
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const WAL_AUTOCHECKPOINT_PAGES: i64 = 1_000;

#[async_trait]
pub trait ScannerStore: Send + Sync {
    async fn last_scanned_block(&self) -> Result<Option<BlockRef>>;
    async fn block_by_height(&self, height: u64) -> Result<Option<BlockRef>>;
    async fn commit_scanned_block(&self, scanned: &ScannedBlock) -> Result<()>;
    async fn rollback_to_height(&self, height: u64) -> Result<()>;
    async fn detection_count(&self) -> Result<u64>;

    async fn mark_started(&self) -> Result<()> {
        Ok(())
    }
    async fn heartbeat(&self) -> Result<()> {
        Ok(())
    }
    async fn record_error(&self, _error: &str) -> Result<()> {
        Ok(())
    }
    async fn mark_stopped(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ScannerRuntimeState {
    pub started_at: Option<i64>,
    pub heartbeat_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_error_at: Option<i64>,
}

impl ScannerRuntimeState {
    /// Returns true if the scanner has processed a block within the last
    /// `HEARTBEAT_STALE_SECS` seconds. Reflects recent progress, not process
    /// liveness — a halted chain looks the same as a dead scanner here.
    pub fn is_active(&self, now_secs: i64) -> bool {
        match self.heartbeat_at {
            Some(hb) => now_secs.saturating_sub(hb) <= HEARTBEAT_STALE_SECS,
            None => false,
        }
    }
}

fn unix_now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

pub struct SqliteScannerStore {
    db_path: Arc<PathBuf>,
    writer: Arc<Mutex<Connection>>,
    read_pool: Arc<Mutex<Vec<Connection>>>,
}

impl SqliteScannerStore {
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        let conn = Connection::open(&db_path)?;
        Self::configure_writer(&conn)?;
        Self::initialize_schema(&conn)?;
        let read_pool = if db_path.as_os_str() == ":memory:" {
            Vec::new()
        } else {
            (0..READ_POOL_SIZE)
                .map(|_| Self::open_read_conn(&db_path))
                .collect::<Result<Vec<_>>>()?
        };
        Ok(Self {
            db_path: Arc::new(db_path),
            writer: Arc::new(Mutex::new(conn)),
            read_pool: Arc::new(Mutex::new(read_pool)),
        })
    }

    fn configure_writer(conn: &Connection) -> Result<()> {
        conn.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
        conn.pragma_update(None, "auto_vacuum", "INCREMENTAL")?;
        conn.execute_batch("VACUUM;")?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "wal_autocheckpoint", WAL_AUTOCHECKPOINT_PAGES)?;
        Ok(())
    }

    fn configure_reader(conn: &Connection) -> Result<()> {
        conn.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
        conn.pragma_update(None, "query_only", "ON")?;
        Ok(())
    }

    fn open_read_conn(db_path: &Path) -> Result<Connection> {
        let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Self::configure_reader(&conn)?;
        Ok(conn)
    }

    fn initialize_schema(conn: &Connection) -> Result<()> {
        let schema_sql = r#"
            CREATE TABLE IF NOT EXISTS scanner_schema (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                identity TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS scanner_blocks (
                height INTEGER PRIMARY KEY,
                block_hash BLOB NOT NULL,
                parent_hash BLOB NOT NULL,
                block_time_unix INTEGER,
                scan_status TEXT NOT NULL CHECK (scan_status IN ('committed'))
            );

            CREATE TABLE IF NOT EXISTS scanner_detections (
                height INTEGER NOT NULL,
                block_hash BLOB NOT NULL,
                tx_index INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                asset_id TEXT NOT NULL,
                is_flagged INTEGER NOT NULL,
                salt BLOB NOT NULL,
                routing_tag_0 INTEGER NOT NULL,
                routing_tag_1 INTEGER NOT NULL,
                ciphertext_bytes BLOB NOT NULL,
                audit_status TEXT NOT NULL DEFAULT 'pending'
                    CHECK (audit_status IN ('pending', 'evidence_valid', 'evidence_invalid', 'decrypt_failed', 'audit_complete')),
                evidence_object_hash BLOB,
                PRIMARY KEY(height, tx_hash, action_index, output_index)
            );

            CREATE INDEX IF NOT EXISTS idx_scanner_detections_asset_id
                ON scanner_detections(asset_id);

            CREATE INDEX IF NOT EXISTS idx_scanner_detections_is_flagged
                ON scanner_detections(is_flagged);

            CREATE TABLE IF NOT EXISTS scanner_ciphertexts (
                height INTEGER NOT NULL,
                block_hash BLOB NOT NULL,
                tx_index INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                record_type INTEGER NOT NULL CHECK (record_type IN (1, 2, 3)),
                withdrawal_public_data BLOB,
                raw_bytes BLOB NOT NULL,
                compliance_metadata_bytes BLOB,
                screen_status TEXT NOT NULL
                    CHECK (screen_status IN ('pending', 'irrelevant', 'detected', 'invalid')),
                screen_reason TEXT,
                PRIMARY KEY(height, tx_hash, action_index, output_index)
            );

            CREATE INDEX IF NOT EXISTS idx_scanner_ciphertexts_status
                ON scanner_ciphertexts(screen_status);

            CREATE TABLE IF NOT EXISTS scanner_invalid_ciphertexts (
                height INTEGER NOT NULL,
                block_hash BLOB NOT NULL,
                tx_index INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                reason TEXT NOT NULL,
                raw_bytes BLOB NOT NULL,
                PRIMARY KEY(height, tx_hash, action_index, output_index)
            );

            CREATE TABLE IF NOT EXISTS scanner_invalid_ciphertext_summaries (
                height INTEGER PRIMARY KEY,
                block_hash BLOB NOT NULL,
                skipped_count INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS scanner_clear_flows (
                height INTEGER NOT NULL,
                block_hash BLOB NOT NULL,
                tx_index INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                flow_type TEXT NOT NULL,
                asset_id TEXT NOT NULL,
                amount TEXT NOT NULL,
                self_address TEXT,
                counterparty TEXT,
                public_address TEXT,
                PRIMARY KEY(height, tx_hash, action_index, output_index)
            );

            CREATE TABLE IF NOT EXISTS audit_rows (
                height INTEGER NOT NULL,
                block_hash BLOB NOT NULL,
                tx_index INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                flow_type TEXT NOT NULL,
                asset_id TEXT NOT NULL,
                is_flagged INTEGER NOT NULL DEFAULT 0,
                amount TEXT,
                self_address TEXT,
                counterparty_address TEXT,
                public_address TEXT,
                decrypted_via TEXT,
                evidence_object_hash BLOB,
                updated_at_unix INTEGER,
                PRIMARY KEY(height, tx_hash, action_index, output_index)
            );

            CREATE INDEX IF NOT EXISTS idx_audit_rows_flow_type
                ON audit_rows(flow_type);

            CREATE TABLE IF NOT EXISTS audit_address_aliases (
                address TEXT PRIMARY KEY,
                name TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS audit_row_audits (
                height INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                subject TEXT NOT NULL,
                audited_at_unix INTEGER,
                PRIMARY KEY(height, tx_hash, action_index, output_index, subject)
            );

            CREATE TABLE IF NOT EXISTS audit_decryption_failures (
                height INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                branch TEXT NOT NULL,
                reason TEXT NOT NULL,
                failed_at_unix INTEGER,
                PRIMARY KEY(height, tx_hash, action_index, output_index, branch)
            );

            CREATE TABLE IF NOT EXISTS audit_evidence_failures (
                height INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                stage TEXT NOT NULL,
                reason TEXT NOT NULL,
                failed_at_unix INTEGER NOT NULL,
                PRIMARY KEY(height, tx_hash, action_index, output_index, stage)
            );

            CREATE TABLE IF NOT EXISTS audit_orbis_receipts (
                height INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                tier TEXT NOT NULL,
                receipt_json TEXT NOT NULL,
                created_at_unix INTEGER,
                PRIMARY KEY(height, tx_hash, action_index, output_index, tier)
            );

            CREATE TABLE IF NOT EXISTS compliance_evidence_objects (
                object_hash BLOB PRIMARY KEY,
                height INTEGER NOT NULL,
                tx_hash BLOB NOT NULL,
                action_index INTEGER NOT NULL,
                output_index INTEGER NOT NULL,
                object_bytes BLOB NOT NULL,
                created_at_unix INTEGER
            );

            CREATE TABLE IF NOT EXISTS scanner_sync (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                last_height INTEGER NOT NULL,
                last_block_hash BLOB
            );

            CREATE TABLE IF NOT EXISTS scanner_runtime (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER,
                heartbeat_at INTEGER,
                last_error TEXT,
                last_error_at INTEGER
            );

            INSERT OR IGNORE INTO scanner_sync (id, last_height, last_block_hash)
            VALUES (1, 0, NULL);

            INSERT OR IGNORE INTO scanner_runtime (id) VALUES (1);

            "#;
        let schema_id = hex::encode(Sha256::digest(schema_sql.as_bytes()));
        Self::ensure_supported_schema(conn, &schema_id)?;
        conn.execute_batch(schema_sql)?;
        conn.execute(
            "INSERT OR IGNORE INTO scanner_schema (id, identity) VALUES (1, ?1)",
            [&schema_id],
        )?;
        let actual = Self::schema_id(conn)?.context("scanner DB schema identity missing")?;
        anyhow::ensure!(
            actual == schema_id,
            "scanner DB schema identity mismatch; recreate the scanner DB"
        );
        Ok(())
    }

    fn ensure_supported_schema(conn: &Connection, expected: &str) -> Result<()> {
        match Self::schema_id(conn)? {
            Some(actual) => {
                anyhow::ensure!(
                    actual == expected,
                    "scanner DB schema identity mismatch; recreate the scanner DB"
                );
                Ok(())
            }
            None => {
                let user_table_count: i64 = conn.query_row(
                    "SELECT COUNT(*)
                     FROM sqlite_master
                     WHERE type = 'table'
                       AND name NOT LIKE 'sqlite_%'",
                    [],
                    |row| row.get(0),
                )?;
                anyhow::ensure!(
                    user_table_count == 0,
                    "scanner DB schema is unrecognized; recreate the scanner DB"
                );
                Ok(())
            }
        }
    }

    fn schema_id(conn: &Connection) -> Result<Option<String>> {
        let has_schema_table: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'scanner_schema'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        if has_schema_table.is_none() {
            return Ok(None);
        }
        conn.query_row(
            "SELECT identity FROM scanner_schema WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .context("read scanner DB schema identity")
    }

    pub fn invalid_ciphertext_count(&self) -> Result<u64> {
        self.with_read_conn(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM scanner_invalid_ciphertexts",
                [],
                |row| row.get(0),
            )?;
            Ok(count as u64)
        })
    }

    pub fn skipped_invalid_ciphertext_count(&self, height: u64) -> Result<u64> {
        self.with_read_conn(|conn| {
            let count: Option<i64> = conn
                .query_row(
                    "SELECT skipped_count FROM scanner_invalid_ciphertext_summaries WHERE height = ?1",
                    params![height as i64],
                    |row| row.get(0),
                )
                .optional()?;
            Ok(count.unwrap_or_default() as u64)
        })
    }

    pub fn detection_count_sync(&self) -> Result<i64> {
        self.with_read_conn(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM scanner_detections", [], |row| {
                    row.get(0)
                })?;
            Ok(count)
        })
    }

    pub fn mark_scanner_started(&self) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE scanner_runtime
             SET started_at = ?1, heartbeat_at = ?1, last_error = NULL, last_error_at = NULL
             WHERE id = 1",
            params![unix_now_secs()],
        )?;
        Ok(())
    }

    pub fn record_heartbeat(&self) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE scanner_runtime SET heartbeat_at = ?1 WHERE id = 1",
            params![unix_now_secs()],
        )?;
        Ok(())
    }

    pub fn mark_scanner_stopped(&self) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE scanner_runtime SET heartbeat_at = NULL WHERE id = 1",
            [],
        )?;
        Ok(())
    }

    pub fn record_scanner_error(&self, error: &str) -> Result<()> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE scanner_runtime SET last_error = ?1, last_error_at = ?2 WHERE id = 1",
            params![error, unix_now_secs()],
        )?;
        Ok(())
    }

    pub fn scanner_runtime_state(&self) -> Result<ScannerRuntimeState> {
        self.with_read_conn(|conn| {
            let row = conn.query_row(
                "SELECT started_at, heartbeat_at, last_error, last_error_at
                 FROM scanner_runtime WHERE id = 1",
                [],
                |row| {
                    Ok(ScannerRuntimeState {
                        started_at: row.get(0)?,
                        heartbeat_at: row.get(1)?,
                        last_error: row.get(2)?,
                        last_error_at: row.get(3)?,
                    })
                },
            )?;
            Ok(row)
        })
    }

    pub(crate) fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.writer
            .lock()
            .map_err(|e| anyhow!("scanner store writer connection mutex poisoned: {e}"))
    }

    fn with_read_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        if self.db_path.as_os_str() == ":memory:" {
            let conn = self.lock_conn()?;
            return f(&conn);
        }

        let conn = {
            let mut pool = self
                .read_pool
                .lock()
                .map_err(|e| anyhow!("scanner store read pool mutex poisoned: {e}"))?;
            pool.pop()
        }
        .map(Ok)
        .unwrap_or_else(|| Self::open_read_conn(&self.db_path))?;

        let result = f(&conn);

        let mut pool = self
            .read_pool
            .lock()
            .map_err(|e| anyhow!("scanner store read pool mutex poisoned: {e}"))?;
        if pool.len() < READ_POOL_SIZE {
            pool.push(conn);
        }
        result
    }

    fn checkpoint_wal(conn: &Connection) -> Result<()> {
        conn.execute_batch("PRAGMA incremental_vacuum; PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
}

#[async_trait]
impl ScannerStore for SqliteScannerStore {
    async fn last_scanned_block(&self) -> Result<Option<BlockRef>> {
        self.with_read_conn(|conn| {
            let last_height: i64 = conn.query_row(
                "SELECT last_height FROM scanner_sync WHERE id = 1",
                [],
                |row| row.get(0),
            )?;

            if last_height <= 0 {
                return Ok(None);
            }
            conn.query_row(
                "SELECT height, block_hash, parent_hash, block_time_unix FROM scanner_blocks WHERE height = ?1",
                params![last_height],
                block_ref_from_row,
            )
            .optional()
            .with_context(|| format!("read scanner block at height {last_height}"))
        })
    }

    async fn block_by_height(&self, height: u64) -> Result<Option<BlockRef>> {
        self.with_read_conn(|conn| {
            conn.query_row(
                "SELECT height, block_hash, parent_hash, block_time_unix FROM scanner_blocks WHERE height = ?1",
                params![height as i64],
                block_ref_from_row,
            )
            .optional()
            .with_context(|| format!("read scanner block at height {height}"))
        })
    }

    async fn commit_scanned_block(&self, scanned: &ScannedBlock) -> Result<()> {
        let block = &scanned.block;
        anyhow::ensure!(
            block.height > 0 && block.height <= i64::MAX as u64,
            "invalid scanner block height"
        );
        let mut identities = std::collections::HashSet::new();
        for output in &scanned.outputs {
            let output_ref = &output.ciphertext.record_ref.output_ref();
            anyhow::ensure!(
                &output_ref.action.tx.block == block,
                "scanner output block mismatch"
            );
            anyhow::ensure!(
                identities.insert((
                    output_ref.action.tx.tx_hash,
                    output_ref.action.action_index,
                    output_ref.output_index
                )),
                "duplicate scanner output"
            );
            if let OutputOutcome::Detected { event, .. } = &output.outcome {
                anyhow::ensure!(
                    event.record_ref == output.ciphertext.record_ref,
                    "detection output identity mismatch"
                );
            }
        }
        for flow in &scanned.clear_flows {
            anyhow::ensure!(
                &flow.output_ref.action.tx.block == block,
                "clear flow block mismatch"
            );
            let output_ref = &flow.output_ref;
            anyhow::ensure!(
                identities.insert((
                    output_ref.action.tx.tx_hash,
                    output_ref.action.action_index,
                    output_ref.output_index
                )),
                "duplicate scanner output"
            );
        }
        let conn = self.lock_conn()?;
        let tx = conn.unchecked_transaction()?;
        let existing = tx.query_row(
            "SELECT height, block_hash, parent_hash, block_time_unix FROM scanner_blocks WHERE height = ?1",
            params![block.height as i64], block_ref_from_row).optional()?;
        if let Some(existing) = existing {
            anyhow::ensure!(
                existing == *block,
                "scanner block conflict; roll back before replacing a block"
            );
            return Ok(());
        }
        let last_height: i64 = tx.query_row(
            "SELECT last_height FROM scanner_sync WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            block.height as i64 > last_height,
            "scanner cursor cannot move backwards"
        );
        let mut invalid_count = 0usize;

        tx.execute(
            "INSERT OR REPLACE INTO scanner_blocks
             (height, block_hash, parent_hash, block_time_unix, scan_status)
             VALUES (?1, ?2, ?3, ?4, 'committed')",
            params![
                block.height as i64,
                block.block_hash.as_slice(),
                block.parent_hash.as_slice(),
                block.block_time_unix,
            ],
        )?;

        for output in &scanned.outputs {
            let ciphertext = &output.ciphertext;
            let output_ref = &ciphertext.record_ref.output_ref();
            let tx_ref = &output_ref.action.tx;
            tx.execute(
                "INSERT OR IGNORE INTO scanner_ciphertexts
                 (height, block_hash, tx_index, tx_hash, action_index, output_index,
                  raw_bytes, compliance_metadata_bytes, screen_status, screen_reason, record_type, withdrawal_public_data)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, ?10, ?11)",
                params![
                    tx_ref.block.height as i64,
                    tx_ref.block.block_hash.as_slice(),
                    tx_ref.tx_index as i64,
                    tx_ref.tx_hash.as_ref(),
                    output_ref.action.action_index as i64,
                    output_ref.output_index as i64,
                    ciphertext.raw_bytes.as_slice(),
                    ciphertext.metadata_bytes.as_deref(),
                    ScreenStatus::Pending.as_str(),
                    crate::EvidenceObjectType::for_record(&ciphertext.record_ref) as i64,
                    ciphertext.public_withdrawal.as_ref().map(serde_json::to_vec).transpose()?,
                ],
            )?;

            match &output.outcome {
                OutputOutcome::Irrelevant => {
                    update_ciphertext_status(&tx, output_ref, ScreenStatus::Irrelevant, None)?
                }
                OutputOutcome::Invalid { reason } => {
                    let reason = crate::audit::bounded_failure_reason(reason);
                    update_ciphertext_status(
                        &tx,
                        output_ref,
                        ScreenStatus::Invalid,
                        Some(&reason),
                    )?;
                    invalid_count += 1;
                    if invalid_count <= MAX_INVALID_CIPHERTEXTS_PER_BLOCK {
                        tx.execute(
                            "INSERT INTO scanner_invalid_ciphertexts
                             (height, block_hash, tx_index, tx_hash, action_index, output_index, reason, raw_bytes)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            params![block.height as i64, block.block_hash.as_slice(), tx_ref.tx_index as i64,
                                tx_ref.tx_hash.as_ref(), output_ref.action.action_index as i64,
                                output_ref.output_index as i64, reason, ciphertext.raw_bytes.as_slice()],
                        )?;
                    }
                }
                OutputOutcome::Detected { event, evidence } => {
                    update_ciphertext_status(&tx, output_ref, ScreenStatus::Detected, None)?;
                    tx.execute(
                        "INSERT OR IGNORE INTO scanner_detections
                 (height, block_hash, tx_index, tx_hash, action_index, output_index,
                  asset_id, is_flagged, salt,
                  routing_tag_0, routing_tag_1, ciphertext_bytes, audit_status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            tx_ref.block.height as i64,
                            tx_ref.block.block_hash.as_slice(),
                            tx_ref.tx_index as i64,
                            tx_ref.tx_hash.as_ref(),
                            output_ref.action.action_index as i64,
                            output_ref.output_index as i64,
                            event.asset_id.to_string(),
                            if event.is_flagged { 1i64 } else { 0i64 },
                            event.salt.to_bytes().as_slice(),
                            i64::from(event.routing_tags[0]),
                            i64::from(event.routing_tags[1]),
                            event.raw_bytes.as_slice(),
                            AuditStatus::Pending.as_str(),
                        ],
                    )?;
                    tx.execute(
                        "INSERT OR IGNORE INTO audit_rows
                 (height, block_hash, tx_index, tx_hash, action_index, output_index,
                  flow_type, asset_id, is_flagged, amount, self_address, counterparty_address,
                  public_address, decrypted_via, updated_at_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL, ?14)",
                        params![
                            tx_ref.block.height as i64,
                            tx_ref.block.block_hash.as_slice(),
                            tx_ref.tx_index as i64,
                            tx_ref.tx_hash.as_ref(),
                            output_ref.action.action_index as i64,
                            output_ref.output_index as i64,
                            if event.public_withdrawal.is_some() {
                                FLOW_TYPE_WITHDRAW
                            } else {
                                FLOW_TYPE_PRIVATE_TRANSFER
                            },
                            event.asset_id.to_string(),
                            if event.is_flagged { 1i64 } else { 0i64 },
                            event
                                .public_withdrawal
                                .as_ref()
                                .map(|public| public.amount.to_string()),
                            event
                                .public_withdrawal
                                .as_ref()
                                .and_then(|public| public.self_address.as_deref()),
                            event
                                .public_withdrawal
                                .as_ref()
                                .map(|public| public.destination.as_str()),
                            event
                                .public_withdrawal
                                .as_ref()
                                .map(|public| public.destination.as_str()),
                            block.block_time_unix,
                        ],
                    )?;
                    match evidence {
                        CandidateEvidence::Ready(evidence) => {
                            crate::audit::validate_and_save_evidence_tx(&tx, output_ref, evidence)?;
                        }
                        CandidateEvidence::BuildFailure { reason } => {
                            crate::audit::record_evidence_failure_tx(
                                &tx,
                                block.height,
                                tx_ref.tx_hash.as_ref(),
                                output_ref.action.action_index,
                                output_ref.output_index,
                                crate::audit::EVIDENCE_STAGE_BUILD,
                                reason,
                            )?
                        }
                    }
                }
            }
        }

        for event in &scanned.clear_flows {
            let output_ref = &event.output_ref;
            let tx_ref = &output_ref.action.tx;
            let amount = event.amount.to_string();
            tx.execute(
                "INSERT OR IGNORE INTO scanner_clear_flows
                 (height, block_hash, tx_index, tx_hash, action_index, output_index,
                  flow_type, asset_id, amount, self_address, counterparty, public_address)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    tx_ref.block.height as i64,
                    tx_ref.block.block_hash.as_slice(),
                    tx_ref.tx_index as i64,
                    tx_ref.tx_hash.as_ref(),
                    output_ref.action.action_index as i64,
                    output_ref.output_index as i64,
                    event.kind.as_str(),
                    event.asset_id.to_string(),
                    amount.as_str(),
                    event.self_address.as_deref(),
                    event.counterparty.as_deref(),
                    event.public_address.as_deref(),
                ],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO audit_rows
                 (height, block_hash, tx_index, tx_hash, action_index, output_index,
                  flow_type, asset_id, is_flagged, amount, self_address, counterparty_address,
                  public_address, decrypted_via, updated_at_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    tx_ref.block.height as i64,
                    tx_ref.block.block_hash.as_slice(),
                    tx_ref.tx_index as i64,
                    tx_ref.tx_hash.as_ref(),
                    output_ref.action.action_index as i64,
                    output_ref.output_index as i64,
                    event.kind.as_str(),
                    event.asset_id.to_string(),
                    amount.as_str(),
                    event.self_address.as_deref(),
                    event.counterparty.as_deref(),
                    event.public_address.as_deref(),
                    DECRYPTED_VIA_PUBLIC,
                    block.block_time_unix,
                ],
            )?;
        }

        if invalid_count > MAX_INVALID_CIPHERTEXTS_PER_BLOCK {
            tx.execute(
                "INSERT OR REPLACE INTO scanner_invalid_ciphertext_summaries
                 (height, block_hash, skipped_count)
                 VALUES (?1, ?2, ?3)",
                params![
                    block.height as i64,
                    block.block_hash.as_slice(),
                    (invalid_count - MAX_INVALID_CIPHERTEXTS_PER_BLOCK) as i64,
                ],
            )?;
        }

        tx.execute(
            "UPDATE scanner_sync SET last_height = ?1, last_block_hash = ?2 WHERE id = 1",
            params![block.height as i64, block.block_hash.as_slice()],
        )?;

        tx.commit()?;
        Self::checkpoint_wal(&conn)?;
        Ok(())
    }

    async fn rollback_to_height(&self, height: u64) -> Result<()> {
        let conn = self.lock_conn()?;
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM compliance_evidence_objects WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM audit_evidence_failures WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM audit_orbis_receipts WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM audit_decryption_failures WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM audit_row_audits WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM audit_rows WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM scanner_clear_flows WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM scanner_ciphertexts WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM scanner_detections WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM scanner_invalid_ciphertexts WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM scanner_invalid_ciphertext_summaries WHERE height > ?1",
            params![height as i64],
        )?;
        tx.execute(
            "DELETE FROM scanner_blocks WHERE height > ?1",
            params![height as i64],
        )?;

        let last_hash: Option<Vec<u8>> = if height == 0 {
            None
        } else {
            tx.query_row(
                "SELECT block_hash FROM scanner_blocks WHERE height = ?1",
                params![height as i64],
                |row| row.get(0),
            )
            .optional()?
        };

        tx.execute(
            "UPDATE scanner_sync SET last_height = ?1, last_block_hash = ?2 WHERE id = 1",
            params![height as i64, last_hash],
        )?;
        tx.commit()?;
        Self::checkpoint_wal(&conn)?;

        Ok(())
    }

    async fn detection_count(&self) -> Result<u64> {
        self.with_read_conn(|conn| {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM scanner_detections", [], |row| {
                    row.get(0)
                })?;
            Ok(count as u64)
        })
    }

    async fn mark_started(&self) -> Result<()> {
        self.mark_scanner_started()
    }

    async fn heartbeat(&self) -> Result<()> {
        self.record_heartbeat()
    }

    async fn record_error(&self, error: &str) -> Result<()> {
        self.record_scanner_error(error)
    }

    async fn mark_stopped(&self) -> Result<()> {
        self.mark_scanner_stopped()
    }
}

fn update_ciphertext_status(
    tx: &Transaction<'_>,
    output_ref: &OutputRef,
    status: ScreenStatus,
    reason: Option<&str>,
) -> Result<()> {
    let tx_ref = &output_ref.action.tx;
    let current: Option<String> = tx
        .query_row(
            "SELECT screen_status
             FROM scanner_ciphertexts
             WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
            params![
                tx_ref.block.height as i64,
                tx_ref.tx_hash.as_ref(),
                output_ref.action.action_index as i64,
                output_ref.output_index as i64,
            ],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(current) = current {
        ScreenStatus::try_advance(ScreenStatus::from_str(&current)?, status)?;
    }
    let reason = reason.map(crate::audit::bounded_failure_reason);
    tx.execute(
        "UPDATE scanner_ciphertexts
         SET screen_status = ?1, screen_reason = ?2
         WHERE height = ?3 AND tx_hash = ?4 AND action_index = ?5 AND output_index = ?6",
        params![
            status.as_str(),
            reason.as_deref(),
            tx_ref.block.height as i64,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index as i64,
            output_ref.output_index as i64,
        ],
    )?;
    Ok(())
}

fn block_ref_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<BlockRef> {
    let height: i64 = row.get(0)?;
    let block_hash: Vec<u8> = row.get(1)?;
    let parent_hash: Vec<u8> = row.get(2)?;
    let block_time_unix: Option<i64> = row.get(3)?;
    Ok(BlockRef {
        height: height as u64,
        block_hash: vec_to_hash(block_hash).map_err(to_sql_error)?,
        parent_hash: vec_to_hash(parent_hash).map_err(to_sql_error)?,
        block_time_unix,
    })
}

fn vec_to_hash(bytes: Vec<u8>) -> Result<[u8; 32]> {
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow!("stored block hash must be 32 bytes, got {}", bytes.len())
    })
}

fn to_sql_error(error: anyhow::Error) -> rusqlite::Error {
    let error = std::io::Error::new(std::io::ErrorKind::InvalidData, error.to_string());
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Blob, Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{ActionRef, ClearFlowKind, OutputRef, TxRef};
    use shieldd_sdk_asset::asset;
    use shieldd_sdk_txhash::TransactionId;
    use tempfile::NamedTempFile;

    use super::super::types::{
        ClearFlowEvent, DetectionEvent, ExtractedComplianceCiphertext, InvalidCiphertext,
        ScannedOutput,
    };

    fn detected_output(event: DetectionEvent) -> ScannedOutput {
        let ciphertext = ExtractedComplianceCiphertext {
            record_ref: event.record_ref.clone(),
            kind: match event.ciphertext {
                super::super::types::ComplianceCiphertext::Transfer(_) => {
                    super::super::types::ComplianceCiphertextKind::Transfer
                }
                super::super::types::ComplianceCiphertext::Withdrawal(_) => {
                    super::super::types::ComplianceCiphertextKind::Withdrawal
                }
            },
            public_withdrawal: event.public_withdrawal.clone(),
            routing_tags: event.routing_tags,
            raw_bytes: event.raw_bytes.clone(),
            metadata_bytes: None,
        };
        let evidence = CandidateEvidence::from_detection(&event, None);
        ScannedOutput {
            ciphertext,
            outcome: OutputOutcome::Detected { event, evidence },
        }
    }

    fn invalid_output(invalid: InvalidCiphertext) -> ScannedOutput {
        ScannedOutput {
            ciphertext: ExtractedComplianceCiphertext {
                record_ref: invalid.record_ref,
                kind: super::super::types::ComplianceCiphertextKind::Transfer,
                public_withdrawal: None,
                routing_tags: [0, 0],
                raw_bytes: invalid.raw_bytes,
                metadata_bytes: None,
            },
            outcome: OutputOutcome::Invalid {
                reason: invalid.reason,
            },
        }
    }

    fn block(height: u64) -> BlockRef {
        BlockRef {
            height,
            block_hash: [height as u8; 32],
            parent_hash: [height.saturating_sub(1) as u8; 32],
            block_time_unix: Some(height as i64 * 10),
        }
    }

    fn output_ref(height: u64, tx_index: u32, action_index: u32, output_index: u32) -> OutputRef {
        OutputRef {
            action: ActionRef {
                tx: TxRef {
                    block: block(height),
                    tx_index,
                    tx_hash: TransactionId([tx_index as u8; 32]),
                },
                action_index,
            },
            output_index,
        }
    }

    fn invalid(height: u64, output_index: u32) -> InvalidCiphertext {
        InvalidCiphertext {
            record_ref: crate::ComplianceRecordRef::TransferOutput(output_ref(
                height,
                1,
                2,
                output_index,
            )),
            reason: "invalid".to_string(),
            raw_bytes: vec![output_index as u8],
        }
    }

    fn ciphertext(height: u64, output_index: u32) -> ExtractedComplianceCiphertext {
        ExtractedComplianceCiphertext {
            record_ref: crate::ComplianceRecordRef::TransferOutput(output_ref(
                height,
                1,
                2,
                output_index,
            )),
            kind: crate::scanner::types::ComplianceCiphertextKind::Transfer,
            routing_tags: [11, 22],
            raw_bytes: vec![output_index as u8, 9],
            metadata_bytes: Some(vec![8, output_index as u8]),
            public_withdrawal: None,
        }
    }

    fn detection(height: u64) -> DetectionEvent {
        DetectionEvent {
            record_ref: crate::ComplianceRecordRef::TransferOutput(output_ref(height, 1, 2, 3)),
            asset_id: asset::Id(decaf377::Fq::from(123u64)),
            is_flagged: true,
            salt: decaf377::Fq::from(9u64),
            routing_tags: [11, 22],
            ciphertext: crate::scanner::types::ComplianceCiphertext::Transfer(
                crate::transfer::TransferComplianceCiphertext {
                    sender_core_epk: decaf377::Element::GENERATOR,
                    sender_ext_epk: decaf377::Element::GENERATOR,
                    output_core_epk: decaf377::Element::GENERATOR,
                    output_ext_epk: decaf377::Element::GENERATOR,
                    sender_core_c2: decaf377::Fq::from(1u64),
                    sender_ext_c2: decaf377::Fq::from(2u64),
                    output_core_c2: decaf377::Fq::from(3u64),
                    output_ext_c2: decaf377::Fq::from(4u64),
                    sender_core_key_confirmation: decaf377::Fq::from(5u64),
                    output_core_key_confirmation: decaf377::Fq::from(6u64),
                    detection_tag: [0u8; crate::structs::DETECTION_TAG_BYTES],
                    encrypted_sender_core: [0u8; 32],
                    encrypted_sender_ext: [0u8; 96],
                    encrypted_output_core: [0u8; 32],
                    encrypted_output_ext: [0u8; 96],
                },
            ),
            raw_bytes: vec![1, 2, 3],
            public_withdrawal: None,
        }
    }

    #[tokio::test]
    async fn sqlite_store_commits_block_and_detection_atomically() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        let scanner_block = block(10);
        let mut scanned = ScannedBlock::new(scanner_block.clone());
        scanned.outputs.push(detected_output(detection(10)));
        store.commit_scanned_block(&scanned).await.unwrap();

        assert_eq!(store.detection_count().await.unwrap(), 1);
        assert_eq!(
            store.last_scanned_block().await.unwrap(),
            Some(scanner_block)
        );

        let mut scanned = ScannedBlock::new(block(10).clone());
        scanned.outputs.push(detected_output(detection(10)));
        store.commit_scanned_block(&scanned).await.unwrap();
        assert_eq!(store.detection_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn sqlite_store_caps_invalid_ciphertexts_per_block() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        let block = block(20);
        let mut scanned = ScannedBlock::new(block.clone());
        for i in 0..(MAX_INVALID_CIPHERTEXTS_PER_BLOCK as u32 + 7) {
            scanned.outputs.push(invalid_output(invalid(20, i)));
        }
        store.commit_scanned_block(&scanned).await.unwrap();

        assert_eq!(
            store.invalid_ciphertext_count().unwrap(),
            MAX_INVALID_CIPHERTEXTS_PER_BLOCK as u64
        );
        assert_eq!(store.skipped_invalid_ciphertext_count(20).unwrap(), 7);
    }

    #[tokio::test]
    async fn sqlite_store_persists_raw_ciphertext_screening_status() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        let block = block(25);
        let ciphertext = ciphertext(25, 4);
        let mut scanned = ScannedBlock::new(block.clone());
        scanned.outputs.push(ScannedOutput {
            ciphertext: ciphertext.clone(),
            outcome: OutputOutcome::Irrelevant,
        });
        store.commit_scanned_block(&scanned).await.unwrap();

        let conn = store.lock_conn().unwrap();
        let (status, bundle): (String, Vec<u8>) = conn
            .query_row(
                "SELECT screen_status, compliance_metadata_bytes FROM scanner_ciphertexts WHERE height = 25",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "irrelevant");
        assert_eq!(bundle, vec![8, 4]);
    }

    #[tokio::test]
    async fn sqlite_store_rejects_illegal_screen_status_transition() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        let block = block(26);
        let ciphertext = ciphertext(26, 4);
        let mut scanned = ScannedBlock::new(block.clone());
        scanned.outputs.push(ScannedOutput {
            ciphertext: ciphertext.clone(),
            outcome: OutputOutcome::Irrelevant,
        });
        store.commit_scanned_block(&scanned).await.unwrap();

        let conn = store.lock_conn().unwrap();
        let tx = conn.unchecked_transaction().unwrap();
        let err = update_ciphertext_status(
            &tx,
            &ciphertext.record_ref.output_ref(),
            ScreenStatus::Detected,
            None,
        )
        .expect_err("irrelevant ciphertext cannot become detected");

        assert!(
            err.to_string().contains("illegal screen status transition"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn sqlite_store_rejects_unrecognized_db() {
        let temp_file = NamedTempFile::new().unwrap();
        {
            let conn = Connection::open(temp_file.path()).unwrap();
            conn.execute(
                "CREATE TABLE scanner_sync (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    last_height INTEGER NOT NULL,
                    last_block_hash BLOB
                )",
                [],
            )
            .unwrap();
        }

        let err = match SqliteScannerStore::new(temp_file.path()) {
            Ok(_) => panic!("unrecognized scanner DB should fail to open"),
            Err(error) => error,
        };

        assert!(
            err.to_string()
                .contains("scanner DB schema is unrecognized; recreate the scanner DB"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn sqlite_store_initializes_current_schema_and_rejects_wrong_identity() {
        let current_file = NamedTempFile::new().unwrap();
        let current = SqliteScannerStore::new(current_file.path()).unwrap();
        let identity: String = current
            .lock_conn()
            .unwrap()
            .query_row(
                "SELECT identity FROM scanner_schema WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(identity.len(), 64);

        let stale_file = NamedTempFile::new().unwrap();
        {
            let conn = Connection::open(stale_file.path()).unwrap();
            conn.execute_batch(
                "CREATE TABLE scanner_schema (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    identity TEXT NOT NULL
                 );
                 INSERT INTO scanner_schema (id, identity) VALUES (1, 'wrong');",
            )
            .unwrap();
        }
        let error = match SqliteScannerStore::new(stale_file.path()) {
            Ok(_) => panic!("wrong scanner schema should fail to open"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("scanner DB schema identity mismatch"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn sqlite_store_status_constraints_reject_invalid_values() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        let conn = store.lock_conn().unwrap();
        let block_hash = [1u8; 32];
        let tx_hash = [2u8; 32];
        let salt = [3u8; 32];
        let err = conn
            .execute(
                "INSERT INTO scanner_ciphertexts
                 (height, block_hash, tx_index, tx_hash, action_index, output_index,
                  raw_bytes, screen_status, record_type)
                 VALUES (1, ?1, 0, ?2, 0, 0, x'00', 'unknown', 1)",
                params![block_hash.as_slice(), tx_hash.as_slice()],
            )
            .expect_err("invalid screen status should fail");
        assert!(
            err.to_string().contains("CHECK constraint failed"),
            "unexpected error: {err:#}"
        );

        let err = conn
            .execute(
                "INSERT INTO scanner_detections
                 (height, block_hash, tx_index, tx_hash, action_index, output_index,
                  asset_id, is_flagged, salt,
                  routing_tag_0, routing_tag_1, ciphertext_bytes, audit_status)
                 VALUES (1, ?1, 0, ?2, 0, 0, 'asset', 0, ?3, 11, 22, x'00', 'unknown')",
                params![block_hash.as_slice(), tx_hash.as_slice(), salt.as_slice()],
            )
            .expect_err("invalid audit status should fail");
        assert!(
            err.to_string().contains("CHECK constraint failed"),
            "unexpected error: {err:#}"
        );
    }

    #[tokio::test]
    async fn sqlite_store_bounds_invalid_ciphertext_reasons() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        let block = block(27);
        let ciphertext = ciphertext(27, 4);
        let long_reason = "x".repeat(crate::audit::MAX_FAILURE_REASON_BYTES + 100);
        let invalid = InvalidCiphertext {
            record_ref: ciphertext.record_ref.clone(),
            reason: long_reason,
            raw_bytes: vec![1, 2, 3],
        };

        let mut scanned = ScannedBlock::new(block.clone());
        scanned.outputs.push(ScannedOutput {
            ciphertext: ciphertext.clone(),
            outcome: OutputOutcome::Invalid {
                reason: invalid.reason.clone(),
            },
        });
        store.commit_scanned_block(&scanned).await.unwrap();

        let conn = store.lock_conn().unwrap();
        let (screen_reason, invalid_reason): (String, String) = conn
            .query_row(
                "SELECT c.screen_reason, i.reason
                 FROM scanner_ciphertexts c
                 JOIN scanner_invalid_ciphertexts i
                   ON i.height = c.height
                  AND i.tx_hash = c.tx_hash
                  AND i.action_index = c.action_index
                  AND i.output_index = c.output_index
                 WHERE c.height = 27",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert!(screen_reason.len() <= crate::audit::MAX_FAILURE_REASON_BYTES);
        assert!(invalid_reason.len() <= crate::audit::MAX_FAILURE_REASON_BYTES);
        assert!(screen_reason.ends_with("[truncated]"));
        assert!(invalid_reason.ends_with("[truncated]"));
    }

    #[tokio::test]
    async fn sqlite_store_enables_wal_and_bounded_checkpointing() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();

        {
            let conn = store.lock_conn().unwrap();
            let journal_mode: String = conn
                .pragma_query_value(None, "journal_mode", |row| row.get(0))
                .unwrap();
            let synchronous: i64 = conn
                .pragma_query_value(None, "synchronous", |row| row.get(0))
                .unwrap();
            let auto_vacuum: i64 = conn
                .pragma_query_value(None, "auto_vacuum", |row| row.get(0))
                .unwrap();
            let wal_autocheckpoint: i64 = conn
                .pragma_query_value(None, "wal_autocheckpoint", |row| row.get(0))
                .unwrap();

            assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
            assert_eq!(synchronous, 1);
            assert_eq!(auto_vacuum, 2);
            assert_eq!(wal_autocheckpoint, WAL_AUTOCHECKPOINT_PAGES);
        }

        for height in 1..=5 {
            let block = block(height);
            let mut scanned = ScannedBlock::new(block.clone());
            scanned.outputs.push(detected_output(detection(height)));
            store.commit_scanned_block(&scanned).await.unwrap();
        }

        let wal_path = PathBuf::from(format!("{}-wal", temp_file.path().display()));
        let wal_size = std::fs::metadata(wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(wal_size < 1024 * 1024, "WAL file grew to {wal_size} bytes");
    }

    #[tokio::test]
    async fn sqlite_store_allows_concurrent_readers_during_writes() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = Arc::new(SqliteScannerStore::new(temp_file.path()).unwrap());

        let mut readers = Vec::new();
        for _ in 0..READ_POOL_SIZE {
            let store = Arc::clone(&store);
            readers.push(tokio::spawn(async move {
                for _ in 0..50 {
                    store.detection_count().await.unwrap();
                    store.last_scanned_block().await.unwrap();
                }
            }));
        }

        for height in 1..=20 {
            let block = block(height);
            let mut scanned = ScannedBlock::new(block.clone());
            scanned.outputs.push(detected_output(detection(height)));
            store.commit_scanned_block(&scanned).await.unwrap();
        }

        for reader in readers {
            reader.await.unwrap();
        }

        assert_eq!(store.detection_count().await.unwrap(), 20);
    }

    #[tokio::test]
    async fn sqlite_store_rolls_back_later_scanner_state() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        for height in 1..=3 {
            let block = block(height);
            let mut scanned = ScannedBlock::new(block.clone());
            scanned.outputs.push(detected_output(detection(height)));
            scanned.outputs.push(invalid_output(invalid(height, 0)));
            store.commit_scanned_block(&scanned).await.unwrap();
        }

        store.rollback_to_height(1).await.unwrap();
        assert_eq!(store.last_scanned_block().await.unwrap(), Some(block(1)));
        assert_eq!(store.detection_count().await.unwrap(), 1);
        assert!(store.block_by_height(2).await.unwrap().is_none());
        assert_eq!(store.invalid_ciphertext_count().unwrap(), 1);

        let conn = store.lock_conn().unwrap();
        let audit_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM audit_rows", [], |row| row.get(0))
            .unwrap();
        assert_eq!(audit_count, 1);
    }

    #[tokio::test]
    async fn sqlite_store_commits_empty_block_advances_cursor() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        let empty = block(42);
        let scanned = ScannedBlock::new(empty.clone());
        store.commit_scanned_block(&scanned).await.unwrap();

        assert_eq!(store.last_scanned_block().await.unwrap(), Some(empty));
        assert_eq!(store.detection_count().await.unwrap(), 0);

        let conn = store.lock_conn().unwrap();
        let ciphertexts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scanner_ciphertexts WHERE height = 42",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let clear_flows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM scanner_clear_flows WHERE height = 42",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ciphertexts, 0);
        assert_eq!(clear_flows, 0);
    }

    fn clear_flow(height: u64, kind: ClearFlowKind, output_index: u32) -> ClearFlowEvent {
        ClearFlowEvent {
            output_ref: output_ref(height, 1, 2, output_index),
            kind,
            asset_id: asset::Id(decaf377::Fq::from(7u64)),
            amount: shieldd_sdk_num::Amount::from(100u64),
            self_address: Some("shieldd1self".to_string()),
            counterparty: Some("shieldd1counter".to_string()),
            public_address: None,
        }
    }

    #[tokio::test]
    async fn sqlite_store_projects_clear_shield_and_withdraw_to_audit_rows() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();
        let block = block(50);
        let mut scanned = ScannedBlock::new(block.clone());
        scanned
            .clear_flows
            .push(clear_flow(50, ClearFlowKind::Shield, 1));
        scanned
            .clear_flows
            .push(clear_flow(50, ClearFlowKind::Withdraw, 2));
        store.commit_scanned_block(&scanned).await.unwrap();

        let conn = store.lock_conn().unwrap();
        let flow_rows: Vec<(String, String)> = conn
            .prepare(
                "SELECT flow_type, asset_id FROM scanner_clear_flows WHERE height = 50 ORDER BY output_index",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(flow_rows.len(), 2);
        assert_eq!(flow_rows[0].0, "shield");
        assert_eq!(flow_rows[1].0, "withdraw");

        let audit_rows: Vec<(String, Option<String>)> = conn
            .prepare(
                "SELECT flow_type, decrypted_via FROM audit_rows WHERE height = 50 ORDER BY output_index",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(audit_rows.len(), 2);
        assert_eq!(audit_rows[0].0, "shield");
        assert_eq!(audit_rows[1].0, "withdraw");
        assert_eq!(audit_rows[0].1.as_deref(), Some(DECRYPTED_VIA_PUBLIC));
        assert_eq!(audit_rows[1].1.as_deref(), Some(DECRYPTED_VIA_PUBLIC));
    }

    #[tokio::test]
    async fn sqlite_store_records_runtime_started_heartbeat_and_error() {
        let temp_file = NamedTempFile::new().unwrap();
        let store = SqliteScannerStore::new(temp_file.path()).unwrap();

        let initial = store.scanner_runtime_state().unwrap();
        assert!(initial.started_at.is_none());
        assert!(initial.heartbeat_at.is_none());
        assert!(!initial.is_active(unix_now_secs()));

        store.mark_scanner_started().unwrap();
        let after_start = store.scanner_runtime_state().unwrap();
        assert!(after_start.started_at.is_some());
        assert!(after_start.heartbeat_at.is_some());
        assert!(after_start.is_active(unix_now_secs()));

        store.record_scanner_error("boom").unwrap();
        let after_err = store.scanner_runtime_state().unwrap();
        assert_eq!(after_err.last_error.as_deref(), Some("boom"));
        assert!(after_err.last_error_at.is_some());

        let stale_now = unix_now_secs() + HEARTBEAT_STALE_SECS + 5;
        assert!(!after_err.is_active(stale_now));

        store.mark_scanner_stopped().unwrap();
        let after_stop = store.scanner_runtime_state().unwrap();
        assert!(after_stop.heartbeat_at.is_none());
        assert!(!after_stop.is_active(unix_now_secs()));
    }
}
