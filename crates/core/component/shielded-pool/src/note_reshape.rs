use blake2b_simd;
use decaf377::{Fq, Fr};
#[cfg(any(unix, windows))]
use decaf377_rdsa::Signature;
use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
use penumbra_sdk_asset::asset;
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_tct as tct;

use crate::{Note, Rseed};

pub(crate) struct HiddenArityPadder {
    pub value_blinding: Fr,
    pub first_spend_randomizer: Fr,
    pub sender_address: Address,
    pub asset_id: asset::Id,
    pub nullifier_domain_sep_label: &'static [u8],
    pub nullifier_seed_label: &'static [u8],
    pub spend_auth_key_label: &'static [u8],
    pub spend_auth_randomizer_label: &'static [u8],
    pub input_note_label: &'static [u8],
    pub output_note_label: &'static [u8],
}

impl HiddenArityPadder {
    fn derive_dummy_bytes(&self, label: &[u8], slot: usize) -> [u8; 32] {
        let mut data = Vec::with_capacity(label.len() + 32 + 32 + 8);
        data.extend_from_slice(label);
        data.extend_from_slice(&self.value_blinding.to_bytes());
        data.extend_from_slice(&self.first_spend_randomizer.to_bytes());
        data.extend_from_slice(&(slot as u64).to_le_bytes());
        let digest = blake2b_simd::blake2b(&data);
        digest.as_bytes()[..32]
            .try_into()
            .expect("blake2b output is 64 bytes; taking first 32 bytes")
    }

    pub fn synthetic_dummy_nullifier_seed(&self, slot: usize) -> Fq {
        Fq::from_le_bytes_mod_order(&self.derive_dummy_bytes(self.nullifier_seed_label, slot))
    }

    pub fn synthetic_dummy_spend_auth_key(&self, slot: usize) -> Fr {
        Fr::from_le_bytes_mod_order(&self.derive_dummy_bytes(self.spend_auth_key_label, slot))
    }

    pub fn synthetic_dummy_spend_auth_randomizer(&self, slot: usize) -> Fr {
        Fr::from_le_bytes_mod_order(
            &self.derive_dummy_bytes(self.spend_auth_randomizer_label, slot),
        )
    }

    fn synthetic_dummy_rseed(&self, slot: usize, label: &[u8]) -> Rseed {
        Rseed(self.derive_dummy_bytes(label, slot))
    }

    pub fn synthetic_dummy_nullifier(&self, slot: usize) -> Nullifier {
        let seed = self.synthetic_dummy_nullifier_seed(slot);
        let randomizer = self.synthetic_dummy_spend_auth_randomizer(slot);
        Nullifier(poseidon377::hash_3(
            &self.nullifier_domain_sep(),
            (
                seed,
                Fq::from_le_bytes_mod_order(&randomizer.to_bytes()),
                Fq::from(slot as u64),
            ),
        ))
    }

    pub fn synthetic_dummy_verification_key(&self, slot: usize) -> VerificationKey<SpendAuth> {
        let dummy_sk = SigningKey::<SpendAuth>::from(self.synthetic_dummy_spend_auth_key(slot));
        let randomized = dummy_sk.randomize(&self.synthetic_dummy_spend_auth_randomizer(slot));
        VerificationKey::from(randomized)
    }

    #[cfg(any(unix, windows))]
    pub fn synthetic_dummy_auth_sig(
        &self,
        slot: usize,
        effect_hash: &[u8],
    ) -> Signature<SpendAuth> {
        let dummy_sk = SigningKey::<SpendAuth>::from(self.synthetic_dummy_spend_auth_key(slot));
        let randomized = dummy_sk.randomize(&self.synthetic_dummy_spend_auth_randomizer(slot));
        randomized.sign_deterministic(effect_hash)
    }

    pub fn synthetic_dummy_input_note(&self, slot: usize) -> Note {
        self.synthetic_dummy_note(slot, self.input_note_label)
    }

    pub fn synthetic_dummy_output_note(&self, slot: usize) -> Note {
        self.synthetic_dummy_note(slot, self.output_note_label)
    }

    fn synthetic_dummy_note(&self, slot: usize, label: &[u8]) -> Note {
        Note::from_parts(
            self.sender_address.clone(),
            penumbra_sdk_asset::Value {
                amount: Amount::zero(),
                asset_id: self.asset_id,
            },
            self.synthetic_dummy_rseed(slot, label),
        )
        .expect("synthetic hidden-arity dummy note is valid")
    }

    fn nullifier_domain_sep(&self) -> Fq {
        Fq::from_le_bytes_mod_order(
            blake2b_simd::blake2b(self.nullifier_domain_sep_label).as_bytes(),
        )
    }
}

pub(crate) fn pad_to_len<T>(
    items: &mut Vec<T>,
    len: usize,
    mut make_padding: impl FnMut(usize) -> T,
) {
    while items.len() < len {
        let slot = items.len();
        items.push(make_padding(slot));
    }
}

pub(crate) fn dummy_state_commitment_proof(commitment: tct::StateCommitment) -> tct::Proof {
    tct::Proof::new(
        commitment,
        0u64.into(),
        [[penumbra_sdk_tct::structure::Hash::new(Fq::from(0u64)); 3]; 24],
    )
}

#[cfg(any(unix, windows))]
pub(crate) fn dummy_spend_auth_sig() -> Signature<SpendAuth> {
    [0u8; 64].into()
}
