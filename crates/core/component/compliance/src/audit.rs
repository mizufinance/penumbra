use anyhow::{anyhow, Context, Result};
use rusqlite::{params, OptionalExtension};
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::Address;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::audit_records::{
    detected_ref_from_row_parts, AuditDetectedRef, AuditScanExport, DetectedRefRowParts,
};
use crate::audit_status::{AuditStatus, DecryptedVia, FlowType};
use crate::scanner::storage::SqliteScannerStore;
use crate::scanner::types::AuditLedgerRow;
#[cfg(test)]
use crate::scanner::types::{AUDIT_STATUS_EVIDENCE_INVALID, AUDIT_STATUS_EVIDENCE_VALID};
use crate::scanning::decrypt_full_flagged;
use crate::transfer::TransferComplianceCiphertext;
use crate::{
    validate_audit_evidence, AuditValidationInput, AuditValidationStatus, ComplianceEvidenceObject,
    DetectionKey, OutputRef,
};

pub const EVIDENCE_STAGE_BUILD: &str = "build_evidence";
pub const EVIDENCE_STAGE_VALIDATE: &str = "validate_evidence";
pub const EVIDENCE_STAGE_METADATA: &str = "validate_metadata";
pub(crate) const MAX_FAILURE_REASON_BYTES: usize = 1024;

const FAILURE_TRUNCATION_SUFFIX: &str = "...[truncated]";

pub(crate) fn bounded_failure_reason(reason: &str) -> String {
    if reason.len() <= MAX_FAILURE_REASON_BYTES {
        return reason.to_owned();
    }
    let mut end = MAX_FAILURE_REASON_BYTES - FAILURE_TRUNCATION_SUFFIX.len();
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &reason[..end], FAILURE_TRUNCATION_SUFFIX)
}

fn validate_audit_status_transitions(transitions: &[(AuditStatus, AuditStatus)]) -> Result<()> {
    for (from, to) in transitions {
        AuditStatus::try_advance(*from, *to)?;
    }
    Ok(())
}

