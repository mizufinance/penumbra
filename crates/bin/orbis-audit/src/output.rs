use penumbra_sdk_num::Amount;
use serde::{Deserialize, Serialize};

use crate::scan::DetectedTxRef;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditEntry {
    pub height: u64,
    pub tx_hash: String,
    pub action_index: usize,
    #[serde(default)]
    pub output_index: usize,
    pub amount: AuditAmount,
    pub self_address: TransmissionKeyHex,
    pub counterparty: TransmissionKeyHex,
    pub decrypted_via: DecryptedVia,
}

impl AuditEntry {
    pub fn new(
        tx_ref: &DetectedTxRef,
        amount: Amount,
        self_address: impl Into<String>,
        counterparty: impl Into<String>,
        decrypted_via: DecryptedVia,
    ) -> Self {
        Self {
            height: tx_ref.height,
            tx_hash: tx_ref.tx_hash.clone(),
            action_index: tx_ref.action_index,
            output_index: tx_ref.output_index,
            amount: AuditAmount::from(amount),
            self_address: TransmissionKeyHex(self_address.into()),
            counterparty: TransmissionKeyHex(counterparty.into()),
            decrypted_via,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuditAmount(pub String);

impl From<Amount> for AuditAmount {
    fn from(amount: Amount) -> Self {
        Self(amount.value().to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransmissionKeyHex(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecryptedVia {
    #[serde(rename = "orbis_pre")]
    OrbisPre,
}

#[cfg(test)]
mod tests {
    use super::DecryptedVia;

    #[test]
    fn decrypted_via_rejects_unknown_strings() {
        let error = serde_json::from_str::<DecryptedVia>(r#""unknown""#)
            .expect_err("unknown decrypted_via should fail");
        assert!(error.to_string().contains("unknown variant"));
    }
}
