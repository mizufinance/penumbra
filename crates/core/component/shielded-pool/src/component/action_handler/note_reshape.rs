use anyhow::{Context, Result};
use cnidarium::StateWrite;
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
use shieldd_sdk_keys::ensure_nonidentity_spend_auth_key;
use shieldd_sdk_proto::{DomainType as _, StateWriteProto as _};
use shieldd_sdk_sct::component::{source::SourceContext, tree::SctManager};
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_txhash::TransactionContext;

use crate::{component::NoteManager, event, NotePayload};

pub(crate) struct NoteReshapeInputPublicParts {
    pub nullifier: Nullifier,
    pub rk: VerificationKey<SpendAuth>,
}

pub(crate) struct NoteReshapeOutputPublicParts {
    pub note_commitment: shieldd_sdk_tct::StateCommitment,
}

pub(crate) fn validate_action_anchor(
    action_label: &str,
    action_anchor: shieldd_sdk_tct::Root,
    context: &TransactionContext,
) -> Result<()> {
    anyhow::ensure!(
        action_anchor == context.anchor,
        "{action_label} body anchor does not match transaction anchor"
    );
    Ok(())
}

pub(crate) fn verify_auth_sigs<I>(
    action_label: &str,
    inputs: &[I],
    auth_sigs: &[Signature<SpendAuth>],
    context: &TransactionContext,
    rk: impl Fn(&I) -> &VerificationKey<SpendAuth>,
) -> Result<()> {
    anyhow::ensure!(
        inputs.len() == auth_sigs.len(),
        "{action_label} expected {} auth sigs, got {}",
        inputs.len(),
        auth_sigs.len()
    );
    for (index, (input, auth_sig)) in inputs.iter().zip(auth_sigs.iter()).enumerate() {
        let rk = rk(input);
        ensure_nonidentity_spend_auth_key(
            rk,
            &format!("{action_label} randomized spend key {index}"),
        )?;
        rk.verify(context.effect_hash.as_ref(), auth_sig)
            .with_context(|| format!("{action_label} auth signature {index} failed to verify"))?;
    }
    Ok(())
}

pub(crate) fn extract_public_parts<I, O>(
    inputs: &[I],
    outputs: &[O],
    input_parts: impl Fn(&I) -> (Nullifier, &VerificationKey<SpendAuth>),
    output_parts: impl Fn(&O) -> &NotePayload,
) -> (
    Vec<NoteReshapeInputPublicParts>,
    Vec<NoteReshapeOutputPublicParts>,
) {
    let inputs = inputs
        .iter()
        .map(|input| {
            let (nullifier, rk) = input_parts(input);
            NoteReshapeInputPublicParts { nullifier, rk: *rk }
        })
        .collect();
    let outputs = outputs
        .iter()
        .map(|output| NoteReshapeOutputPublicParts {
            note_commitment: output_parts(output).note_commitment,
        })
        .collect();
    (inputs, outputs)
}

