use shieldd_sdk_compliance::{AuditDetectedRef, AuditScanExport};

#[test]
fn audit_dtos_importable_from_crate_root() {
    fn assert_importable<T>() {}

    assert_importable::<AuditDetectedRef>();
    assert_importable::<AuditScanExport>();
}
