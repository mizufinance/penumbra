use penumbra_sdk_num::Amount;

use crate::{
    output::{AuditEntry, DecryptedVia},
    scan::DetectedTxRef,
};

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
    tx_ref: &DetectedTxRef,
    candidate: TransferMatch,
    tier_mode: &str,
    subject_transmission_key_hex: &str,
) -> AuditEntry {
    match (tier_mode, candidate) {
        ("default", TransferMatch::Receiver { amount, .. })
        | ("default", TransferMatch::Sender { amount, .. }) => AuditEntry::new(
            tx_ref,
            amount,
            subject_transmission_key_hex,
            "",
            DecryptedVia::OrbisPre,
        ),
        ("extension", TransferMatch::Receiver { amount, sender }) => AuditEntry::new(
            tx_ref,
            amount,
            subject_transmission_key_hex,
            sender.transmission_key_hex,
            DecryptedVia::OrbisPre,
        ),
        ("extension", TransferMatch::Sender { amount, receiver }) => AuditEntry::new(
            tx_ref,
            amount,
            subject_transmission_key_hex,
            receiver.transmission_key_hex,
            DecryptedVia::OrbisPre,
        ),
        _ => unreachable!("tier already validated"),
    }
}
