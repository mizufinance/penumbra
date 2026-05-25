use anyhow::Error;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Deserialize, serde::Serialize,
)]
#[serde(transparent)]
pub struct ConsolidateFamilyId(pub u32);

#[allow(non_upper_case_globals)]
impl ConsolidateFamilyId {
    pub const TwoByOne: Self = Self(1);
    pub const FourByOne: Self = Self(2);
    pub const EightByOne: Self = Self(3);

    pub const ALL: [Self; 3] = [Self::TwoByOne, Self::FourByOne, Self::EightByOne];

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
        self.input_count()
    }

    pub fn spec(self) -> &'static ConsolidateFamilySpec {
        CONSOLIDATE_FAMILY_SPECS
            .iter()
            .find(|spec| spec.id == self)
            .expect("unknown consolidate family id")
    }

    pub fn smallest_covering_input_count(inputs: usize) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|family| family.input_count() >= inputs)
    }
}

impl TryFrom<u32> for ConsolidateFamilyId {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let family = Self(value);
        if CONSOLIDATE_FAMILY_SPECS
            .iter()
            .any(|spec| spec.id == family)
        {
            Ok(family)
        } else {
            Err(anyhow::anyhow!("unknown consolidate family id {value}"))
        }
    }
}

impl From<ConsolidateFamilyId> for u32 {
    fn from(value: ConsolidateFamilyId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConsolidateFamilySpec {
    pub id: ConsolidateFamilyId,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub n_in: usize,
    pub n_out: usize,
}

pub const CONSOLIDATE_FAMILY_SPECS: [ConsolidateFamilySpec; 3] = [
    ConsolidateFamilySpec {
        id: ConsolidateFamilyId::TwoByOne,
        label: "consolidate2x1",
        artifact_name: "consolidate2x1",
        n_in: 2,
        n_out: 1,
    },
    ConsolidateFamilySpec {
        id: ConsolidateFamilyId::FourByOne,
        label: "consolidate4x1",
        artifact_name: "consolidate4x1",
        n_in: 4,
        n_out: 1,
    },
    ConsolidateFamilySpec {
        id: ConsolidateFamilyId::EightByOne,
        label: "consolidate8x1",
        artifact_name: "consolidate8x1",
        n_in: 8,
        n_out: 1,
    },
];
