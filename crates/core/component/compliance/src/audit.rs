use anyhow::{anyhow, Context, Result};
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use rusqlite::{params, OptionalExtension};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::audit_records::{
    classify_orbis_import_row, detected_ref_from_row_parts, AuditDetectedRef, AuditImportRow,
    AuditScanExport, DetectedRefRowParts, OrbisAuditEntry, OrbisImportEligibility,
};
use crate::scanner::storage::SqliteScannerStore;
use crate::scanner::types::{
    AuditLedgerRow, AUDIT_STATUS_AUDIT_COMPLETE, AUDIT_STATUS_DECRYPT_FAILED,
    AUDIT_STATUS_EVIDENCE_INVALID, AUDIT_STATUS_EVIDENCE_VALID, AUDIT_STATUS_PENDING,
    DECRYPTED_VIA_ISSUER_DK, DECRYPTED_VIA_ORBIS_PRE, FLOW_TYPE_PRIVATE_TRANSFER,
};
use crate::scanning::decrypt_full_flagged;
use crate::transfer::TransferComplianceCiphertext;
use crate::{
    validate_audit_evidence, AuditValidationInput, AuditValidationStatus, ComplianceEvidenceObject,
    DetectionKey, OutputRef, TransferOrbisUploadBundle,
};

pub const EVIDENCE_STAGE_BUILD: &str = "build_evidence";
pub const EVIDENCE_STAGE_VALIDATE: &str = "validate_evidence";
pub const EVIDENCE_STAGE_UPLOAD_BUNDLE: &str = "validate_upload_bundle";
pub const EVIDENCE_STAGE_ORBIS_IMPORT: &str = "validate_orbis_import";

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
           AND d.audit_status IN (?2, ?3)
           AND a.flow_type = ?1
           AND a.amount IS NULL",
    )?;
    let pending = rows
        .query_map(
            params![
                FLOW_TYPE_PRIVATE_TRANSFER,
                AUDIT_STATUS_EVIDENCE_VALID,
                AUDIT_STATUS_DECRYPT_FAILED
            ],
            |row| {
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
            },
        )?
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
                tx.execute(
                    "UPDATE scanner_detections
                     SET audit_status = ?1
                     WHERE height = ?2
                       AND tx_hash = ?3
                       AND action_index = ?4
                       AND output_index = ?5",
                    params![
                        AUDIT_STATUS_AUDIT_COMPLETE,
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
                Ok(detected_ref_from_row_parts(DetectedRefRowParts {
                    height: height as u64,
                    tx_hash,
                    action_index: action_index as u32,
                    output_index: output_index as u32,
                    asset_id,
                    is_flagged: is_flagged != 0,
                    flow_type,
                }))
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
        let row = row_status.map(|(audit_status, is_flagged)| AuditImportRow {
            audit_status,
            is_flagged: is_flagged != 0,
        });
        match classify_orbis_import_row(row) {
            OrbisImportEligibility::Eligible => {}
            OrbisImportEligibility::Ineligible { reason } => {
                record_evidence_failure_tx(
                    &tx,
                    entry.height,
                    tx_hash.as_slice(),
                    entry.action_index,
                    entry.output_index,
                    EVIDENCE_STAGE_ORBIS_IMPORT,
                    &reason,
                )?;
                continue;
            }
        }
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
            tx.execute(
                "UPDATE scanner_detections
                 SET audit_status = ?1
                 WHERE height = ?2
                   AND tx_hash = ?3
                   AND action_index = ?4
                   AND output_index = ?5",
                params![
                    AUDIT_STATUS_AUDIT_COMPLETE,
                    entry.height as i64,
                    tx_hash.as_slice(),
                    entry.action_index as i64,
                    entry.output_index as i64,
                ],
            )?;
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

pub fn record_evidence_failure(
    store: &SqliteScannerStore,
    output_ref: &OutputRef,
    stage: &str,
    reason: &str,
) -> Result<()> {
    let tx_ref = &output_ref.action.tx;
    let conn = store.lock_conn()?;
    let tx = conn.unchecked_transaction()?;
    record_evidence_failure_tx(
        &tx,
        tx_ref.block.height,
        tx_ref.tx_hash.as_ref(),
        output_ref.action.action_index,
        output_ref.output_index,
        stage,
        reason,
    )?;
    tx.commit()?;
    Ok(())
}

pub fn validate_and_save_evidence_object(
    store: &SqliteScannerStore,
    evidence: &ComplianceEvidenceObject,
    upload_bundle: &TransferOrbisUploadBundle,
    ring_pk: &decaf377::Element,
) -> Result<[u8; 32]> {
    let output_ref = &evidence.output_ref;
    let tx_ref = &output_ref.action.tx;
    let conn = store.lock_conn()?;
    let tx = conn.unchecked_transaction()?;

    if let Err(error) = evidence.validate_payload_hash() {
        record_evidence_failure_tx(
            &tx,
            tx_ref.block.height,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index,
            output_ref.output_index,
            EVIDENCE_STAGE_VALIDATE,
            &error.to_string(),
        )?;
        tx.commit()?;
        return Err(error);
    }

    let persisted_raw_bytes: Option<Vec<u8>> = tx
        .query_row(
            "SELECT raw_bytes
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
    let transfer_bytes = evidence.transfer_ciphertext.to_bytes();
    if persisted_raw_bytes.as_deref() != Some(transfer_bytes.as_slice()) {
        let reason = "evidence ciphertext does not match persisted scanner ciphertext";
        record_evidence_failure_tx(
            &tx,
            tx_ref.block.height,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index,
            output_ref.output_index,
            EVIDENCE_STAGE_VALIDATE,
            reason,
        )?;
        tx.commit()?;
        anyhow::bail!(reason);
    }

    let persisted_upload_bundle_bytes: Option<Vec<u8>> = tx
        .query_row(
            "SELECT orbis_upload_bundle_bytes
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
        .optional()?
        .flatten();
    let upload_bundle_bytes = upload_bundle.to_bytes()?;
    if persisted_upload_bundle_bytes.as_deref() != Some(upload_bundle_bytes.as_slice()) {
        let reason = "evidence upload bundle does not match persisted scanner upload bundle";
        record_evidence_failure_tx(
            &tx,
            tx_ref.block.height,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index,
            output_ref.output_index,
            EVIDENCE_STAGE_UPLOAD_BUNDLE,
            reason,
        )?;
        tx.commit()?;
        anyhow::bail!(reason);
    }

    let detected: Option<(String, i64, Vec<u8>)> = tx
        .query_row(
            "SELECT asset_id, is_flagged, salt
             FROM scanner_detections
             WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
            params![
                tx_ref.block.height as i64,
                tx_ref.tx_hash.as_ref(),
                output_ref.action.action_index as i64,
                output_ref.output_index as i64,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let detected_matches = detected.is_some_and(|(asset_id, is_flagged, salt)| {
        asset_id == evidence.asset_id.to_string()
            && (is_flagged != 0) == evidence.is_flagged
            && salt == evidence.detection_salt.to_bytes()
    });
    if !detected_matches {
        let reason = "evidence asset, flag, or salt does not match scanner detection";
        record_evidence_failure_tx(
            &tx,
            tx_ref.block.height,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index,
            output_ref.output_index,
            EVIDENCE_STAGE_VALIDATE,
            reason,
        )?;
        tx.commit()?;
        anyhow::bail!(reason);
    }

    match validate_audit_evidence(AuditValidationInput {
        evidence: evidence.clone(),
        upload_bundle: Some(upload_bundle.clone()),
        ring_pk: *ring_pk,
    }) {
        AuditValidationStatus::Valid => {}
        AuditValidationStatus::MissingUploadBundle => {
            let reason = "missing upload bundle";
            record_evidence_failure_tx(
                &tx,
                tx_ref.block.height,
                tx_ref.tx_hash.as_ref(),
                output_ref.action.action_index,
                output_ref.output_index,
                EVIDENCE_STAGE_UPLOAD_BUNDLE,
                reason,
            )?;
            tx.commit()?;
            anyhow::bail!(reason);
        }
        AuditValidationStatus::InvalidEvidence(reason) => {
            record_evidence_failure_tx(
                &tx,
                tx_ref.block.height,
                tx_ref.tx_hash.as_ref(),
                output_ref.action.action_index,
                output_ref.output_index,
                EVIDENCE_STAGE_VALIDATE,
                &reason,
            )?;
            tx.commit()?;
            anyhow::bail!(reason);
        }
        AuditValidationStatus::InvalidOrbisPackage(reason) => {
            record_evidence_failure_tx(
                &tx,
                tx_ref.block.height,
                tx_ref.tx_hash.as_ref(),
                output_ref.action.action_index,
                output_ref.output_index,
                EVIDENCE_STAGE_UPLOAD_BUNDLE,
                &reason,
            )?;
            tx.commit()?;
            anyhow::bail!(reason);
        }
    }

    let object_hash = evidence.object_hash();
    let object_bytes = evidence.to_bytes();
    tx.execute(
        "INSERT OR REPLACE INTO compliance_evidence_objects
         (object_hash, height, tx_hash, action_index, output_index, object_bytes, created_at_unix)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            object_hash.as_slice(),
            tx_ref.block.height as i64,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index as i64,
            output_ref.output_index as i64,
            object_bytes.as_slice(),
            now_unix(),
        ],
    )?;
    tx.execute(
        "UPDATE scanner_detections
             SET evidence_object_hash = ?1,
                 audit_status = CASE
                     WHEN audit_status = ?6 THEN ?6
                     WHEN audit_status = ?8 THEN ?8
                     ELSE ?7
                 END
         WHERE height = ?2 AND tx_hash = ?3 AND action_index = ?4 AND output_index = ?5",
        params![
            object_hash.as_slice(),
            tx_ref.block.height as i64,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index as i64,
            output_ref.output_index as i64,
            AUDIT_STATUS_AUDIT_COMPLETE,
            AUDIT_STATUS_EVIDENCE_VALID,
            AUDIT_STATUS_DECRYPT_FAILED,
        ],
    )?;
    tx.execute(
        "UPDATE audit_rows
         SET evidence_object_hash = ?1
         WHERE height = ?2 AND tx_hash = ?3 AND action_index = ?4 AND output_index = ?5",
        params![
            object_hash.as_slice(),
            tx_ref.block.height as i64,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index as i64,
            output_ref.output_index as i64,
        ],
    )?;
    tx.execute(
        "DELETE FROM audit_evidence_failures
         WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
        params![
            tx_ref.block.height as i64,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index as i64,
            output_ref.output_index as i64,
        ],
    )?;
    tx.commit()?;
    Ok(object_hash)
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
            Ok(detected_ref_from_row_parts(DetectedRefRowParts {
                height: height as u64,
                tx_hash,
                action_index: action_index as u32,
                output_index: output_index as u32,
                asset_id,
                is_flagged: is_flagged != 0,
                flow_type,
            }))
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
    tx.execute(
        "UPDATE scanner_detections
         SET audit_status = ?1
         WHERE height = ?2 AND tx_hash = ?3 AND action_index = ?4 AND output_index = ?5
           AND audit_status IN (?6, ?7)",
        params![
            AUDIT_STATUS_DECRYPT_FAILED,
            height as i64,
            tx_hash,
            action_index as i64,
            output_index as i64,
            AUDIT_STATUS_EVIDENCE_VALID,
            AUDIT_STATUS_DECRYPT_FAILED,
        ],
    )?;
    Ok(())
}

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
    use crate::scanner::{DetectionEvent, ExtractedComplianceCiphertext, ScannerStore};

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

    #[tokio::test]
    async fn evidence_object_is_persisted_by_hash() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
        persist_evidence_detection(&store, &evidence, &bundle, false).await;
        let object_hash =
            validate_and_save_evidence_object(&store, &evidence, &bundle, &ring_pk).unwrap();

        let conn = store.lock_conn().unwrap();
        let stored_len: i64 = conn
            .query_row(
                "SELECT length(object_bytes) FROM compliance_evidence_objects WHERE object_hash = ?1",
                params![object_hash.as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_len as usize, evidence.to_bytes().len());
        drop(conn);
        assert_eq!(audit_status(&store, &evidence), AUDIT_STATUS_EVIDENCE_VALID);
    }

    #[tokio::test]
    async fn evidence_object_rejects_mismatched_persisted_ciphertext() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
        persist_evidence_detection(&store, &evidence, &bundle, true).await;

        let error =
            validate_and_save_evidence_object(&store, &evidence, &bundle, &ring_pk).unwrap_err();
        assert!(error
            .to_string()
            .contains("evidence ciphertext does not match persisted scanner ciphertext"));

        let conn = store.lock_conn().unwrap();
        let (status, reason): (String, String) = conn
            .query_row(
                "SELECT d.audit_status, f.reason
                 FROM scanner_detections d
                 JOIN audit_evidence_failures f
                   ON f.height = d.height
                  AND f.tx_hash = d.tx_hash
                  AND f.action_index = d.action_index
                  AND f.output_index = d.output_index",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, AUDIT_STATUS_EVIDENCE_INVALID);
        assert!(reason.contains("persisted scanner ciphertext"));
    }

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

    #[tokio::test]
    async fn flagged_decrypt_requires_valid_evidence() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let (evidence, bundle, _ring_pk) = crate::evidence::tests::valid_evidence_fixture();
        persist_evidence_detection(&store, &evidence, &bundle, false).await;
        let conn = store.lock_conn().unwrap();
        conn.execute(
            "UPDATE scanner_detections SET is_flagged = 1
             WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
            params![
                evidence.output_ref.action.tx.block.height as i64,
                evidence.output_ref.action.tx.tx_hash.as_ref(),
                evidence.output_ref.action.action_index as i64,
                evidence.output_ref.output_index as i64,
            ],
        )
        .unwrap();
        conn.execute(
            "UPDATE audit_rows SET is_flagged = 1
             WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
            params![
                evidence.output_ref.action.tx.block.height as i64,
                evidence.output_ref.action.tx.tx_hash.as_ref(),
                evidence.output_ref.action.action_index as i64,
                evidence.output_ref.output_index as i64,
            ],
        )
        .unwrap();
        drop(conn);

        assert_eq!(
            decrypt_flagged_rows(&store, &DetectionKey::demo()).unwrap(),
            0
        );
        assert_eq!(audit_status(&store, &evidence), AUDIT_STATUS_PENDING);
    }

    #[tokio::test]
    async fn rollback_removes_evidence_objects_and_failures() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let (evidence, bundle, ring_pk) = crate::evidence::tests::valid_evidence_fixture();
        persist_evidence_detection(&store, &evidence, &bundle, false).await;
        validate_and_save_evidence_object(&store, &evidence, &bundle, &ring_pk).unwrap();
        record_evidence_failure(
            &store,
            &evidence.output_ref,
            EVIDENCE_STAGE_BUILD,
            "synthetic failure after valid evidence",
        )
        .unwrap();

        store
            .rollback_to_height(evidence.output_ref.action.tx.block.height - 1)
            .await
            .unwrap();

        let conn = store.lock_conn().unwrap();
        for table in [
            "compliance_evidence_objects",
            "audit_evidence_failures",
            "scanner_detections",
            "audit_rows",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} should be empty after rollback");
        }
    }

    async fn persist_evidence_detection(
        store: &SqliteScannerStore,
        evidence: &ComplianceEvidenceObject,
        bundle: &TransferOrbisUploadBundle,
        tamper_ciphertext: bool,
    ) {
        let block = evidence.output_ref.action.tx.block.clone();
        let mut raw_bytes = evidence.transfer_ciphertext.to_bytes();
        if tamper_ciphertext {
            raw_bytes[0] ^= 1;
        }
        store.begin_block(&block).await.unwrap();
        store
            .save_ciphertext(&ExtractedComplianceCiphertext {
                output_ref: evidence.output_ref.clone(),
                raw_bytes,
                upload_bundle_bytes: Some(bundle.to_bytes().unwrap()),
            })
            .await
            .unwrap();
        store
            .save_detection(&DetectionEvent {
                output_ref: evidence.output_ref.clone(),
                asset_id: evidence.asset_id,
                is_flagged: evidence.is_flagged,
                salt: evidence.detection_salt,
                ciphertext: evidence.transfer_ciphertext.clone(),
                raw_bytes: evidence.transfer_ciphertext.to_bytes(),
            })
            .await
            .unwrap();
        store.commit_block(&block).await.unwrap();
    }

    fn audit_status(store: &SqliteScannerStore, evidence: &ComplianceEvidenceObject) -> String {
        let conn = store.lock_conn().unwrap();
        conn.query_row(
            "SELECT audit_status FROM scanner_detections
             WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
            params![
                evidence.output_ref.action.tx.block.height as i64,
                evidence.output_ref.action.tx.tx_hash.as_ref(),
                evidence.output_ref.action.action_index as i64,
                evidence.output_ref.output_index as i64,
            ],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn orbis_entry(evidence: &ComplianceEvidenceObject) -> OrbisAuditEntry {
        OrbisAuditEntry {
            height: evidence.output_ref.action.tx.block.height,
            tx_hash: hex::encode(evidence.output_ref.action.tx.tx_hash.as_ref()),
            action_index: evidence.output_ref.action.action_index,
            output_index: evidence.output_ref.output_index,
            amount: "1234".to_string(),
            self_address: "receiver".to_string(),
            counterparty: "sender".to_string(),
            decrypted_via: DECRYPTED_VIA_ORBIS_PRE.to_string(),
        }
    }
}
