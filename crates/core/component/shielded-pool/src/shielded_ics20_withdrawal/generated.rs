use anyhow::Error;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Deserialize, serde::Serialize,
)]
#[serde(transparent)]
pub struct ShieldedIcs20WithdrawalFamilyId(pub u32);

#[allow(non_upper_case_globals)]
impl ShieldedIcs20WithdrawalFamilyId {
    pub const Canonical: Self = Self(1);

    pub const ALL: [Self; 1] = [Self::Canonical];

    pub const fn get(self) -> u32 {
        self.0
    }

    pub fn label(self) -> &'static str {
        self.spec().label
    }

    pub const fn input_count(self) -> usize {
        2
    }

    pub const fn auth_sig_count(self) -> usize {
        2
    }

    pub fn spec(self) -> &'static ShieldedIcs20WithdrawalFamilySpec {
        SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS
            .iter()
            .find(|spec| spec.id == self)
            .expect("unknown shielded ICS-20 withdrawal family id")
    }
}

impl TryFrom<u32> for ShieldedIcs20WithdrawalFamilyId {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let family = Self(value);
        if SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS
            .iter()
            .any(|spec| spec.id == family)
        {
            Ok(family)
        } else {
            Err(anyhow::anyhow!(
                "unknown shielded ICS-20 withdrawal family id {value}"
            ))
        }
    }
}

impl From<ShieldedIcs20WithdrawalFamilyId> for u32 {
    fn from(value: ShieldedIcs20WithdrawalFamilyId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShieldedIcs20WithdrawalFamilySpec {
    pub id: ShieldedIcs20WithdrawalFamilyId,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub n_in: usize,
}

pub const SHIELDED_ICS20_WITHDRAWAL_FAMILY_SPECS: [ShieldedIcs20WithdrawalFamilySpec; 1] =
    [ShieldedIcs20WithdrawalFamilySpec {
        id: ShieldedIcs20WithdrawalFamilyId::Canonical,
        label: "shielded_ics20_withdrawal",
        artifact_name: "shielded_ics20_withdrawal",
        n_in: 2,
    }];