pub(crate) async fn execute_proof_bound_effects<S, I, O>(
    state: &mut S,
    inputs: &[I],
    outputs: &[O],
    input_nullifier: impl Fn(&I) -> Nullifier,
    output_note_payload: impl Fn(&O) -> &NotePayload,
) -> Result<()>
where
    S: StateWrite,
{
    // Every fixed-shape input and output is proof-bound. Consensus does not
    // classify private padding or suppress any serialized slot.
    let nullifiers = inputs
        .iter()
        .map(input_nullifier)
        .collect::<Vec<Nullifier>>();

    let source = state
        .get_current_source()
        .ok_or_else(|| anyhow::anyhow!("source should be set during execution"))?;

    // Batch insertion checks committed conflicts and duplicate nullifiers
    // before mutating state, so a malformed action cannot partially nullify.
    state.nullify_all(&nullifiers, source.into()).await?;
    for nullifier in nullifiers {
        state.record_proto(event::EventNullifierSpent { nullifier }.to_proto());
    }
    for output in outputs {
        let note_payload = output_note_payload(output).clone();
        let note_commitment = note_payload.note_commitment;
        state.add_note_payload(note_payload, source.into()).await;
        state.record_proto(event::EventNoteCreated { note_commitment }.to_proto());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cnidarium::{StateDelta, TempStorage};
    use decaf377::{Fq, Fr};
    use decaf377_rdsa::SigningKey;
    use rand_core::OsRng;
    use shieldd_sdk_sct::component::tree::SctRead;
    use shieldd_sdk_txhash::TransactionId;

    struct TestInput(Nullifier);

    #[test]
    fn action_anchor_must_match_transaction_context() {
        let context = TransactionContext {
            anchor: shieldd_sdk_tct::Tree::default().root(),
            effect_hash: Default::default(),
            recent_position_floor: 0,
        };
        validate_action_anchor("test action", context.anchor, &context)
            .expect("matching anchor should pass");

        let other_anchor =
            shieldd_sdk_tct::Root(shieldd_sdk_tct::structure::Hash::new(Fq::from(1u64)));
        let err = validate_action_anchor("test action", other_anchor, &context)
            .expect_err("mismatched action anchor should fail");
        assert!(
            err.to_string()
                .contains("body anchor does not match transaction anchor"),
            "unexpected error: {err:#}"
        );
    }

    async fn run_execute<S: StateWrite>(
        state: &mut S,
        nullifier: Nullifier,
        outputs: &[NotePayload],
    ) -> Result<()> {
        execute_proof_bound_effects::<_, TestInput, NotePayload>(
            state,
            &[TestInput(nullifier)],
            outputs,
            |input| input.0,
            |payload| payload,
        )
        .await
    }

    /// Assurance-case R2.2 (evidence gap #1): the check-then-nullify handler
    /// path rejects a nullifier that was already spent.
    #[tokio::test]
    async fn execute_rejects_repeated_nullifier() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        shieldd_sdk_sct::nullifier_tree::initialize(&mut state).await?;
        shieldd_sdk_sct::component::clock::EpochManager::put_block_height(&mut state, 1);
        state.put_current_source(Some(TransactionId([7u8; 32])));
        let nullifier = Nullifier(Fq::from(42u64));

        run_execute(&mut state, nullifier, &[]).await?;

        let err = run_execute(&mut state, nullifier, &[])
            .await
            .expect_err("second spend of the same nullifier must be rejected");
        assert!(
            err.to_string().contains("already spent"),
            "unexpected rejection reason: {err:#}"
        );

        // A distinct nullifier is still accepted after the rejection.
        run_execute(&mut state, Nullifier(Fq::from(43u64)), &[]).await?;
        Ok(())
    }

    #[tokio::test]
    async fn execute_rejects_duplicate_nullifiers_before_mutation() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        shieldd_sdk_sct::nullifier_tree::initialize(&mut state).await?;
        shieldd_sdk_sct::component::clock::EpochManager::put_block_height(&mut state, 1);
        state.put_current_source(Some(TransactionId([10u8; 32])));
        let duplicate = Nullifier(Fq::from(47u64));

        let error = execute_proof_bound_effects::<_, TestInput, NotePayload>(
            &mut state,
            &[TestInput(duplicate), TestInput(duplicate)],
            &[],
            |input| input.0,
            |payload| payload,
        )
        .await
        .expect_err("duplicate proof-bound nullifiers must be rejected atomically");
        assert!(
            error.to_string().contains("duplicate nullifier"),
            "unexpected rejection reason: {error:#}"
        );
        assert!(
            state.pending_nullifiers().is_empty(),
            "duplicate rejection must not stage a partial nullifier write"
        );
        Ok(())
    }

    #[tokio::test]
    async fn note_reshape_persists_every_proof_bound_nullifier() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        shieldd_sdk_sct::nullifier_tree::initialize(&mut state).await?;
        shieldd_sdk_sct::component::clock::EpochManager::put_block_height(&mut state, 1);
        state.put_current_source(Some(TransactionId([8u8; 32])));
        let real_nullifier = Nullifier(Fq::from(44u64));

        run_execute(&mut state, real_nullifier, &[]).await?;

        let err = run_execute(&mut state, real_nullifier, &[])
            .await
            .expect_err("a proof-bound nullifier must be persisted");
        assert!(
            err.to_string().contains("already spent"),
            "unexpected rejection reason: {err:#}"
        );
        Ok(())
    }

    #[tokio::test]
    async fn proof_bound_output_is_persisted() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        shieldd_sdk_sct::nullifier_tree::initialize(&mut state).await?;
        shieldd_sdk_sct::component::clock::EpochManager::put_block_height(&mut state, 1);
        state.put_current_source(Some(TransactionId([9u8; 32])));
        let output = NotePayload {
            note_commitment: shieldd_sdk_tct::StateCommitment(Fq::from(45u64)),
            ephemeral_key: decaf377_ka::Public([0u8; 32]),
            encrypted_note: crate::NoteCiphertext([0u8; crate::note::NOTE_CIPHERTEXT_BYTES]),
            recovery_capsule: None,
        };

        run_execute(
            &mut state,
            Nullifier(Fq::from(46u64)),
            std::slice::from_ref(&output),
        )
        .await?;

        let pending = state.pending_note_payloads();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1.note_commitment, output.note_commitment);
        Ok(())
    }

    #[test]
    fn auth_verification_rejects_invalid_dummy_slot_signature() {
        let real_sk = SigningKey::<SpendAuth>::from(Fr::from(11u64));
        let dummy_sk = SigningKey::<SpendAuth>::from(Fr::from(12u64));
        let wrong_dummy_sk = SigningKey::<SpendAuth>::from(Fr::from(13u64));
        let inputs = [
            VerificationKey::from(real_sk.clone()),
            VerificationKey::from(dummy_sk.clone()),
        ];
        let context = TransactionContext {
            anchor: shieldd_sdk_tct::Tree::default().root(),
            effect_hash: Default::default(),
            recent_position_floor: 0,
        };
        let signatures = [
            real_sk.sign(OsRng, context.effect_hash.as_ref()),
            wrong_dummy_sk.sign(OsRng, context.effect_hash.as_ref()),
        ];

        let err = verify_auth_sigs("note reshape", &inputs, &signatures, &context, |rk| rk)
            .expect_err("every padded RK, including a dummy slot, must verify");
        assert!(
            err.to_string().contains("auth signature 1 failed"),
            "unexpected rejection reason: {err:#}"
        );
    }

    #[test]
    fn note_reshape_auth_verification_rejects_identity_randomized_key() {
        let identity_sk = SigningKey::<SpendAuth>::from(Fr::from(0u64));
        let identity_rk = VerificationKey::from(identity_sk.clone());
        let context = TransactionContext {
            anchor: shieldd_sdk_tct::Tree::default().root(),
            effect_hash: Default::default(),
            recent_position_floor: 0,
        };
        let different_message = b"different note reshape authorization hash";
        assert_ne!(&different_message[..], context.effect_hash.as_ref());
        let signature = identity_sk.sign_deterministic(different_message);

        identity_rk
            .verify(context.effect_hash.as_ref(), &signature)
            .expect("the pinned RDSA primitive admits identity keys across messages");
        let error = verify_auth_sigs(
            "note_reshape",
            &[identity_rk],
            &[signature],
            &context,
            |rk| rk,
        )
        .expect_err("identity randomized spend keys must fail before RDSA verification");
        assert!(
            error
                .to_string()
                .contains("randomized spend key 0 must not be identity"),
            "unexpected rejection reason: {error:#}"
        );
    }
}
