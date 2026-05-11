use anyhow::{anyhow, Context, Result};
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::scanner::storage::SqliteScannerStore;
use crate::scanner::types::{
    AuditLedgerRow, DECRYPTED_VIA_ISSUER_DK, DECRYPTED_VIA_ORBIS_PRE, FLOW_TYPE_PRIVATE_TRANSFER,
};
use crate::scanning::decrypt_full_flagged;
use crate::transfer::TransferComplianceCiphertext;
use crate::DetectionKey;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditDetectedRef {
    pub height: u64,
    pub tx_hash: String,
    pub action_index: u32,
    #[serde(default)]
    pub output_index: u32,
    pub asset_id: String,
    pub is_flagged: bool,
    #[serde(default = "private_transfer_flow_type")]
    pub flow_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditScanExport {
    pub scan_info: serde_json::Value,
    pub detected: Vec<AuditDetectedRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrbisAuditEntry {
    pub height: u64,
    pub tx_hash: String,
    pub action_index: u32,
    #[serde(default)]
    pub output_index: u32,
    pub amount: String,
    pub self_address: String,
    pub counterparty: String,
    pub decrypted_via: String,
}

fn private_transfer_flow_type() -> String {
    FLOW_TYPE_PRIVATE_TRANSFER.to_string()
}

pub fn record_address_alias(store: &SqliteScannerStore, address: &str, name: &str) -> Result<()> {
    let conn = store.lock_conn()?;
    conn.execute(
        "INSERT OR REPLACE INTO audit_address_aliases (address, name) VALUES (?1, ?2)",
        params![address, name],
    )?;

    if let Ok(parsed) = Address::from_str(address) {
        conn.execute(
            "INSERT OR REPLACE INTO audit_address_aliases (address, name) VALUES (?1, ?2)",
            params![hex::encode(parsed.transmission_key().0), name],
        )?;
    }

    Ok(())
}

pub fn mark_row_audited(
    store: &SqliteScannerStore,
    height: u64,
    tx_hash_hex: &str,
    action_index: u32,
    output_index: u32,
    subject: &str,
) -> Result<()> {
    let tx_hash = decode_tx_hash(tx_hash_hex)?;
    let conn = store.lock_conn()?;
    conn.execute(
        "INSERT OR IGNORE INTO audit_row_audits
         (height, tx_hash, action_index, output_index, subject, audited_at_unix)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            height as i64,
            tx_hash.as_slice(),
            action_index as i64,
            output_index as i64,
            subject,
            now_unix(),
        ],
    )?;
    Ok(())
}

pub fn decrypt_flagged_rows(store: &SqliteScannerStore, dk: &DetectionKey) -> Result<u64> {
    let conn = store.lock_conn()?;
    let tx = conn.unchecked_transaction()?;
    let mut rows = tx.prepare(
        "SELECT d.height, d.tx_hash, d.action_index, d.output_index, d.asset_id, d.ciphertext_bytes
         FROM scanner_detections d
         JOIN audit_rows a
           ON a.height = d.height
          AND a.tx_hash = d.tx_hash
          AND a.action_index = d.action_index
          AND a.output_index = d.output_index
         WHERE d.is_flagged = 1
           AND a.flow_type = ?1
           AND a.amount IS NULL",
    )?;
    let pending = rows
        .query_map(params![FLOW_TYPE_PRIVATE_TRANSFER], |row| {
            let height: i64 = row.get(0)?;
            let tx_hash: Vec<u8> = row.get(1)?;
            let action_index: i64 = row.get(2)?;
            let output_index: i64 = row.get(3)?;
            let asset_id: String = row.get(4)?;
            let ciphertext_bytes: Vec<u8> = row.get(5)?;
            Ok((
                height as u64,
                tx_hash,
                action_index as u32,
                output_index as u32,
                asset_id,
                ciphertext_bytes,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(rows);

    let mut updated = 0u64;
    for (height, tx_hash, action_index, output_index, asset_id, ciphertext_bytes) in pending {
        let asset_id: asset::Id = asset_id
            .parse()
            .with_context(|| format!("parse detected asset id {asset_id}"))?;
        let ciphertext = TransferComplianceCiphertext::from_bytes(&ciphertext_bytes)?;
        match decrypt_full_flagged(dk.inner(), &ciphertext, asset_id) {
            Ok(Some(data)) => {
                tx.execute(
                    "UPDATE audit_rows
                     SET amount = ?1,
                         self_address = ?2,
                         counterparty_address = ?3,
                         decrypted_via = ?4,
                         updated_at_unix = ?5
                     WHERE height = ?6
                       AND tx_hash = ?7
                       AND action_index = ?8
                       AND output_index = ?9",
                    params![
                        data.amount.value().to_string(),
                        hex::encode(data.receiver_address.transmission_key),
                        hex::encode(data.sender_address.transmission_key),
                        DECRYPTED_VIA_ISSUER_DK,
                        now_unix(),
                        height as i64,
                        tx_hash.as_slice(),
                        action_index as i64,
                        output_index as i64,
                    ],
                )?;
                updated += 1;
            }
            Ok(None) => {
                record_failure_tx(
                    &tx,
                    height,
                    &tx_hash,
                    action_index,
                    output_index,
                    DECRYPTED_VIA_ISSUER_DK,
                    "ciphertext was not flagged",
                )?;
            }
            Err(error) => {
                record_failure_tx(
                    &tx,
                    height,
                    &tx_hash,
                    action_index,
                    output_index,
                    DECRYPTED_VIA_ISSUER_DK,
                    &error.to_string(),
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(updated)
}

pub fn export_orbis_pending_scan(store: &SqliteScannerStore) -> Result<AuditScanExport> {
    let detected = export_detected_refs(store)?
        .into_iter()
        .filter(|row| !row.is_flagged && row.flow_type == FLOW_TYPE_PRIVATE_TRANSFER)
        .collect::<Vec<_>>();
    Ok(AuditScanExport {
        scan_info: scan_info(store)?,
        detected,
    })
}

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
        let changed = tx.execute(
            "UPDATE audit_rows
             SET amount = ?1,
                 self_address = CASE
                     WHEN self_address IS NULL OR self_address = '' THEN ?2
                     ELSE self_address
                 END,
                 counterparty_address = CASE
                     WHEN ?3 != '' THEN ?3
                     ELSE counterparty_address
                 END,
                 decrypted_via = ?4,
                 updated_at_unix = ?5
             WHERE height = ?6
               AND tx_hash = ?7
               AND action_index = ?8
               AND output_index = ?9",
            params![
                entry.amount,
                entry.self_address,
                entry.counterparty,
                DECRYPTED_VIA_ORBIS_PRE,
                now_unix(),
                entry.height as i64,
                tx_hash.as_slice(),
                entry.action_index as i64,
                entry.output_index as i64,
            ],
        )?;
        if changed > 0 {
            updated += 1;
            if let Some(subject) = subject {
                tx.execute(
                    "INSERT OR IGNORE INTO audit_row_audits
                     (height, tx_hash, action_index, output_index, subject, audited_at_unix)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        entry.height as i64,
                        tx_hash.as_slice(),
                        entry.action_index as i64,
                        entry.output_index as i64,
                        subject,
                        now_unix(),
                    ],
                )?;
            }
        }
    }
    tx.commit()?;
    Ok(updated)
}

pub fn export_detected_refs(store: &SqliteScannerStore) -> Result<Vec<AuditDetectedRef>> {
    let conn = store.lock_conn()?;
    let mut rows = conn.prepare(
        "SELECT height, tx_hash, action_index, output_index, asset_id, is_flagged, ?1
         FROM scanner_detections
         UNION ALL
         SELECT height, tx_hash, action_index, output_index, asset_id, 0, flow_type
         FROM scanner_clear_flows
         ORDER BY height, tx_hash, action_index, output_index",
    )?;
    let refs = rows
        .query_map(params![FLOW_TYPE_PRIVATE_TRANSFER], |row| {
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
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(refs)
}

pub fn export_scan_json(store: &SqliteScannerStore) -> Result<serde_json::Value> {
    Ok(serde_json::to_value(AuditScanExport {
        scan_info: scan_info(store)?,
        detected: export_detected_refs(store)?,
    })?)
}

pub fn export_ledger_rows_json(store: &SqliteScannerStore) -> Result<serde_json::Value> {
    Ok(serde_json::Value::Array(
        export_ledger_rows(store)?
            .into_iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()?,
    ))
}

pub fn export_ledger_rows(store: &SqliteScannerStore) -> Result<Vec<AuditLedgerRow>> {
    let conn = store.lock_conn()?;
    let mut rows = conn.prepare(
        "SELECT a.height,
                a.block_hash,
                a.tx_index,
                a.tx_hash,
                a.action_index,
                a.output_index,
                a.flow_type,
                a.asset_id,
                a.is_flagged,
                a.amount,
                a.self_address,
                self_alias.name,
                a.counterparty_address,
                counterparty_alias.name,
                a.public_address,
                a.decrypted_via
         FROM audit_rows a
         LEFT JOIN audit_address_aliases self_alias
           ON self_alias.address = a.self_address
         LEFT JOIN audit_address_aliases counterparty_alias
           ON counterparty_alias.address = a.counterparty_address
         ORDER BY a.height, a.tx_hash, a.action_index, a.output_index",
    )?;

    let mut ledger = Vec::new();
    let mapped = rows.query_map([], |row| {
        let height: i64 = row.get(0)?;
        let block_hash: Vec<u8> = row.get(1)?;
        let tx_index: i64 = row.get(2)?;
        let tx_hash: Vec<u8> = row.get(3)?;
        let action_index: i64 = row.get(4)?;
        let output_index: i64 = row.get(5)?;
        let is_flagged: i64 = row.get(8)?;
        Ok(AuditLedgerRow {
            height: height as u64,
            block_hash_hex: hex::encode(block_hash),
            tx_index: tx_index as u32,
            tx_hash_hex: hex::encode(tx_hash),
            action_index: action_index as u32,
            output_index: output_index as u32,
            flow_type: row.get(6)?,
            asset_id: row.get(7)?,
            is_flagged: is_flagged != 0,
            amount: row.get(9)?,
            self_address: row.get(10)?,
            self_alias: row.get(11)?,
            counterparty_address: row.get(12)?,
            counterparty_alias: row.get(13)?,
            public_address: row.get(14)?,
            decrypted_via: row.get(15)?,
            audited_subjects: Vec::new(),
        })
    })?;
    for row in mapped {
        let mut row = row?;
        row.audited_subjects = audited_subjects(
            &conn,
            row.height,
            &decode_tx_hash(&row.tx_hash_hex)?,
            row.action_index,
            row.output_index,
        )?;
        ledger.push(row);
    }
    Ok(ledger)
}

pub fn scanner_health_json(store: &SqliteScannerStore) -> Result<serde_json::Value> {
    let conn = store.lock_conn()?;
    let (last_height, last_hash): (i64, Option<Vec<u8>>) = conn.query_row(
        "SELECT last_height, last_block_hash FROM scanner_sync WHERE id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(serde_json::json!({
        "healthy": true,
        "message": "Scanner running",
        "last_height": last_height,
        "last_block_hash": last_hash.map(hex::encode),
        "updatedAt": now_unix(),
    }))
}

fn scan_info(store: &SqliteScannerStore) -> Result<serde_json::Value> {
    let conn = store.lock_conn()?;
    let (last_height, detection_count): (i64, i64) = conn.query_row(
        "SELECT s.last_height, (SELECT COUNT(*) FROM scanner_detections) FROM scanner_sync s WHERE s.id = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(serde_json::json!({
        "scan_time": now_unix(),
        "last_height": last_height,
        "detected_count": detection_count,
    }))
}

fn audited_subjects(
    conn: &rusqlite::Connection,
    height: u64,
    tx_hash: &[u8],
    action_index: u32,
    output_index: u32,
) -> Result<Vec<String>> {
    let mut rows = conn.prepare(
        "SELECT subject FROM audit_row_audits
         WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4
         ORDER BY subject",
    )?;
    let subjects = rows
        .query_map(
            params![
                height as i64,
                tx_hash,
                action_index as i64,
                output_index as i64
            ],
            |row| row.get(0),
        )?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(subjects)
}

fn record_failure_tx(
    tx: &rusqlite::Transaction<'_>,
    height: u64,
    tx_hash: &[u8],
    action_index: u32,
    output_index: u32,
    branch: &str,
    reason: &str,
) -> Result<()> {
    tx.execute(
        "INSERT OR REPLACE INTO audit_decryption_failures
         (height, tx_hash, action_index, output_index, branch, reason, failed_at_unix)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            height as i64,
            tx_hash,
            action_index as i64,
            output_index as i64,
            branch,
            reason,
            now_unix(),
        ],
    )?;
    Ok(())
}

fn decode_tx_hash(tx_hash_hex: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(tx_hash_hex).context("decode transaction hash")?;
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow!("transaction hash must be 32 bytes, got {}", bytes.len()))
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_records_transmission_key_for_penumbra_address() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let address = crate::test_helpers::make_address(88);
        record_address_alias(&store, &address.to_string(), "Alice").unwrap();

        let conn = store.lock_conn().unwrap();
        let alias: String = conn
            .query_row(
                "SELECT name FROM audit_address_aliases WHERE address = ?1",
                params![hex::encode(address.transmission_key().0)],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(alias, "Alice");
    }

    #[test]
    fn empty_store_exports_stable_scan_shape() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let scan = export_scan_json(&store).unwrap();
        assert!(scan.get("scan_info").is_some());
        assert_eq!(scan.get("detected").unwrap().as_array().unwrap().len(), 0);
    }
}
