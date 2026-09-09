// Code generated from shielded_withdrawal_families.json. DO NOT EDIT.
pub struct GeneratedShieldedWithdrawalFamily {
    pub id: u32,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub bundled_lib_basename: &'static str,
    pub n_in: usize,
    pub n_out: usize,
}

pub const GENERATED_SHIELDED_WITHDRAWAL_FAMILIES:
    &[GeneratedShieldedWithdrawalFamily] = &[
    GeneratedShieldedWithdrawalFamily {
        id: 1,
        label: "shielded_withdrawal",
        artifact_name: "shielded_withdrawal",
        bundled_lib_basename: "libshieldd_gnark_shielded_withdrawal",
        n_in: 2,
        n_out: 1,
    },
];
