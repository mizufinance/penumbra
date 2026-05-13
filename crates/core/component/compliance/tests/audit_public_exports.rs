use penumbra_sdk_compliance::audit::{
    AuditDetectedRef as AuditModuleDetectedRef, AuditScanExport as AuditModuleScanExport,
    OrbisAuditEntry as AuditModuleOrbisAuditEntry,
};
use penumbra_sdk_compliance::{AuditDetectedRef, AuditScanExport, OrbisAuditEntry};

#[test]
fn moved_audit_dtos_remain_importable_from_crate_root() {
    fn assert_importable<T>() {}

    assert_importable::<AuditDetectedRef>();
    assert_importable::<AuditScanExport>();
    assert_importable::<OrbisAuditEntry>();
}

#[test]
fn moved_audit_dtos_remain_importable_from_audit_module() {
    fn assert_importable<T>() {}

    assert_importable::<AuditModuleDetectedRef>();
    assert_importable::<AuditModuleScanExport>();
    assert_importable::<AuditModuleOrbisAuditEntry>();
}
