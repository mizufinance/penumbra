use std::borrow::Cow;
use std::mem;

use blake2::{
    digest::{generic_array::GenericArray, FixedOutput, Reset, Update},
    Blake2b,
};
use penumbra_sdk_shielded_pool::TransferFamilyId;

use crate::ProofFamilyId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TranscriptPhase {
    Prover,
    Verifier,
}

pub fn transcript_family_domain(family_id: ProofFamilyId) -> Cow<'static, [u8]> {
    match family_id {
        ProofFamilyId::Spend => Cow::Borrowed(b"penumbra.snarkpack.spend.v1"),
        ProofFamilyId::Output => Cow::Borrowed(b"penumbra.snarkpack.output.v1"),
        ProofFamilyId::Transfer(family_id) => {
            Cow::Owned(format!("penumbra.snarkpack.{}.v1", family_id.label()).into_bytes())
        }
        ProofFamilyId::Swap => Cow::Borrowed(b"penumbra.snarkpack.swap.v1"),
        ProofFamilyId::SwapClaim => Cow::Borrowed(b"penumbra.snarkpack.swap_claim.v1"),
        ProofFamilyId::Convert => Cow::Borrowed(b"penumbra.snarkpack.convert.v1"),
        ProofFamilyId::DelegatorVote => Cow::Borrowed(b"penumbra.snarkpack.delegator_vote.v1"),
    }
}

pub fn transcript_domain(family_id: ProofFamilyId, phase: TranscriptPhase) -> Cow<'static, [u8]> {
    match (family_id, phase) {
        (ProofFamilyId::Spend, TranscriptPhase::Prover) => {
            Cow::Borrowed(b"penumbra.snarkpack.spend.prover.v1")
        }
        (ProofFamilyId::Spend, TranscriptPhase::Verifier) => {
            Cow::Borrowed(b"penumbra.snarkpack.spend.verifier.v1")
        }
        (ProofFamilyId::Output, TranscriptPhase::Prover) => {
            Cow::Borrowed(b"penumbra.snarkpack.output.prover.v1")
        }
        (ProofFamilyId::Output, TranscriptPhase::Verifier) => {
            Cow::Borrowed(b"penumbra.snarkpack.output.verifier.v1")
        }
        (ProofFamilyId::Transfer(family_id), TranscriptPhase::Prover) => {
            Cow::Owned(format!("penumbra.snarkpack.{}.prover.v1", family_id.label()).into_bytes())
        }
        (ProofFamilyId::Transfer(family_id), TranscriptPhase::Verifier) => {
            Cow::Owned(format!("penumbra.snarkpack.{}.verifier.v1", family_id.label()).into_bytes())
        }
        (ProofFamilyId::Swap, TranscriptPhase::Prover) => {
            Cow::Borrowed(b"penumbra.snarkpack.swap.prover.v1")
        }
        (ProofFamilyId::Swap, TranscriptPhase::Verifier) => {
            Cow::Borrowed(b"penumbra.snarkpack.swap.verifier.v1")
        }
        (ProofFamilyId::SwapClaim, TranscriptPhase::Prover) => {
            Cow::Borrowed(b"penumbra.snarkpack.swap_claim.prover.v1")
        }
        (ProofFamilyId::SwapClaim, TranscriptPhase::Verifier) => {
            Cow::Borrowed(b"penumbra.snarkpack.swap_claim.verifier.v1")
        }
        (ProofFamilyId::Convert, TranscriptPhase::Prover) => {
            Cow::Borrowed(b"penumbra.snarkpack.convert.prover.v1")
        }
        (ProofFamilyId::Convert, TranscriptPhase::Verifier) => {
            Cow::Borrowed(b"penumbra.snarkpack.convert.verifier.v1")
        }
        (ProofFamilyId::DelegatorVote, TranscriptPhase::Prover) => {
            Cow::Borrowed(b"penumbra.snarkpack.delegator_vote.prover.v1")
        }
        (ProofFamilyId::DelegatorVote, TranscriptPhase::Verifier) => {
            Cow::Borrowed(b"penumbra.snarkpack.delegator_vote.verifier.v1")
        }
    }
}

