use penumbra_sdk_compliance::{AuditDetectedRef, AuditScanExport, OrbisAuditEntry};

#[test]
fn audit_dtos_importable_from_crate_root() {
    fn assert_importable<T>() {}

    assert_importable::<AuditDetectedRef>();
    assert_importable::<AuditScanExport>();
    assert_importable::<OrbisAuditEntry>();
}
