//! Additional audit access to existing Transfer payload keys.
use decaf377::{Element, Fq};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

static DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.transfer.master_wrapping.v1").as_bytes(),
    )
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum MasterSelection {
    Amount = 0,
    Sender = 1,
    Receiver = 2,
}

impl MasterSelection {
    pub const ALL: [Self; 3] = [Self::Amount, Self::Sender, Self::Receiver];

    pub fn tier(self) -> crate::transfer_audit::TransferTier {
        use crate::transfer_audit::TransferTier;
        match self {
            Self::Amount => TransferTier::OutputCore,
            Self::Sender => TransferTier::OutputExt,
            Self::Receiver => TransferTier::SenderExt,
        }
    }

    /// Caller must verify PRE or issuer evidence against this selection's accepted EPK and key.
    pub fn decrypt(
        self,
        ct: &crate::TransferComplianceCiphertext,
        metadata: &crate::TransferComplianceMetadata,
        shared: &Element,
    ) -> anyhow::Result<crate::transfer_audit::TransferAuditData> {
        anyhow::ensure!(!shared.is_identity(), "identity master shared point");
        let tier = self.tier().select(ct, metadata)?;
        let seed = ct.master_wrappings[self as usize] - self.mask(shared, &tier.epk);
        tier.decrypt_seed(seed)
    }

    pub fn mask(self, shared: &Element, epk: &Element) -> Fq {
        poseidon377::hash_3(
            &DOMAIN,
            (
                Fq::from(self as u64),
                shared.vartime_compress_to_field(),
                epk.vartime_compress_to_field(),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::Fr;

    #[test]
    fn masks_bind_selection_and_ephemeral_key_without_flagged_equality() {
        let epk = Element::GENERATOR * Fr::from(17u64);
        let shared = epk * Fr::from(23u64);
        let seed = Fq::from(29u64);
        let original = seed + shared.vartime_compress_to_field();
        let masks = MasterSelection::ALL.map(|s| s.mask(&shared, &epk));
        for (i, selection) in MasterSelection::ALL.into_iter().enumerate() {
            assert_ne!(seed + masks[i], original);
            assert_ne!(
                masks[i],
                selection.mask(&shared, &(epk + Element::GENERATOR))
            );
            assert_ne!(
                masks[i],
                selection.mask(&(shared + Element::GENERATOR), &epk)
            );
            for other in &masks[i + 1..] {
                assert_ne!(masks[i], *other);
            }
        }
    }
}
