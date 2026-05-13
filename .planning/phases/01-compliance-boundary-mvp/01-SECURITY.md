---
phase: 01
slug: compliance-boundary-mvp
status: verified
threats_open: 0
asvs_level: 1
created: 2026-05-13T13:27:46Z
updated: 2026-05-13T13:27:46Z
---

# Phase 01 - Security

Per-phase security contract: threat register, accepted risks, and audit trail.

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| audit import JSON -> audit DTOs | Imported Orbis audit entries are untrusted before row eligibility checks. | Orbis audit entries, row identifiers, audit amounts, addresses |
| scanner SQLite rows -> export DTOs | Durable scanner rows are projected into audit/export JSON consumed by tooling. | Scanner detections, clear flows, audit rows |
| pure helpers -> durable audit facade | Pure classification/projection helpers must not bypass persistence checks. | Already-loaded row facts and DTO projections |
| refactor scope -> deferred security TODOs | Registration authorization and regulated IBC policy are known security items outside this phase. | Deferred requirements, no implementation changes |

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-01-01 | Tampering | `audit_records::classify_orbis_import_row` | mitigate | `audit_records.rs` preserves eligible statuses and the unflagged requirement; tests cover eligible, flagged, invalid-status, and missing rows. | closed |
| T-01-02 | Information Disclosure | audit export DTO projection | accept | Existing export fields were preserved; no new data sources, secrets, private keys, upload bundles, or plaintext fields were added by this refactor. | closed |
| T-01-03 | Elevation of Privilege | registration authorization TODOs | transfer | Registration authorization remained out of scope per phase decisions; no related implementation path was added or modified. | closed |
| T-01-04 | Denial of Service | failure reason strings | accept | `audit_records.rs` has no persistence path; failure persistence remains in `audit.rs` with the existing stored reason shape. | closed |
| T-01-05 | Tampering | `import_orbis_audit_entries` | mitigate | The facade still performs row lookup, eligible-status gate, unflagged gate, failure recording, then audit-complete update. | closed |
| T-01-06 | Repudiation | audit failure recording | mitigate | Rejected Orbis import rows still call `record_evidence_failure_tx` with the Orbis import stage before continuing. | closed |
| T-01-07 | Information Disclosure | `export_ledger_rows_json` and `export_scan_json` | accept | Export functions still serialize the existing DTOs/ledger rows; grep and review found no added private key, upload bundle, or decrypted plaintext export field. | closed |
| T-01-08 | Elevation of Privilege | deferred registration and IBC policy enforcement | transfer | Deferred security TODOs remained untouched; grep gates found no new provider/store/compatibility surface for those areas. | closed |
| T-01-09 | Denial of Service | audit evidence failures | accept | The phase did not add new failure persistence or unbounded storage; durable failure recording remains in the existing facade path. | closed |

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-01-02 | The refactor intentionally preserves existing audit export fields and does not broaden disclosure surface. | GSD security audit | 2026-05-13 |
| AR-02 | T-01-04 | Failure reason persistence is pre-existing and unchanged; this phase adds no new persistence path. | GSD security audit | 2026-05-13 |
| AR-03 | T-01-07 | Existing audit JSON exports remain unchanged in shape; any broader export policy change belongs to a future security/storage phase. | GSD security audit | 2026-05-13 |
| AR-04 | T-01-09 | Existing evidence failure persistence shape is unchanged and bounded by the same row identity keys. | GSD security audit | 2026-05-13 |

## Transferred Risks

| Risk ID | Threat Ref | Destination | Rationale | Date |
|---------|------------|-------------|-----------|------|
| TR-01 | T-01-03 | Future registration authorization phase | Registration authorization is a known deferred security requirement and was explicitly excluded from this refactor. | 2026-05-13 |
| TR-02 | T-01-08 | Future regulated IBC policy enforcement phase | Regulated asset IBC enforcement is a known deferred security requirement and was explicitly excluded from this refactor. | 2026-05-13 |

## Evidence

| Check | Evidence | Result |
|-------|----------|--------|
| Pure helper boundary | `audit_records.rs` owns DTOs and pure helpers; grep found no `lock_conn`, transactions, SQL, `rusqlite`, `StateRead`, or `StateWrite` in that file. | pass |
| Orbis import gate | `audit.rs` still reads row status, calls `classify_orbis_import_row`, records failure for ineligible rows, then updates rows only after the gate. | pass |
| Failure recording | `record_evidence_failure_tx` remains in `audit.rs` and preserves the existing evidence failure persistence path. | pass |
| Export disclosure surface | `export_scan_json` and `export_ledger_rows_json` serialize existing DTOs/ledger rows; no new sensitive export source was added. | pass |
| Deferred security scope | Grep found no new registration authorization, regulated IBC policy, provider trait, compatibility alias, or speculative store wrapper in the changed audit boundary. | pass |
| Test evidence | Phase verification passed focused audit record tests, audit facade tests, public export tests, full compliance crate tests, formatting, compile checks, and `just check`. | pass |

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-05-13 | 9 | 9 | 0 | GSD security audit |

## Sign-Off

- [x] All threats have a disposition: mitigate, accept, or transfer.
- [x] Accepted risks documented in Accepted Risks Log.
- [x] Transferred risks documented with destination.
- [x] `threats_open: 0` confirmed.
- [x] `status: verified` set in frontmatter.

**Approval:** verified 2026-05-13
