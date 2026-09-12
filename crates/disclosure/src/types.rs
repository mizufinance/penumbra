use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;
pub const MAX_OUTPUTS: usize = 32;
pub const MAX_DOCUMENT_BYTES: usize = 64 * 1024;
pub const MAX_WITNESS_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PACKAGE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputRef {
    pub transaction_id: String,
    pub height: u64,
    pub action: ActionRef,
    pub output: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ActionRef {
    Body(u32),
    FeeFunding,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosureRequest {
    pub version: u32,
    pub chain_id: String,
    pub recipient: Option<String>,
    pub challenge: Option<String>,
    pub outputs: Vec<OutputClaim>,
    pub total: Option<TotalClaim>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputClaim {
    pub reference: OutputRef,
    pub amount: bool,
    pub asset: bool,
    pub recipient: bool,
    pub predicate: Option<AmountPredicate>,
    pub memo: bool,
    pub spending_control: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmountPredicate {
    GreaterThan(String),
    LessThan(String),
    AtLeast(String),
    AtMost(String),
    InclusiveRange { lower: String, upper: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TotalClaim {
    pub reveal: bool,
    pub predicate: Option<AmountPredicate>,
}

/// Public bytes extracted from the selected accepted output, never wallet metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicOutput {
    pub reference: OutputRef,
    pub commitment: String,
    pub ephemeral_key: Vec<u8>,
    pub encrypted_note: Vec<u8>,
    pub wrapped_memo_key: Vec<u8>,
    pub memo_ciphertext: Option<Vec<u8>>,
    pub spend_verification_key: Option<Vec<u8>>,
}

/// Private proving input. Deliberately does not implement Debug.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosureWitness {
    pub request: DisclosureRequest,
    pub outputs: Vec<OutputWitness>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputWitness {
    pub public: PublicOutput,
    pub note: Vec<u8>,
    pub control_signature: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosureStatement {
    pub request: DisclosureRequest,
    pub outputs: Vec<DisclosedOutput>,
    pub selected_output_total: Option<DisclosedTotal>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosedOutput {
    pub public: PublicOutput,
    pub amount: Option<String>,
    pub asset: Option<String>,
    pub recipient: Option<String>,
    pub memo: Option<DisclosedMemo>,
    pub predicate_result: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosedMemo {
    pub return_address: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosedTotal {
    pub predicate_result: Option<bool>,
    pub asset: String,
    pub amount: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisclosurePackage {
    pub version: u32,
    pub statement: DisclosureStatement,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Evidence {
    Groth16 {
        circuit: String,
        verification_key_sha256: String,
        #[serde(with = "receipt_encoding")]
        proof: Vec<u8>,
        control_signatures: Vec<Option<Vec<u8>>>,
    },
    Openings {
        openings: Vec<NoteOpening>,
        control_signatures: Vec<Option<Vec<u8>>>,
    },
    PayloadKeys {
        keys: Vec<Vec<u8>>,
        control_signatures: Vec<Option<Vec<u8>>>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Acceptance {
    NotChecked,
    Rejected,
    Confirmed { chain_id: String, heights: Vec<u64> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerificationResult {
    pub cryptography_verified: bool,
    pub acceptance: Acceptance,
}

impl VerificationResult {
    pub fn fully_verified(&self) -> bool {
        self.cryptography_verified && matches!(self.acceptance, Acceptance::Confirmed { .. })
    }
}

/// Committed node data fetched independently of a disclosure package.
#[derive(Clone)]
pub struct AcceptedBlock {
    pub height: u64,
    pub transactions: Vec<shieldd_sdk_transaction::Transaction>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Inspection {
    pub statement: DisclosureStatement,
    pub grants_note_decryption: bool,
    pub grants_transaction_wide_memo_decryption: bool,
}

mod receipt_encoding {
    use super::*;
    use base64::{engine::general_purpose::STANDARD, Engine};
    pub fn serialize<S: serde::Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<u8>, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        if encoded.len() > MAX_PACKAGE_BYTES {
            return Err(serde::de::Error::custom("receipt exceeds size limit"));
        }
        STANDARD.decode(encoded).map_err(serde::de::Error::custom)
    }
}

/// Full opening excludes note seed and decryption keys.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteOpening {
    pub blinding: String,
    pub amount: String,
    pub asset: String,
    pub recipient: String,
    pub recovery: String,
}
