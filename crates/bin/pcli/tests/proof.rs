//! Tests guard against drift in the current constraints versus the provided
//! proving/verification key.

use penumbra_sdk_asset::{asset, Value};
use penumbra_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
use penumbra_sdk_proof_params::{
    NULLIFIER_DERIVATION_PROOF_PROVING_KEY, NULLIFIER_DERIVATION_PROOF_VERIFICATION_KEY,
};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_shielded_pool::Note;
use penumbra_sdk_shielded_pool::{
    NullifierDerivationProof, NullifierDerivationProofPrivate, NullifierDerivationProofPublic,
};
use penumbra_sdk_tct as tct;
use rand_core::OsRng;

#[test]
fn nullifier_derivation_parameters_vs_current_nullifier_derivation_circuit() {
    let pk = &*NULLIFIER_DERIVATION_PROOF_PROVING_KEY;
    let vk = &*NULLIFIER_DERIVATION_PROOF_VERIFICATION_KEY;

    let mut rng = OsRng;

    let seed_phrase = SeedPhrase::generate(OsRng);
    let sk_sender = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
    let fvk_sender = sk_sender.full_viewing_key();
    let ivk_sender = fvk_sender.incoming();
    let (sender, _dtk_d) = ivk_sender.payment_address(0u32.into());

    let value_to_send = Value {
        amount: 1u128.into(),
        asset_id: asset::Cache::with_known_assets()
            .get_unit("upenumbra")
            .unwrap()
            .id(),
    };

    let note = Note::generate(&mut rng, &sender, value_to_send);
    let note_commitment = note.commit();
    let nk = *sk_sender.nullifier_key();
    let mut sct = tct::Tree::new();

    sct.insert(tct::Witness::Keep, note_commitment).unwrap();
    let state_commitment_proof = sct.witness(note_commitment).unwrap();
    let position = state_commitment_proof.position();
    let nullifier = Nullifier::derive(&nk, state_commitment_proof.position(), &note_commitment);

    let public = NullifierDerivationProofPublic {
        position,
        note_commitment,
        nullifier,
    };
    let private = NullifierDerivationProofPrivate { nk };
    let proof = NullifierDerivationProof::prove(&mut rng, pk, public.clone(), private)
        .expect("can create proof");

    let proof_result = proof.verify(vk, public);

    assert!(proof_result.is_ok());
}
