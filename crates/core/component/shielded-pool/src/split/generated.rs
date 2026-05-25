use anyhow::Error;

#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, serde::Deserialize, serde::Serialize,
)]
#[serde(transparent)]
pub struct SplitFamilyId(pub u32);

#[allow(non_upper_case_globals)]
impl SplitFamilyId {
    pub const OneByFour: Self = Self(1);
    pub const OneByEight: Self = Self(2);

    pub const ALL: [Self; 2] = [Self::OneByFour, Self::OneByEight];

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

    pub fn spec(self) -> &'static SplitFamilySpec {
        SPLIT_FAMILY_SPECS
            .iter()
            .find(|spec| spec.id == self)
            .expect("unknown split family id")
    }

    pub fn smallest_covering_output_count(outputs: usize) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|family| family.output_count() >= outputs)
    }
}

impl TryFrom<u32> for SplitFamilyId {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let family = Self(value);
        if SPLIT_FAMILY_SPECS.iter().any(|spec| spec.id == family) {
            Ok(family)
        } else {
            Err(anyhow::anyhow!("unknown split family id {value}"))
        }
    }
}

impl From<SplitFamilyId> for u32 {
    fn from(value: SplitFamilyId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SplitFamilySpec {
    pub id: SplitFamilyId,
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub n_in: usize,
    pub n_out: usize,
}

pub const SPLIT_FAMILY_SPECS: [SplitFamilySpec; 2] = [
    SplitFamilySpec {
        id: SplitFamilyId::OneByFour,
        label: "split1x4",
        artifact_name: "split1x4",
        n_in: 1,
        n_out: 4,
    },
    SplitFamilySpec {
        id: SplitFamilyId::OneByEight,
        label: "split1x8",
        artifact_name: "split1x8",
        n_in: 1,
        n_out: 8,
    },
];
