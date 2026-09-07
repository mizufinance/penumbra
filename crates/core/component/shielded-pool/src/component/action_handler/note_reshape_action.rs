use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use shieldd_sdk_compliance::ComplianceRegistryRead as _;
use shieldd_sdk_proof_params::batch::{self, BatchItem, VerifiedBatchItem};
use shieldd_sdk_txhash::TransactionContext;

use crate::{
    component::action_handler::note_reshape, NoteReshape, NoteReshapeInputPublic,
    NoteReshapeOutputPublic, NoteReshapeProofPublic,
};

fn note_reshape_verify_auth_sigs(
    note_reshape: &NoteReshape,
    context: &TransactionContext,
) -> Result<()> {
    note_reshape::verify_auth_sigs(
        "note_reshape",
        &note_reshape.body.inputs,
        &note_reshape.auth_sigs,
        context,
        |input| &input.rk,
    )
}

fn note_reshape_extract_public(
    note_reshape: &NoteReshape,
    context: &TransactionContext,
) -> Result<NoteReshapeProofPublic> {
    let (inputs, outputs) = note_reshape::extract_public_parts(
        &note_reshape.body.inputs,
        &note_reshape.body.outputs,
        |input| (input.nullifier, &input.rk),
        |output| &output.note_payload,
    );

    let public = NoteReshapeProofPublic {
        family_id: note_reshape.body.family_id,
        anchor: context.anchor,
        balance_commitment: note_reshape.body.balance_commitment,
        asset_anchor: note_reshape.body.asset_anchor,
        compliance_anchor: note_reshape.body.compliance_anchor,
        routing_tag: note_reshape.body.routing_tag,
        routing_parameter_set_id: note_reshape.body.routing_parameter_set_id,
        recent_position_floor: context.recent_position_floor,
        inputs: inputs
            .into_iter()
            .zip(note_reshape.body.inputs.iter())
            .map(|(input, body_input)| NoteReshapeInputPublic {
                nullifier: input.nullifier,
                rk: input.rk,
                history_required: body_input.history_required,
            })
            .collect(),
        outputs: outputs
            .into_iter()
            .zip(note_reshape.body.outputs.iter())
            .map(|(output, body_output)| {
                Ok(NoteReshapeOutputPublic {
                    note_commitment: output.note_commitment,
                    recovery_commitment: body_output
                        .note_payload
                        .recovery_capsule
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("missing note reshape recovery capsule"))?
                        .commitment(),
                })
            })
            .collect::<Result<Vec<_>>>()?,
    };
    public
        .validate_shape()
        .context("note_reshape proof family shape mismatch")?;
    Ok(public)
}

fn note_reshape_to_batch_item(
    note_reshape: &NoteReshape,
    public: NoteReshapeProofPublic,
) -> Result<BatchItem> {
    note_reshape.proof.to_batch_item(&public)
}

pub fn note_reshape_check_stateless_and_extract(
    note_reshape: &NoteReshape,
    context: &TransactionContext,
) -> Result<BatchItem> {
    note_reshape::validate_action_anchor("note_reshape", note_reshape.body.anchor, context)?;
    note_reshape.body.validate_shape()?;
    note_reshape_verify_auth_sigs(note_reshape, context)?;
    let public = note_reshape_extract_public(note_reshape, context)?;
    note_reshape_to_batch_item(note_reshape, public)
}

/// Execute a NoteReshape whose exact proof item has already verified.
pub async fn note_reshape_execute_verified<S: StateWrite>(
    note_reshape: &NoteReshape,
    context: &TransactionContext,
    verified_proof: &VerifiedBatchItem,
    mut state: S,
) -> Result<()> {
    let item = note_reshape_check_stateless_and_extract(note_reshape, context)?;
    verified_proof
        .ensure_binds(note_reshape.body.family_id.deployed_proof_key(), &item)
        .context("note_reshape verified proof capability mismatch")?;
    state
        .validate_compliance_anchors(
            &note_reshape.body.compliance_anchor,
            &note_reshape.body.asset_anchor,
        )
        .await?;
    note_reshape::execute_proof_bound_effects(
        &mut state,
        &note_reshape.body.inputs,
        &note_reshape.body.outputs,
        |input| input.nullifier,
        |output| &output.note_payload,
    )
    .await
}

