# Compliance Constraint Checklist

This checklist covers compliance-owned constraints only. Transfer-circuit
soundness constraints such as nullifiers, randomized spend authorization keys,
note commitments, anchors, value conservation, and balance commitments are
external invariants tracked in `docs/transfer-circuit/constraint-checklist.md`.

Each entry lists where the rule is enforced, where runtime validation observes
or re-checks it, and where regressions should be caught.

## Circuit Constraints

### Asset Policy Binding

- Enforced in: `crates/core/component/compliance/src/r1cs.rs`
  (`verify_asset_registry_imt`, `verify_compliance_integrity`).
- Verified at: transaction proof verification and registry anchor checks in
  `crates/core/component/compliance/src/registry.rs`.
- Tested by: `r1cs::tests::test_verify_asset_registry_imt_membership`,
  `r1cs::tests::test_verify_asset_registry_imt_non_membership`, and
  `tools/gnark/internal/circuits/transfer_metamorphic_test.go`.
- Violation: transfer proof rejects or registry/domain check returns an error.

### Regulated Policy Usage

- Enforced in: transfer planning/proof wiring under
  `crates/core/component/shielded-pool/src/transfer/` and compliance checks in
  `crates/core/component/compliance/src/r1cs.rs`.
- Verified at: planner asset-policy lookup and validator proof verification.
- Tested by: compliance registry tests and transfer gnark metamorphic tests.
- Violation: transaction planning fails or the transfer proof rejects.

### Threshold Flag

- Enforced in: `verify_threshold_flag_simple` and transfer compliance public
  inputs.
- Verified at: detection-tier DK screening and audit evidence matching.
- Tested by: `tools/gnark/internal/compliance/threshold_test.go` and
  `scanner::screener::tests::screener_detects_matching_asset`.
- Violation: proof rejects, or scanner/evidence validation records a failure.

### Detection Tier

- Enforced in: transfer compliance ciphertext construction and circuit public
  inputs.
- Verified at: `ComplianceScreener::screen` and
  `DetectionKey::try_decrypt_detection`.
- Tested by: `scanner::screener::*` tests and
  `issuer_keys::tests::test_detection_tier_roundtrip`.
- Violation: ciphertext is marked irrelevant or invalid; malformed rows are
  capped and persisted.

### Tier Encryption And DLEQ Binding

- Enforced in: transfer compliance circuit fields and DLEQ checks in
  `crates/core/component/compliance/src/r1cs.rs`.
- Verified at: `validate_audit_evidence`,
  `PublicTransferTierDecodeObject::validate`, and
  `TransferOrbisUploadBundle::validate`.
- Tested by: `audit_validation::*`, `decode_object::*`,
  `upload_package::*`, and
  `tools/gnark/internal/circuits/transfer_metamorphic_test.go`.
- Violation: proof rejects, evidence is marked `evidence_invalid`, or Orbis
  import/decryption is refused.

### Tier Metadata

- Enforced in: transfer DLEQ metadata hash construction for
  policy/resource/permission hash, tier label, target timestamp, and salt.
- Verified at: `TransferTierMetadataStatement::validate_shape`,
  `validate_audit_evidence`, and upload bundle validation.
- Tested by: `audit_validation::tests::tampered_tier_label_is_invalid_evidence`,
  `audit_validation::tests::tampered_orbis_package_metadata_is_invalid_package`,
  and gnark tier mutation tests.
- Violation: proof rejects or evidence/import validation records a failure.

## Scanner And Evidence Constraints

### Output Identity

- Enforced in: typed refs `BlockRef`, `TxRef`, `ActionRef`, `OutputRef` and
  scanner primary keys.
- Verified at: `SqliteScannerStore::commit_block` and
  `validate_and_save_evidence_object`.
- Tested by: scanner storage tests and transaction-id parity test.
- Violation: duplicate rows are idempotent; mismatched evidence cannot validate.

### Persisted Ciphertext Match

- Enforced in: `validate_and_save_evidence_object`.
- Verified at: scanner DB lookup of `scanner_ciphertexts.raw_bytes`.
- Tested by: `audit::tests::evidence_object_rejects_mismatched_persisted_ciphertext`.
- Violation: evidence failure is recorded with stage `validate_evidence`.

### DK Plaintext Match

- Enforced in: `validate_and_save_evidence_object`.
- Verified at: comparison against `scanner_detections` asset, flag, and salt.
- Tested by: audit validation tamper tests and screener tests.
- Violation: evidence failure is recorded and the row cannot complete audit.

### Upload Bundle Match

- Enforced in: `validate_and_save_evidence_object` and
  `TransferOrbisUploadBundle::validate`.
- Verified at: persisted upload-bundle bytes and tier statement comparison.
- Tested by: `audit_validation::*`, `evidence::*`, and `upload_package::*`.
- Violation: evidence failure is recorded with stage `validate_upload_bundle`.

### Audit Completion Gate

- Enforced in: `decrypt_flagged_rows`, `export_orbis_pending_scan`, and
  `import_orbis_audit_entries`.
- Verified at: `scanner_detections.audit_status`; only `evidence_valid` rows
  can complete via issuer DK or Orbis PRE.
- Tested by: `audit::tests::flagged_decrypt_requires_valid_evidence`,
  `audit::tests::orbis_export_requires_valid_evidence`, and
  `audit::tests::orbis_import_requires_valid_evidence`.
- Violation: completion is refused and a failure row is persisted where useful.
