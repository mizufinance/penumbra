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
    record_evidence_failure_tx(&tx, 1, &tx_hash, 2, 3, EVIDENCE_STAGE_BUILD, &long_reason).unwrap();
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
    let output_ref = evidence.output_ref();
    persist_evidence_detection(&store, &evidence, &metadata, true).await;
    let conn = store.lock_conn().unwrap();
    conn.execute(
        "UPDATE scanner_detections SET is_flagged = 1
         WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
        params![
            output_ref.action.tx.block.height as i64,
            output_ref.action.tx.tx_hash.as_ref(),
            output_ref.action.action_index as i64,
            output_ref.output_index as i64,
        ],
    )
    .unwrap();
    conn.execute(
        "UPDATE audit_rows SET is_flagged = 1
         WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
        params![
            output_ref.action.tx.block.height as i64,
            output_ref.action.tx.tx_hash.as_ref(),
            output_ref.action.action_index as i64,
            output_ref.output_index as i64,
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
        &evidence.output_ref(),
        EVIDENCE_STAGE_BUILD,
        "synthetic failure after valid evidence",
    )
    .unwrap();

    store
        .rollback_to_height(evidence.output_ref().action.tx.block.height - 1)
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

#[tokio::test]
async fn withdrawal_scanning_preserves_public_asset_and_completes_flagged_audit() {
    use crate::scanner::{
        screener::{ComplianceScreener, ScreeningResult},
        types::{ComplianceCiphertext, ComplianceCiphertextKind, PublicWithdrawalData},
    };
    for host in [true, false] {
        for flagged in [true, false] {
            let store = SqliteScannerStore::new(":memory:").unwrap();
            let (transfer, _) = crate::evidence::tests::valid_evidence_fixture();
            let asset_id = transfer.asset_id;
            let record_ref = if host {
                crate::ComplianceRecordRef::HostWithdrawal(transfer.output_ref().action)
            } else {
                crate::ComplianceRecordRef::Ics20Withdrawal(transfer.output_ref().action)
            };
            let sender = crate::test_helpers::make_address(42);
            let key = if flagged {
                DetectionKey::demo().public_key()
            } else {
                decaf377::Element::GENERATOR
            };
            let ciphertext = crate::encrypt_withdrawal(rand_core::OsRng, key, &sender)
                .unwrap()
                .ciphertext;
            let public = PublicWithdrawalData {
                asset_id,
                amount: 123u64.into(),
                self_address: None,
                destination: "destination".to_owned(),
            };
            let extracted = ExtractedComplianceCiphertext {
                record_ref: record_ref.clone(),
                kind: ComplianceCiphertextKind::Withdrawal,
                routing_tags: [11, 0],
                raw_bytes: ciphertext.to_bytes().to_vec(),
                metadata_bytes: None,
                public_withdrawal: Some(public.clone()),
            };
            let other_asset = asset::Id(asset_id.0 + decaf377::Fq::from(1u64));
            assert!(matches!(
                ComplianceScreener::new(DetectionKey::demo(), other_asset)
                    .screen(extracted.clone()),
                ScreeningResult::Irrelevant
            ));
            let ScreeningResult::Detected(event) =
                ComplianceScreener::new(DetectionKey::demo(), asset_id).screen(extracted.clone())
            else {
                panic!("public withdrawal must be detected")
            };
            assert_eq!(event.asset_id, asset_id);
            assert_eq!(event.is_flagged, flagged);
            assert!(matches!(
                event.ciphertext,
                ComplianceCiphertext::Withdrawal(_)
            ));
            let evidence = ComplianceEvidenceObject::new_withdrawal(
                record_ref,
                asset_id,
                flagged,
                ciphertext,
                crate::WithdrawalEvidencePublicData {
                    amount: public.amount,
                    self_address: public.self_address,
                    destination: public.destination,
                },
            )
            .unwrap();
            let block = evidence.output_ref().action.tx.block;
            store
                .commit_scanned_block(&crate::scanner::ScannedBlock {
                    block,
                    outputs: vec![crate::scanner::ScannedOutput {
                        ciphertext: extracted,
                        outcome: crate::scanner::OutputOutcome::Detected {
                            event,
                            evidence: crate::scanner::CandidateEvidence::Ready(evidence.clone()),
                        },
                    }],
                    clear_flows: vec![],
                })
                .await
                .unwrap();
            assert_eq!(audit_status(&store, &evidence), AUDIT_STATUS_EVIDENCE_VALID);
            assert_eq!(
                decrypt_flagged_rows(&store, &DetectionKey::demo()).unwrap(),
                u64::from(flagged)
            );
            assert_eq!(
                audit_status(&store, &evidence),
                if flagged {
                    crate::scanner::types::AUDIT_STATUS_AUDIT_COMPLETE
                } else {
                    AUDIT_STATUS_EVIDENCE_VALID
                }
            );
            let mut changed_public = evidence.withdrawal.clone().unwrap();
            changed_public.amount = 999u64.into();
            let crate::ComplianceEvidenceCiphertext::Withdrawal(ct) = &evidence.ciphertext else {
                panic!("withdrawal fixture expected")
            };
            let changed = ComplianceEvidenceObject::new_withdrawal(
                evidence.record_ref.clone(),
                asset_id,
                flagged,
                ct.clone(),
                changed_public,
            )
            .unwrap();
            assert!(
                validate_and_save_evidence_object(&store, &changed).is_err(),
                "evidence must bind the public withdrawal amount"
            );
            let swapped = match &evidence.record_ref {
                crate::ComplianceRecordRef::HostWithdrawal(action) => {
                    crate::ComplianceRecordRef::Ics20Withdrawal(action.clone())
                }
                crate::ComplianceRecordRef::Ics20Withdrawal(action) => {
                    crate::ComplianceRecordRef::HostWithdrawal(action.clone())
                }
                _ => panic!("withdrawal fixture expected"),
            };
            let changed = ComplianceEvidenceObject::new_withdrawal(
                swapped,
                asset_id,
                flagged,
                ct.clone(),
                evidence.withdrawal.clone().unwrap(),
            )
            .unwrap();
            assert!(
                validate_and_save_evidence_object(&store, &changed).is_err(),
                "evidence must bind the withdrawal action type"
            );
        }
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
    let block = evidence.output_ref().action.tx.block.clone();
    let mut raw_bytes = evidence.ciphertext_bytes();
    if tamper_ciphertext {
        raw_bytes[0] ^= 1;
    }
    let event = DetectionEvent {
        record_ref: evidence.record_ref.clone(),
        asset_id: evidence.asset_id,
        is_flagged: evidence.is_flagged,
        salt: evidence.detection_salt,
        routing_tags: [11, 22],
        ciphertext: match &evidence.ciphertext {
            crate::ComplianceEvidenceCiphertext::Transfer(ct) => {
                crate::scanner::types::ComplianceCiphertext::Transfer(ct.clone())
            }
            _ => panic!("transfer fixture expected"),
        },
        public_withdrawal: None,
        raw_bytes: evidence.ciphertext_bytes(),
    };
    crate::scanner::ScannedBlock {
        block,
        outputs: vec![crate::scanner::ScannedOutput {
            ciphertext: ExtractedComplianceCiphertext {
                record_ref: evidence.record_ref.clone(),
                kind: crate::scanner::types::ComplianceCiphertextKind::Transfer,
                public_withdrawal: None,
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
            Some(evidence.output_ref().action.tx.block.clone()),
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
    let crate::ComplianceRecordRef::TransferOutput(output_ref) =
        &mut malformed.ciphertext.record_ref
    else {
        panic!("transfer fixture expected")
    };
    output_ref.output_index += 1;
    if let OutputOutcome::Detected { event, .. } = &mut malformed.outcome {
        event.record_ref = malformed.ciphertext.record_ref.clone();
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
    let output_ref = evidence.output_ref();
    let conn = store.lock_conn().unwrap();
    conn.query_row(
        "SELECT audit_status FROM scanner_detections
         WHERE height = ?1 AND tx_hash = ?2 AND action_index = ?3 AND output_index = ?4",
        params![
            output_ref.action.tx.block.height as i64,
            output_ref.action.tx.tx_hash.as_ref(),
            output_ref.action.action_index as i64,
            output_ref.output_index as i64,
        ],
        |row| row.get(0),
    )
    .unwrap()
}