macro_rules! define_family_digest {
    ($name:ident, $family:expr) => {
        #[derive(Clone)]
        pub(crate) struct $name(Blake2b);

        impl Default for $name {
            fn default() -> Self {
                let mut inner = Blake2b::default();
                inner.update(transcript_family_domain($family).as_ref());
                Self(inner)
            }
        }

        impl Update for $name {
            fn update(&mut self, data: impl AsRef<[u8]>) {
                self.0.update(data);
            }
        }

        impl Reset for $name {
            fn reset(&mut self) {
                *self = Self::default();
            }
        }

        impl FixedOutput for $name {
            type OutputSize = <Blake2b as FixedOutput>::OutputSize;

            fn finalize_into(self, out: &mut GenericArray<u8, Self::OutputSize>) {
                self.0.finalize_into(out);
            }

            fn finalize_into_reset(&mut self, out: &mut GenericArray<u8, Self::OutputSize>) {
                let inner = mem::take(&mut self.0);
                inner.finalize_into(out);
                self.reset();
            }
        }
    };
}

define_family_digest!(SpendTranscriptDigest, ProofFamilyId::Spend);
define_family_digest!(OutputTranscriptDigest, ProofFamilyId::Output);
define_family_digest!(SwapTranscriptDigest, ProofFamilyId::Swap);
define_family_digest!(SwapClaimTranscriptDigest, ProofFamilyId::SwapClaim);
define_family_digest!(ConvertTranscriptDigest, ProofFamilyId::Convert);
define_family_digest!(DelegatorVoteTranscriptDigest, ProofFamilyId::DelegatorVote);

#[derive(Clone)]
pub(crate) struct TransferTranscriptDigest<const FAMILY_ID: u32>(Blake2b);

impl<const FAMILY_ID: u32> Default for TransferTranscriptDigest<FAMILY_ID> {
    fn default() -> Self {
        let mut inner = Blake2b::default();
        inner.update(
            transcript_family_domain(ProofFamilyId::Transfer(TransferFamilyId(FAMILY_ID))).as_ref(),
        );
        Self(inner)
    }
}

impl<const FAMILY_ID: u32> Update for TransferTranscriptDigest<FAMILY_ID> {
    fn update(&mut self, data: impl AsRef<[u8]>) {
        self.0.update(data);
    }
}

impl<const FAMILY_ID: u32> Reset for TransferTranscriptDigest<FAMILY_ID> {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl<const FAMILY_ID: u32> FixedOutput for TransferTranscriptDigest<FAMILY_ID> {
    type OutputSize = <Blake2b as FixedOutput>::OutputSize;

    fn finalize_into(self, out: &mut GenericArray<u8, Self::OutputSize>) {
        self.0.finalize_into(out);
    }

    fn finalize_into_reset(&mut self, out: &mut GenericArray<u8, Self::OutputSize>) {
        let inner = mem::take(&mut self.0);
        inner.finalize_into(out);
        self.reset();
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::ProofFamilyId;
    use penumbra_sdk_shielded_pool::TransferFamilyId;

    use super::{transcript_domain, transcript_family_domain, TranscriptPhase};

    #[test]
    fn transcript_domains_are_unique() {
        let mut domains = BTreeSet::new();
        let mut families = vec![
            ProofFamilyId::Spend,
            ProofFamilyId::Output,
            ProofFamilyId::Swap,
            ProofFamilyId::SwapClaim,
            ProofFamilyId::Convert,
            ProofFamilyId::DelegatorVote,
        ];
        families.extend(
            TransferFamilyId::ALL
                .into_iter()
                .map(ProofFamilyId::Transfer),
        );
        for family in families {
            for phase in [TranscriptPhase::Prover, TranscriptPhase::Verifier] {
                assert!(domains.insert(transcript_domain(family, phase)));
            }
        }
    }

    #[test]
    fn transcript_family_domains_are_unique() {
        let mut domains = BTreeSet::new();
        let mut families = vec![
            ProofFamilyId::Spend,
            ProofFamilyId::Output,
            ProofFamilyId::Swap,
            ProofFamilyId::SwapClaim,
            ProofFamilyId::Convert,
            ProofFamilyId::DelegatorVote,
        ];
        families.extend(
            TransferFamilyId::ALL
                .into_iter()
                .map(ProofFamilyId::Transfer),
        );
        for family in families {
            assert!(domains.insert(transcript_family_domain(family)));
        }
    }
}