fn update_audit_status_tx(
    tx: &rusqlite::Transaction<'_>,
    height: u64,
    tx_hash: &[u8],
    action_index: u32,
    output_index: u32,
    to: AuditStatus,
    allowed_current: &[AuditStatus],
) -> Result<usize> {
    anyhow::ensure!(
        !allowed_current.is_empty() && allowed_current.len() <= 3,
        "audit status helper supports one to three current statuses"
    );
    for from in allowed_current {
        AuditStatus::try_advance(*from, to)?;
    }
    let height = height as i64;
    let action_index = action_index as i64;
    let output_index = output_index as i64;
    let changed = match allowed_current {
        [a] => tx.execute(
            "UPDATE scanner_detections
             SET audit_status = ?1
             WHERE height = ?2 AND tx_hash = ?3 AND action_index = ?4 AND output_index = ?5
               AND audit_status IN (?6)",
            params![
                to.as_str(),
                height,
                tx_hash,
                action_index,
                output_index,
                a.as_str(),
            ],
        )?,
        [a, b] => tx.execute(
            "UPDATE scanner_detections
             SET audit_status = ?1
             WHERE height = ?2 AND tx_hash = ?3 AND action_index = ?4 AND output_index = ?5
               AND audit_status IN (?6, ?7)",
            params![
                to.as_str(),
                height,
                tx_hash,
                action_index,
                output_index,
                a.as_str(),
                b.as_str(),
            ],
        )?,
        [a, b, c] => tx.execute(
            "UPDATE scanner_detections
             SET audit_status = ?1
             WHERE height = ?2 AND tx_hash = ?3 AND action_index = ?4 AND output_index = ?5
               AND audit_status IN (?6, ?7, ?8)",
            params![
                to.as_str(),
                height,
                tx_hash,
                action_index,
                output_index,
                a.as_str(),
                b.as_str(),
                c.as_str(),
            ],
        )?,
        _ => unreachable!("allowed_current length checked above"),
    };
    Ok(changed)
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
           AND d.audit_status IN (?2, ?3)
           AND a.flow_type = ?1
           AND a.amount IS NULL",
    )?;
    let pending = rows
        .query_map(
            params![
                FlowType::PrivateTransfer.as_str(),
                AuditStatus::EvidenceValid.as_str(),
                AuditStatus::DecryptFailed.as_str()
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

    validate_audit_status_transitions(&[
        (AuditStatus::EvidenceValid, AuditStatus::AuditComplete),
        (AuditStatus::DecryptFailed, AuditStatus::AuditComplete),
    ])?;

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
                        DecryptedVia::IssuerDetectionKey.as_str(),
                        now_unix(),
                        height as i64,
                        tx_hash.as_slice(),
                        action_index as i64,
                        output_index as i64,
                    ],
                )?;
                update_audit_status_tx(
                    &tx,
                    height,
                    tx_hash.as_slice(),
                    action_index,
                    output_index,
                    AuditStatus::AuditComplete,
                    &[AuditStatus::EvidenceValid, AuditStatus::DecryptFailed],
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
                    DecryptedVia::IssuerDetectionKey.as_str(),
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
                    DecryptedVia::IssuerDetectionKey.as_str(),
                    &error.to_string(),
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

struct DetectedEvidenceFacts {
    asset_id: String,
    is_flagged: bool,
    salt: Vec<u8>,
}

pub(crate) enum EvidencePersistence {
    Valid([u8; 32]),
    Invalid(String),
}

struct PersistedCiphertext {
    block_hash: Vec<u8>,
    tx_index: u32,
    raw_bytes: Vec<u8>,
    metadata_bytes: Option<Vec<u8>>,
}

struct PersistedEvidenceFacts {
    ciphertext: Option<PersistedCiphertext>,
    detection: Option<DetectedEvidenceFacts>,
}

struct EvidenceValidationFailure {
    stage: &'static str,
    reason: String,
}

fn classify_evidence_for_persistence(
    evidence: &ComplianceEvidenceObject,
    facts: &PersistedEvidenceFacts,
) -> Result<Option<EvidenceValidationFailure>> {
    if let Err(error) = evidence.validate_payload_hash() {
        return Ok(Some(EvidenceValidationFailure {
            stage: EVIDENCE_STAGE_VALIDATE,
            reason: error.to_string(),
        }));
    }

    let transfer_bytes = evidence.transfer_ciphertext.to_bytes();
    let Some(ciphertext) = &facts.ciphertext else {
        return Ok(Some(EvidenceValidationFailure {
            stage: EVIDENCE_STAGE_VALIDATE,
            reason: "missing persisted scanner ciphertext".to_owned(),
        }));
    };
    if ciphertext.block_hash != evidence.output_ref.action.tx.block.block_hash
        || ciphertext.tx_index != evidence.output_ref.action.tx.tx_index
    {
        return Ok(Some(EvidenceValidationFailure {
            stage: EVIDENCE_STAGE_VALIDATE,
            reason: "evidence block or transaction identity mismatch".to_owned(),
        }));
    }
    if ciphertext.raw_bytes != transfer_bytes {
        return Ok(Some(EvidenceValidationFailure {
            stage: EVIDENCE_STAGE_VALIDATE,
            reason: "evidence ciphertext does not match persisted scanner ciphertext".to_owned(),
        }));
    }

    let metadata_bytes = evidence.metadata.to_bytes()?;
    if ciphertext.metadata_bytes.as_deref() != Some(metadata_bytes.as_slice()) {
        return Ok(Some(EvidenceValidationFailure {
            stage: EVIDENCE_STAGE_METADATA,
            reason: "evidence metadata does not match persisted scanner metadata".to_owned(),
        }));
    }

    let detected_matches = facts.detection.as_ref().is_some_and(|detected| {
        detected.asset_id == evidence.asset_id.to_string()
            && detected.is_flagged == evidence.is_flagged
            && detected.salt == evidence.detection_salt.to_bytes()
    });
    if !detected_matches {
        return Ok(Some(EvidenceValidationFailure {
            stage: EVIDENCE_STAGE_VALIDATE,
            reason: "evidence asset, flag, or salt does not match scanner detection".to_owned(),
        }));
    }

    let status = validate_audit_evidence(AuditValidationInput {
        evidence: evidence.clone(),
    });
    let failure = match status {
        AuditValidationStatus::Valid => None,
        AuditValidationStatus::InvalidEvidence(reason) => Some(EvidenceValidationFailure {
            stage: EVIDENCE_STAGE_VALIDATE,
            reason,
        }),
    };
    Ok(failure)
}

pub fn validate_and_save_evidence_object(
    store: &SqliteScannerStore,
    evidence: &ComplianceEvidenceObject,
) -> Result<[u8; 32]> {
    let conn = store.lock_conn()?;
    let tx = conn.unchecked_transaction()?;
    let outcome = validate_and_save_evidence_tx(&tx, &evidence.output_ref, evidence)?;
    tx.commit()?;
    match outcome {
        EvidencePersistence::Valid(hash) => Ok(hash),
        EvidencePersistence::Invalid(reason) => anyhow::bail!(reason),
    }
}

pub(crate) fn validate_and_save_evidence_tx(
    tx: &rusqlite::Transaction<'_>,
    output_ref: &OutputRef,
    evidence: &ComplianceEvidenceObject,
) -> Result<EvidencePersistence> {
    let tx_ref = &output_ref.action.tx;

    let ciphertext: Option<PersistedCiphertext> = tx.query_row(
        "SELECT block_hash, tx_index, raw_bytes, compliance_metadata_bytes FROM scanner_ciphertexts
         WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
        params![tx_ref.block.height as i64, tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index as i64, output_ref.output_index as i64],
        |row| Ok(PersistedCiphertext { block_hash: row.get(0)?, tx_index: row.get(1)?,
            raw_bytes: row.get(2)?, metadata_bytes: row.get(3)? }),
    ).optional()?;

    let detected: Option<DetectedEvidenceFacts> = tx
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
            |row| {
                Ok(DetectedEvidenceFacts {
                    asset_id: row.get(0)?,
                    is_flagged: row.get(1)?,
                    salt: row.get(2)?,
                })
            },
        )
        .optional()?;
    let facts = PersistedEvidenceFacts {
        ciphertext,
        detection: detected,
    };
    let failure = if evidence.output_ref != *output_ref {
        Some(EvidenceValidationFailure {
            stage: EVIDENCE_STAGE_VALIDATE,
            reason: "evidence output identity mismatch".to_owned(),
        })
    } else {
        match classify_evidence_for_persistence(evidence, &facts) {
            Ok(failure) => failure,
            Err(error) => Some(EvidenceValidationFailure {
                stage: EVIDENCE_STAGE_VALIDATE,
                reason: error.to_string(),
            }),
        }
    };
    if let Some(failure) = failure {
        record_evidence_failure_tx(
            &tx,
            tx_ref.block.height,
            tx_ref.tx_hash.as_ref(),
            output_ref.action.action_index,
            output_ref.output_index,
            failure.stage,
            &failure.reason,
        )?;
        return Ok(EvidencePersistence::Invalid(failure.reason));
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
    update_audit_status_tx(
        &tx,
        tx_ref.block.height,
        tx_ref.tx_hash.as_ref(),
        output_ref.action.action_index,
        output_ref.output_index,
        AuditStatus::EvidenceValid,
        &[
            AuditStatus::Pending,
            AuditStatus::EvidenceInvalid,
            AuditStatus::EvidenceValid,
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
    Ok(EvidencePersistence::Valid(object_hash))
}

pub fn export_detected_refs(store: &SqliteScannerStore) -> Result<Vec<AuditDetectedRef>> {
    let conn = store.lock_conn()?;
    let mut rows = conn.prepare(
        "SELECT height, tx_hash, action_index, output_index, asset_id, is_flagged,
                routing_tag_0, routing_tag_1, ?1
         FROM scanner_detections
         UNION ALL
         SELECT height, tx_hash, action_index, output_index, asset_id, 0,
                NULL, NULL, flow_type
         FROM scanner_clear_flows
         ORDER BY height, tx_hash, action_index, output_index",
    )?;
    let refs = rows
        .query_map(params![FlowType::PrivateTransfer.as_str()], |row| {
            let height: i64 = row.get(0)?;
            let tx_hash: Vec<u8> = row.get(1)?;
            let action_index: i64 = row.get(2)?;
            let output_index: i64 = row.get(3)?;
            let asset_id: String = row.get(4)?;
            let is_flagged: i64 = row.get(5)?;
            let routing_tag_0: Option<i64> = row.get(6)?;
            let routing_tag_1: Option<i64> = row.get(7)?;
            let flow_type: String = row.get(8)?;
            let routing_tags = routing_tag_0
                .zip(routing_tag_1)
                .map(|(first, second)| [first as u32, second as u32]);
            Ok(detected_ref_from_row_parts(DetectedRefRowParts {
                height: height as u64,
                tx_hash,
                action_index: action_index as u32,
                output_index: output_index as u32,
                asset_id,
                is_flagged: is_flagged != 0,
                routing_tags,
                flow_type: FlowType::from_str(&flow_type).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        8,
                        rusqlite::types::Type::Text,
                        error.into(),
                    )
                })?,
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

/// Health summary for the compliance scanner.
///
/// `active` is heartbeat-derived: `true` only when the worker has processed a
/// block within `HEARTBEAT_STALE_SECS`. It reflects recent progress, not
/// process liveness — a halted chain looks the same as a dead scanner. On
/// clean exit (catch-up completes, or `run()` returns Ok) the heartbeat is
/// cleared so `active` flips to `false` immediately.
pub fn scanner_health_json(store: &SqliteScannerStore) -> Result<serde_json::Value> {
    let (last_height, last_hash): (i64, Option<Vec<u8>>) = {
        let conn = store.lock_conn()?;
        conn.query_row(
            "SELECT last_height, last_block_hash FROM scanner_sync WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?
    };
    let runtime = store.scanner_runtime_state()?;
    let detection_count = store.detection_count_sync()?;
    let now = now_unix();
    let active = runtime.is_active(now);
    let message = if let Some(err) = runtime.last_error.as_deref() {
        format!("Scanner failed: {err}")
    } else if active {
        "Scanner active".to_string()
    } else if runtime.started_at.is_some() {
        "Scanner stopped".to_string()
    } else {
        "Scanner not started".to_string()
    };
    Ok(serde_json::json!({
        "healthy": true,
        "message": message,
        "active": active,
        "started_at": runtime.started_at,
        "heartbeat_at": runtime.heartbeat_at,
        "heartbeat_stale_secs": crate::scanner::HEARTBEAT_STALE_SECS,
        "last_error": runtime.last_error,
        "last_error_at": runtime.last_error_at,
        "last_height": last_height,
        "last_block_hash": last_hash.map(hex::encode),
        "detection_count": detection_count,
        "updatedAt": now,
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
    let reason = bounded_failure_reason(reason);
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
            reason.as_str(),
            now_unix(),
        ],
    )?;
    update_audit_status_tx(
        tx,
        height,
        tx_hash,
        action_index,
        output_index,
        AuditStatus::DecryptFailed,
        &[AuditStatus::EvidenceValid, AuditStatus::DecryptFailed],
    )?;
    Ok(())
}

pub(crate) fn record_evidence_failure_tx(
    tx: &rusqlite::Transaction<'_>,
    height: u64,
    tx_hash: &[u8],
    action_index: u32,
    output_index: u32,
    stage: &str,
    reason: &str,
) -> Result<()> {
    let reason = bounded_failure_reason(reason);
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
            reason.as_str(),
            now_unix(),
        ],
    )?;
    update_audit_status_tx(
        tx,
        height,
        tx_hash,
        action_index,
        output_index,
        AuditStatus::EvidenceInvalid,
        &[AuditStatus::Pending, AuditStatus::EvidenceInvalid],
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
    fn alias_records_transmission_key_for_shieldd_address() {
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

    #[test]
    fn persisted_audit_failure_reasons_are_bounded() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let conn = store.lock_conn().unwrap();
        let tx_hash = [7u8; 32];
        let long_reason = "x".repeat(MAX_FAILURE_REASON_BYTES + 100);
        let tx = conn.unchecked_transaction().unwrap();

        record_failure_tx(&tx, 1, &tx_hash, 2, 3, "issuer_dk", &long_reason).unwrap();
        record_evidence_failure_tx(&tx, 1, &tx_hash, 2, 3, EVIDENCE_STAGE_BUILD, &long_reason)
            .unwrap();
        tx.commit().unwrap();

        let decryption_reason: String = conn
            .query_row("SELECT reason FROM audit_decryption_failures", [], |row| {
                row.get(0)
            })
            .unwrap();
        let evidence_reason: String = conn
            .query_row("SELECT reason FROM audit_evidence_failures", [], |row| {
                row.get(0)
            })
            .unwrap();

        assert!(decryption_reason.len() <= MAX_FAILURE_REASON_BYTES);
        assert!(evidence_reason.len() <= MAX_FAILURE_REASON_BYTES);
        assert!(decryption_reason.ends_with("[truncated]"));
        assert!(evidence_reason.ends_with("[truncated]"));
    }

    #[tokio::test]
    async fn evidence_object_is_persisted_by_hash() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let (evidence, metadata) = crate::evidence::tests::valid_evidence_fixture();
        persist_evidence_detection(&store, &evidence, &metadata, false).await;
        let object_hash = validate_and_save_evidence_object(&store, &evidence).unwrap();

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
        let (evidence, metadata) = crate::evidence::tests::valid_evidence_fixture();
        persist_evidence_detection(&store, &evidence, &metadata, true).await;

        let error = validate_and_save_evidence_object(&store, &evidence).unwrap_err();
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
    async fn flagged_decrypt_requires_valid_evidence() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let (evidence, metadata) = crate::evidence::tests::valid_evidence_fixture();
        persist_evidence_detection(&store, &evidence, &metadata, true).await;
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
        assert_eq!(
            audit_status(&store, &evidence),
            AUDIT_STATUS_EVIDENCE_INVALID
        );
    }

    #[tokio::test]
    async fn rollback_removes_evidence_objects_and_failures() {
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let (evidence, metadata) = crate::evidence::tests::valid_evidence_fixture();
        persist_evidence_detection(&store, &evidence, &metadata, false).await;
        validate_and_save_evidence_object(&store, &evidence).unwrap();
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
        metadata: &crate::TransferComplianceMetadata,
        tamper_ciphertext: bool,
    ) {
        store
            .commit_scanned_block(&scanned_evidence(evidence, metadata, tamper_ciphertext))
            .await
            .unwrap();
    }

    fn scanned_evidence(
        evidence: &ComplianceEvidenceObject,
        metadata: &crate::TransferComplianceMetadata,
        tamper_ciphertext: bool,
    ) -> crate::scanner::ScannedBlock {
        let block = evidence.output_ref.action.tx.block.clone();
        let mut raw_bytes = evidence.transfer_ciphertext.to_bytes();
        if tamper_ciphertext {
            raw_bytes[0] ^= 1;
        }
        let event = DetectionEvent {
            output_ref: evidence.output_ref.clone(),
            asset_id: evidence.asset_id,
            is_flagged: evidence.is_flagged,
            salt: evidence.detection_salt,
            routing_tags: [11, 22],
            ciphertext: evidence.transfer_ciphertext.clone(),
            raw_bytes: evidence.transfer_ciphertext.to_bytes(),
        };
        crate::scanner::ScannedBlock {
            block,
            outputs: vec![crate::scanner::ScannedOutput {
                ciphertext: ExtractedComplianceCiphertext {
                    output_ref: evidence.output_ref.clone(),
                    routing_tags: [11, 22],
                    raw_bytes,
                    metadata_bytes: Some(metadata.to_bytes().unwrap()),
                },
                outcome: crate::scanner::OutputOutcome::Detected {
                    event,
                    evidence: crate::scanner::CandidateEvidence::Ready(evidence.clone()),
                },
            }],
            clear_flows: vec![],
        }
    }

    #[tokio::test]
    async fn committed_detection_has_evidence_after_restart() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let (evidence, metadata) = crate::evidence::tests::valid_evidence_fixture();
        {
            let store = SqliteScannerStore::new(file.path()).unwrap();
            persist_evidence_detection(&store, &evidence, &metadata, false).await;
            assert_eq!(
                store.last_scanned_block().await.unwrap(),
                Some(evidence.output_ref.action.tx.block.clone()),
            );
        }
        let reopened = SqliteScannerStore::new(file.path()).unwrap();
        assert_eq!(
            audit_status(&reopened, &evidence),
            AUDIT_STATUS_EVIDENCE_VALID
        );
    }

    #[tokio::test]
    async fn persistence_failure_rolls_back_results_evidence_and_cursor() {
        for (table, operation) in [
            ("scanner_ciphertexts", "INSERT"),
            ("scanner_detections", "INSERT"),
            ("compliance_evidence_objects", "INSERT"),
            ("scanner_sync", "UPDATE"),
        ] {
            let file = tempfile::NamedTempFile::new().unwrap();
            let (evidence, metadata) = crate::evidence::tests::valid_evidence_fixture();
            let scanned = scanned_evidence(&evidence, &metadata, false);
            {
                let store = SqliteScannerStore::new(file.path()).unwrap();
                store
                    .lock_conn()
                    .unwrap()
                    .execute_batch(&format!(
                        "CREATE TRIGGER fail_persistence BEFORE {operation} ON {table}
                     BEGIN SELECT RAISE(ABORT, 'injected persistence failure'); END;"
                    ))
                    .unwrap();
                assert!(store.commit_scanned_block(&scanned).await.is_err());
                assert!(store.last_scanned_block().await.unwrap().is_none());
                let conn = store.lock_conn().unwrap();
                for table in [
                    "scanner_blocks",
                    "scanner_ciphertexts",
                    "scanner_detections",
                    "compliance_evidence_objects",
                    "audit_evidence_failures",
                    "audit_rows",
                ] {
                    let count: i64 = conn
                        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                            row.get(0)
                        })
                        .unwrap();
                    assert_eq!(count, 0, "{table} survived rollback");
                }
                conn.execute_batch("DROP TRIGGER fail_persistence").unwrap();
            }
            let reopened = SqliteScannerStore::new(file.path()).unwrap();
            assert!(reopened.last_scanned_block().await.unwrap().is_none());
            reopened.commit_scanned_block(&scanned).await.unwrap();
            reopened.commit_scanned_block(&scanned).await.unwrap();
            assert_eq!(reopened.detection_count().await.unwrap(), 1);
            assert_eq!(
                audit_status(&reopened, &evidence),
                AUDIT_STATUS_EVIDENCE_VALID
            );
        }
    }

    #[tokio::test]
    async fn malformed_candidate_commits_invalid_outcome_alongside_valid_output() {
        use crate::scanner::OutputOutcome;
        let store = SqliteScannerStore::new(":memory:").unwrap();
        let (evidence, metadata) = crate::evidence::tests::valid_evidence_fixture();
        let mut scanned = scanned_evidence(&evidence, &metadata, false);
        let mut malformed = scanned.outputs[0].clone();
        malformed.ciphertext.output_ref.output_index += 1;
        if let OutputOutcome::Detected { event, .. } = &mut malformed.outcome {
            event.output_ref = malformed.ciphertext.output_ref.clone();
        }
        scanned.outputs.push(malformed);
        store.commit_scanned_block(&scanned).await.unwrap();
        assert_eq!(audit_status(&store, &evidence), AUDIT_STATUS_EVIDENCE_VALID);
        let invalid: i64 = store
            .lock_conn()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM scanner_detections WHERE audit_status = 'evidence_invalid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(invalid, 1);
        assert_eq!(
            store.last_scanned_block().await.unwrap(),
            Some(scanned.block)
        );
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
}
