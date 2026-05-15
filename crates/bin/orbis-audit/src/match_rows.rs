use penumbra_sdk_compliance::{AuditDetectedRef, DecryptedVia, OrbisAuditEntry};
use penumbra_sdk_num::Amount;

#[derive(Clone, Debug)]
pub struct AddressData {
    pub transmission_key_hex: String,
}

#[derive(Clone, Debug)]
pub enum TransferMatch {
    Sender {
        amount: Amount,
        receiver: AddressData,
    },
    Receiver {
        amount: Amount,
        sender: AddressData,
    },
}

pub fn candidate_to_entry(
    tx_ref: &AuditDetectedRef,
    candidate: TransferMatch,
    tier_mode: &str,
    subject_transmission_key_hex: &str,
) -> OrbisAuditEntry {
    let (amount, counterparty) = match (tier_mode, candidate) {
        ("default", TransferMatch::Receiver { amount, .. })
        | ("default", TransferMatch::Sender { amount, .. }) => (amount, String::new()),
        ("extension", TransferMatch::Receiver { amount, sender }) => {
            (amount, sender.transmission_key_hex)
        }
        ("extension", TransferMatch::Sender { amount, receiver }) => {
            (amount, receiver.transmission_key_hex)
        }
        _ => unreachable!("tier already validated"),
    };
    OrbisAuditEntry {
        height: tx_ref.height,
        tx_hash: tx_ref.tx_hash.clone(),
        action_index: tx_ref.action_index,
        output_index: tx_ref.output_index,
        amount: amount.value().to_string(),
        self_address: subject_transmission_key_hex.to_string(),
        counterparty,
        decrypted_via: DecryptedVia::OrbisPre,
    }
}
