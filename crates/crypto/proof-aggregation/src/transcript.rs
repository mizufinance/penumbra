use std::borrow::Cow;
use std::mem;

use blake2::{
    digest::{generic_array::GenericArray, FixedOutput, Reset, Update},
    Blake2b,
};
use shieldd_sdk_shielded_pool::{NoteReshapeFamilyId, ShieldedIcs20WithdrawalFamilyId};

use crate::ProofFamilyId;

pub fn transcript_family_domain(family_id: ProofFamilyId) -> Cow<'static, [u8]> {
    match family_id {
        ProofFamilyId::Transfer => Cow::Borrowed(b"shieldd.snarkpack.transfer"),
        ProofFamilyId::NoteReshape(family_id) => {
            Cow::Owned(format!("shieldd.snarkpack.{}", family_id.label()).into_bytes())
        }
        ProofFamilyId::ShieldedIcs20Withdrawal(family_id) => {
            Cow::Owned(format!("shieldd.snarkpack.{}", family_id.label()).into_bytes())
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

define_family_digest!(
    ShieldedIcs20WithdrawalTranscriptDigest,
    ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical)
);

#[derive(Clone)]
pub(crate) struct TransferTranscriptDigest(Blake2b);

impl Default for TransferTranscriptDigest {
    fn default() -> Self {
        let mut inner = Blake2b::default();
        inner.update(transcript_family_domain(ProofFamilyId::Transfer).as_ref());
        Self(inner)
    }
}

impl Update for TransferTranscriptDigest {
    fn update(&mut self, data: impl AsRef<[u8]>) {
        self.0.update(data);
    }
}

impl Reset for TransferTranscriptDigest {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl FixedOutput for TransferTranscriptDigest {
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

#[derive(Clone)]
pub(crate) struct NoteReshapeTranscriptDigest<const FAMILY_ID: u32>(Blake2b);

impl<const FAMILY_ID: u32> Default for NoteReshapeTranscriptDigest<FAMILY_ID> {
    fn default() -> Self {
        let mut inner = Blake2b::default();
        let family_id = NoteReshapeFamilyId::try_from(FAMILY_ID)
            .expect("NoteReshapeTranscriptDigest requires a registered family id");
        inner.update(transcript_family_domain(ProofFamilyId::NoteReshape(family_id)).as_ref());
        Self(inner)
    }
}

impl<const FAMILY_ID: u32> Update for NoteReshapeTranscriptDigest<FAMILY_ID> {
    fn update(&mut self, data: impl AsRef<[u8]>) {
        self.0.update(data);
    }
}

impl<const FAMILY_ID: u32> Reset for NoteReshapeTranscriptDigest<FAMILY_ID> {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl<const FAMILY_ID: u32> FixedOutput for NoteReshapeTranscriptDigest<FAMILY_ID> {
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
    use blake2::{
        digest::{FixedOutput, Update},
        Blake2b,
    };
    use shieldd_sdk_shielded_pool::{NoteReshapeFamilyId, ShieldedIcs20WithdrawalFamilyId};

    use ark_ip_proofs::challenge::{challenge_preimage, ChallengeContext};

    use super::{
        transcript_family_domain, NoteReshapeTranscriptDigest,
        ShieldedIcs20WithdrawalTranscriptDigest, TransferTranscriptDigest,
    };

    fn assert_digest_prefix<D>(family: ProofFamilyId, challenge_frame: &[u8])
    where
        D: Default + FixedOutput + Update,
    {
        let mut actual = D::default();
        actual.update(challenge_frame);
        let actual = actual.finalize_fixed();

        let mut expected = Blake2b::default();
        expected.update(transcript_family_domain(family).as_ref());
        expected.update(challenge_frame);
        let expected = expected.finalize_fixed();

        assert_eq!(actual.as_slice(), expected.as_slice());
    }

    #[test]
    fn transcript_family_domains_are_exact_and_unique() {
        let expected = [
            (
                ProofFamilyId::Transfer,
                "shieldd.snarkpack.transfer".as_bytes(),
            ),
            (
                ProofFamilyId::NoteReshape(NoteReshapeFamilyId::OneByEight),
                "shieldd.snarkpack.note_reshape1x8".as_bytes(),
            ),
            (
                ProofFamilyId::NoteReshape(NoteReshapeFamilyId::EightByOne),
                "shieldd.snarkpack.note_reshape8x1".as_bytes(),
            ),
            (
                ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
                "shieldd.snarkpack.shielded_ics20_withdrawal".as_bytes(),
            ),
        ];
        let mut domains = BTreeSet::new();
        for (family, expected_domain) in expected {
            let domain = transcript_family_domain(family);
            assert_eq!(domain.as_ref(), expected_domain);
            assert!(domains.insert(domain));
        }
    }

    #[test]
    fn every_registered_digest_hashes_family_domain_then_challenge_frame() {
        let context = ChallengeContext::from_statement_digest([0x53; 32]);
        let frame = challenge_preimage(
            &context,
            b"aggregate.randomizer",
            0x0807_0605_0403_0201,
            b"commitment bytes",
        );

        assert_digest_prefix::<TransferTranscriptDigest>(ProofFamilyId::Transfer, &frame);
        assert_digest_prefix::<
            NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::OneByEight.get() }>,
        >(
            ProofFamilyId::NoteReshape(NoteReshapeFamilyId::OneByEight),
            &frame,
        );
        assert_digest_prefix::<
            NoteReshapeTranscriptDigest<{ NoteReshapeFamilyId::EightByOne.get() }>,
        >(
            ProofFamilyId::NoteReshape(NoteReshapeFamilyId::EightByOne),
            &frame,
        );
        assert_digest_prefix::<ShieldedIcs20WithdrawalTranscriptDigest>(
            ProofFamilyId::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalFamilyId::Canonical),
            &frame,
        );
    }
}
