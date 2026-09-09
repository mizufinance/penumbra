// Code generated from shielded_withdrawal_families.json. DO NOT EDIT.
use anyhow::Error;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Deserialize, serde::Serialize,
)]
#[serde(try_from = "u32", into = "u32")]
pub struct ShieldedWithdrawalFamilyId(u32);

#[allow(non_upper_case_globals)]
impl ShieldedWithdrawalFamilyId {
    pub const Canonical: Self = Self(1);

    pub const ALL: [Self; 1] = [Self::Canonical];

    pub const fn get(self) -> u32 {
        self.0
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub fn input_count(self) -> usize {
        self.spec().n_in
    }

    pub fn output_count(self) -> usize {
        self.spec().n_out
    }

    pub fn auth_sig_count(self) -> usize {
        self.spec().n_in
    }

    pub fn spec(self) -> &'static ShieldedWithdrawalFamilySpec {
        SHIELDED_WITHDRAWAL_FAMILY_SPECS
            .iter()
            .find(|spec| spec.id == self)
            .expect("unknown shielded withdrawal family id")
    }
}

impl TryFrom<u32> for ShieldedWithdrawalFamilyId {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let family = Self(value);
        if SHIELDED_WITHDRAWAL_FAMILY_SPECS
            .iter()
            .any(|spec| spec.id == family)
        {
            Ok(family)
        } else {
            Err(anyhow::anyhow!(
                "unknown shielded withdrawal family id {value}"
            ))
        }
    }
}

impl From<ShieldedWithdrawalFamilyId> for u32 {
    fn from(value: ShieldedWithdrawalFamilyId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShieldedWithdrawalFamilySpec {
    pub id: ShieldedWithdrawalFamilyId,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub bundled_lib_basename: &'static str,
    pub n_in: usize,
    pub n_out: usize,
}

pub const SHIELDED_WITHDRAWAL_FAMILY_SPECS: [ShieldedWithdrawalFamilySpec; 1] =
    [ShieldedWithdrawalFamilySpec {
        id: ShieldedWithdrawalFamilyId::Canonical,
        label: "shielded_withdrawal",
        artifact_name: "shielded_withdrawal",
        bundled_lib_basename: "libshieldd_gnark_shielded_withdrawal",
        n_in: 2,
        n_out: 1,
    }];