#[async_trait]
impl ActionHandler for NoteReshape {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let item = note_reshape_check_stateless_and_extract(self, &context)?;
        batch::verify_each(
            self.body.family_id.proof_verification_key(),
            std::slice::from_ref(&item),
        )
        .map_err(|e| anyhow::anyhow!("note_reshape proof did not verify: {e}"))?;
        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, _state: S) -> Result<()> {
        anyhow::bail!("note_reshape execution requires an exact verified proof capability")
    }
}

#[cfg(test)]
mod tests {
    use rand_core::OsRng;
    use shieldd_sdk_asset::{Value, BASE_ASSET_ID};
    use shieldd_sdk_keys::{symmetric::PayloadKey, test_keys};
    use shieldd_sdk_num::Amount;
    use shieldd_sdk_sct::component::tree::SctRead as _;
    use shieldd_sdk_tct as tct;

    use super::*;
    use crate::{
        component::NoteManager as _, Note, NoteReshapeFamilyId, ShieldedInputPlan,
        ShieldedOutputPlan,
    };

    #[tokio::test]
    async fn note_reshape_projection_matches_and_raw_execution_fails_closed_for_every_family() {
        let mut rng = OsRng;
        let storage = cnidarium::TempStorage::new()
            .await
            .expect("temporary storage");
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        for family_id in NoteReshapeFamilyId::ALL {
            let input_count = family_id.min_real_inputs();
            let output_count = family_id.min_real_outputs();
            let total = input_count.max(output_count) as u64;
            let input_amount = total / input_count as u64;
            let output_amount = total / output_count as u64;
            assert_eq!(input_amount * input_count as u64, total);
            assert_eq!(output_amount * output_count as u64, total);

            let notes = (0..input_count)
                .map(|_| {
                    Note::generate(
                        &mut rng,
                        &test_keys::ADDRESS_0,
                        Value {
                            amount: Amount::from(input_amount),
                            asset_id: *BASE_ASSET_ID,
                        },
                    )
                })
                .collect::<Vec<_>>();
            let mut tree = tct::Tree::new();
            for note in &notes {
                tree.insert(tct::Witness::Keep, note.commit())
                    .expect("test SCT has capacity");
            }
            let anchor = tree.root();
            let proofs = notes
                .iter()
                .map(|note| {
                    tree.witness(note.commit())
                        .expect("inserted note is witnessed")
                })
                .collect::<Vec<_>>();
            let spends = notes
                .into_iter()
                .zip(&proofs)
                .map(|(note, proof)| ShieldedInputPlan::new(&mut rng, note, proof.position()))
                .collect();
            let outputs = (0..output_count)
                .map(|_| {
                    ShieldedOutputPlan::new(
                        &mut rng,
                        Value {
                            amount: Amount::from(output_amount),
                            asset_id: *BASE_ASSET_ID,
                        },
                        test_keys::ADDRESS_0.clone(),
                    )
                })
                .collect();
            let plan = crate::test_plan_helpers::note_reshape(
                family_id,
                spends,
                outputs,
                decaf377::Fr::from(7u64),
            )
            .expect("canonical family plan");
            let (proving_public, _) = plan
                .note_reshape_public_private(&test_keys::FULL_VIEWING_KEY, &proofs, anchor, 0)
                .expect("derive proving public");
            let action = NoteReshape {
                body: plan
                    .note_reshape_body(
                        &test_keys::FULL_VIEWING_KEY,
                        &PayloadKey::random_key(&mut rng),
                        anchor,
                        0,
                    )
                    .expect("derive action body"),
                auth_sigs: Vec::new(),
                proof: Default::default(),
            };
            let context = TransactionContext {
                anchor,
                effect_hash: Default::default(),
                recent_position_floor: 0,
            };
            let extracted =
                note_reshape_extract_public(&action, &context).expect("extract verifier public");

            assert_eq!(
                proving_public
                    .statement_hash()
                    .expect("proving statement hash"),
                extracted
                    .statement_hash()
                    .expect("extracted statement hash"),
                "{} action projection must equal the proving statement",
                family_id.label()
            );

            let error = action
                .check_and_execute(&mut state)
                .await
                .expect_err("raw NoteReshape execution must not mutate state");
            assert!(
                error
                    .to_string()
                    .contains("requires an exact verified proof capability"),
                "{} raw execution returned unexpected error: {error:#}",
                family_id.label()
            );
            assert!(state.pending_nullifiers().is_empty());
            assert!(state.pending_note_payloads().is_empty());
        }
    }
}
