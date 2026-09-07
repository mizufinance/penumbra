use anyhow::{anyhow, ensure, Context, Error};
use decaf377::{Fq, Fr};
use decaf377_rdsa::{Signature, SpendAuth};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::Balance;
use shieldd_sdk_keys::symmetric::{PayloadKey, WrappedMemoKey};
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_tct as tct;
#[cfg(any(unix, windows))]
use shieldd_sdk_txhash::EffectingData;
use std::convert::{TryFrom, TryInto};

use crate::discovery::{self, Parameters};

#[cfg(any(unix, windows))]
use super::{NoteReshape, NoteReshapeProof};
use super::{
    NoteReshapeBody, NoteReshapeFamilyId, NoteReshapeInputBody, NoteReshapeInputPrivate,
    NoteReshapeInputPublic, NoteReshapeOutputBody, NoteReshapeOutputPrivate,
    NoteReshapeOutputPublic, NoteReshapeProofPrivate, NoteReshapeProofPublic,
};
use crate::note_reshape_padding::{dummy_state_commitment_proof, pad_to_len, HiddenArityPadder};
use crate::{ShieldedInputPlan, ShieldedOutputPlan};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(try_from = "pb::NoteReshapePlan", into = "pb::NoteReshapePlan")]
pub struct NoteReshapePlan {
    pub family_id: NoteReshapeFamilyId,
    pub value_blinding: Fr,
    pub spends: Vec<ShieldedInputPlan>,
    pub outputs: Vec<ShieldedOutputPlan>,
    pub routing_parameters: Parameters,
    pub compliance: crate::NoteReshapeContext,
}

impl NoteReshapePlan {
    pub fn new(
        family_id: NoteReshapeFamilyId,
        spends: Vec<ShieldedInputPlan>,
        outputs: Vec<ShieldedOutputPlan>,
        value_blinding: Fr,
        compliance: crate::NoteReshapeContext,
        routing_parameters: Parameters,
    ) -> anyhow::Result<Self> {
        let plan = Self {
            family_id,
            value_blinding,
            spends,
            outputs,
            routing_parameters,
            compliance,
        };
        plan.validate()?;
        Ok(plan)
    }

    fn padder_for(
        spends: &[ShieldedInputPlan],
        _outputs: &[ShieldedOutputPlan],
        value_blinding: Fr,
    ) -> HiddenArityPadder {
        let first_spend = spends
            .first()
            .expect("note reshape must contain a real spend");
        HiddenArityPadder {
            value_blinding,
            first_spend_randomizer: first_spend.randomizer,
            sender_address: first_spend.note.address(),
            asset_id: first_spend.note.asset_id(),
            nullifier_domain_sep_label: b"shieldd.note_reshape.synthetic_dummy.nullifier",
            nullifier_seed_label: b"shieldd.note_reshape.synthetic_dummy.nullifier_seed",
            spend_auth_key_label: b"shieldd.note_reshape.synthetic_dummy.spend_auth_key",
            spend_auth_randomizer_label:
                b"shieldd.note_reshape.synthetic_dummy.spend_auth_randomizer",
            input_note_label: b"shieldd.note_reshape.synthetic_dummy.input_note",
            output_note_label: b"shieldd.note_reshape.synthetic_dummy.output_note",
        }
    }

    fn padder(&self) -> HiddenArityPadder {
        Self::padder_for(&self.spends, &self.outputs, self.value_blinding)
    }

    pub fn synthetic_dummy_auth_sig(
        &self,
        slot: usize,
        effect_hash: &[u8],
    ) -> Signature<SpendAuth> {
        self.padder().synthetic_dummy_auth_sig(slot, effect_hash)
    }

    fn encrypted_output_body(
        note: crate::Note,
        fvk: &FullViewingKey,
        memo_key: &PayloadKey,
        action_balance_commitment: shieldd_sdk_asset::balance::Commitment,
    ) -> NoteReshapeOutputBody {
        let wrapped_memo_key = WrappedMemoKey::encrypt(
            memo_key,
            note.ephemeral_secret_key(),
            note.transmission_key(),
            &note.diversified_generator(),
        );
        let ovk_wrapped_key = note.encrypt_key(fvk.outgoing(), action_balance_commitment);
        NoteReshapeOutputBody {
            note_payload: note.payload(),
            wrapped_memo_key,
            ovk_wrapped_key,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            !self.spends.is_empty(),
            "note_reshape requires at least one spend"
        );
        ensure!(
            !self.outputs.is_empty(),
            "note_reshape requires at least one output"
        );
        self.family_id
            .validate_real_counts(self.spends.len(), self.outputs.len())?;
        let first_spend = self
            .spends
            .first()
            .ok_or_else(|| anyhow!("note_reshape requires at least one spend"))?;
        let shared_address = first_spend.note.address();
        let shared_asset_id = first_spend.note.asset_id();
        ensure!(
            self.spends
                .iter()
                .all(|spend| spend.note.address() == shared_address),
            "note_reshape spends must all use the same address",
        );
        ensure!(
            self.outputs
                .iter()
                .all(|output| output.dest_address == shared_address),
            "note_reshape outputs must all use the same address as spends",
        );
        ensure!(
            self.spends
                .iter()
                .all(|spend| spend.note.asset_id() == shared_asset_id)
                && self
                    .outputs
                    .iter()
                    .all(|output| output.value.asset_id == shared_asset_id),
            "note_reshape requires all spends and outputs to use the same asset",
        );
        self.compliance
            .witness
            .validate(first_spend.note.asset_id(), &first_spend.note.address())?;
        Ok(())
    }

    pub fn family_id(&self) -> NoteReshapeFamilyId {
        self.family_id
    }

    pub fn balance(&self) -> Balance {
        self.spends
            .iter()
            .fold(Balance::default(), |mut acc, spend| {
                acc += spend.balance();
                acc
            })
            + self
                .outputs
                .iter()
                .fold(Balance::default(), |mut acc, output| {
                    acc += output.balance();
                    acc
                })
    }

    pub fn note_reshape_public_private(
        &self,
        fvk: &FullViewingKey,
        state_commitment_proofs: &[tct::Proof],
        anchor: tct::Root,
        recent_position_floor: u64,
    ) -> Result<(NoteReshapeProofPublic, NoteReshapeProofPrivate), crate::ProofError> {
        self.validate()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if state_commitment_proofs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "note_reshape expected {} state commitment proofs, got {}",
                self.spends.len(),
                state_commitment_proofs.len()
            )));
        }

        let padder = self.padder();
        let mut input_publics = self
            .spends
            .iter()
            .map(|spend| {
                Ok(NoteReshapeInputPublic {
                    nullifier: spend.nullifier(fvk),
                    rk: spend.rk(fvk),
                    history_required: shieldd_sdk_sct::nullifier_generation::is_old(
                        u64::from(spend.position),
                        recent_position_floor,
                    )
                    .map_err(|error| crate::ProofError::InvalidPublicInput(error.to_string()))?,
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;
        pad_to_len(&mut input_publics, self.family_id().input_count(), |slot| {
            NoteReshapeInputPublic {
                nullifier: padder.synthetic_dummy_nullifier(slot),
                rk: padder.synthetic_dummy_verification_key(slot),
                history_required: false,
            }
        });

        let mut output_publics = self
            .outputs
            .iter()
            .map(|output| {
                Ok(NoteReshapeOutputPublic {
                    note_commitment: output.output_note().commit(),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;
        pad_to_len(
            &mut output_publics,
            self.family_id().output_count(),
            |slot| NoteReshapeOutputPublic {
                note_commitment: padder.synthetic_dummy_output_note(slot).commit(),
            },
        );

        let mut input_privates = self
            .spends
            .iter()
            .zip(state_commitment_proofs.iter().cloned())
            .map(|(spend, state_commitment_proof)| {
                Ok(NoteReshapeInputPrivate {
                    state_commitment_proof,
                    spent_note: spend.note.clone(),
                    spend_auth_randomizer: spend.randomizer,
                    is_dummy: false,
                    dummy_nullifier_seed: Fq::from(0u64),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;
        pad_to_len(
            &mut input_privates,
            self.family_id().input_count(),
            |slot| NoteReshapeInputPrivate {
                state_commitment_proof: dummy_state_commitment_proof(
                    padder.synthetic_dummy_input_note(slot).commit(),
                ),
                spent_note: padder.synthetic_dummy_input_note(slot),
                spend_auth_randomizer: padder.synthetic_dummy_spend_auth_randomizer(slot),
                is_dummy: true,
                dummy_nullifier_seed: padder.synthetic_dummy_nullifier_seed(slot),
            },
        );

        let mut output_privates = self
            .outputs
            .iter()
            .map(|output| {
                Ok(NoteReshapeOutputPrivate {
                    created_note: output.output_note(),
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;
        pad_to_len(
            &mut output_privates,
            self.family_id().output_count(),
            |slot| NoteReshapeOutputPrivate {
                created_note: padder.synthetic_dummy_output_note(slot),
            },
        );

        let first_spend = &self.spends[0];
        let routing_nonce = Fq::from_le_bytes_mod_order(&self.compliance.nonce.to_bytes());
        let routing_tag = discovery::single_tag(
            &first_spend.note.address(),
            self.compliance.witness.asset.is_regulated,
            &self.routing_parameters,
            routing_nonce,
        );

        Ok((
            NoteReshapeProofPublic {
                family_id: self.family_id,
                anchor,
                balance_commitment: self.balance().commit(self.value_blinding),
                asset_anchor: self.compliance.witness.asset.root,
                compliance_anchor: self.compliance.witness.user_root,
                routing_tag,
                routing_parameter_set_id: self.routing_parameters.id(),
                recent_position_floor,
                inputs: input_publics,
                outputs: output_publics,
            },
            NoteReshapeProofPrivate {
                family_id: self.family_id,
                action_balance_blinding: self.value_blinding,
                ak: *fvk.spend_verification_key(),
                nk: *fvk.nullifier_key(),
                asset_path: self.compliance.witness.asset.path.clone(),
                asset_position: self.compliance.witness.asset.position,
                asset_indexed_leaf: self.compliance.witness.asset.leaf.clone(),
                sender_compliance_path: self.compliance.witness.sender.path.clone(),
                sender_compliance_position: self.compliance.witness.sender.position,
                sender_leaf: self.compliance.witness.sender.leaf.clone(),
                is_regulated: self.compliance.witness.asset.is_regulated,
                routing_parameters: self.routing_parameters.clone(),
                routing_nonce,
                inputs: input_privates,
                outputs: output_privates,
            },
        ))
    }

    pub fn note_reshape_body(
        &self,
        fvk: &FullViewingKey,
        memo_key: &PayloadKey,
        anchor: tct::Root,
        recent_position_floor: u64,
    ) -> anyhow::Result<NoteReshapeBody> {
        self.validate()?;
        let padder = self.padder();
        let action_balance_commitment = self.balance().commit(self.value_blinding);
        let mut inputs = self
            .spends
            .iter()
            .map(|spend| {
                let spend_body = spend.action_input_body(fvk, recent_position_floor)?;
                Ok(NoteReshapeInputBody {
                    nullifier: spend_body.nullifier,
                    rk: spend_body.rk,
                    encrypted_backref: spend_body.encrypted_backref,
                    history_required: spend_body.history_required,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        pad_to_len(&mut inputs, self.family_id().input_count(), |slot| {
            let nullifier = padder.synthetic_dummy_nullifier(slot);
            let backref = crate::Backref::new(padder.synthetic_dummy_input_note(slot).commit());
            NoteReshapeInputBody {
                nullifier,
                rk: padder.synthetic_dummy_verification_key(slot),
                encrypted_backref: backref.encrypt(&fvk.backref_key(), &nullifier),
                history_required: false,
            }
        });
        let mut outputs = self
            .outputs
            .iter()
            .map(|output| {
                Self::encrypted_output_body(
                    output.output_note(),
                    fvk,
                    memo_key,
                    action_balance_commitment,
                )
            })
            .collect::<Vec<_>>();
        pad_to_len(&mut outputs, self.family_id().output_count(), |slot| {
            Self::encrypted_output_body(
                padder.synthetic_dummy_output_note(slot),
                fvk,
                memo_key,
                action_balance_commitment,
            )
        });

        let first_spend = &self.spends[0];
        let routing_nonce = Fq::from_le_bytes_mod_order(&self.compliance.nonce.to_bytes());
        let routing_tag = discovery::single_tag(
            &first_spend.note.address(),
            self.compliance.witness.asset.is_regulated,
            &self.routing_parameters,
            routing_nonce,
        );

        Ok(NoteReshapeBody {
            family_id: self.family_id,
            anchor,
            balance_commitment: action_balance_commitment,
            inputs,
            outputs,
            routing_tag,
            routing_parameter_set_id: self.routing_parameters.id(),
            asset_anchor: self.compliance.witness.asset.root,
            compliance_anchor: self.compliance.witness.user_root,
        })
    }

    #[cfg(any(unix, windows))]
    pub fn note_reshape(
        &self,
        fvk: &FullViewingKey,
        auth_sigs: Vec<Signature<SpendAuth>>,
        state_commitment_proofs: Vec<tct::Proof>,
        anchor: tct::Root,
        memo_key: &PayloadKey,
        recent_position_floor: u64,
    ) -> Result<NoteReshape, crate::ProofError> {
        let body = self
            .note_reshape_body(fvk, memo_key, anchor, recent_position_floor)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if auth_sigs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "note_reshape expected {} auth sigs, got {}",
                self.spends.len(),
                auth_sigs.len()
            )));
        }
        let mut auth_sigs = auth_sigs;
        pad_to_len(&mut auth_sigs, self.family_id().auth_sig_count(), |slot| {
            self.synthetic_dummy_auth_sig(slot, body.effect_hash().as_ref())
        });
        let (public, private) = self.note_reshape_public_private(
            fvk,
            &state_commitment_proofs,
            anchor,
            recent_position_floor,
        )?;
        let proof = NoteReshapeProof::prove(public, private)?;

        Ok(NoteReshape {
            body,
            auth_sigs,
            proof,
        })
    }
}

impl DomainType for NoteReshapePlan {
    type Proto = pb::NoteReshapePlan;
}

impl From<NoteReshapePlan> for pb::NoteReshapePlan {
    fn from(msg: NoteReshapePlan) -> Self {
        Self {
            family_id: msg.family_id.into(),
            value_blinding: msg.value_blinding.to_bytes_le().to_vec(),
            spends: msg.spends.into_iter().map(Into::into).collect(),
            outputs: msg.outputs.into_iter().map(Into::into).collect(),
            routing_parameters: Some(msg.routing_parameters.into()),
            compliance: Some(msg.compliance.into()),
        }
    }
}

impl TryFrom<pb::NoteReshapePlan> for NoteReshapePlan {
    type Error = Error;

    fn try_from(proto: pb::NoteReshapePlan) -> Result<Self, Self::Error> {
        let plan = Self {
            family_id: proto.family_id.try_into()?,
            value_blinding: Fr::from_bytes_checked(
                proto
                    .value_blinding
                    .as_slice()
                    .try_into()
                    .context("malformed value blinding")?,
            )
            .map_err(|_| anyhow::anyhow!("invalid value blinding"))?,
            spends: proto
                .spends
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            outputs: proto
                .outputs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            compliance: proto
                .compliance
                .ok_or_else(|| anyhow!("missing action compliance context"))?
                .try_into()?,
            routing_parameters: proto
                .routing_parameters
                .ok_or_else(|| anyhow!("missing routing parameters"))?
                .try_into()?,
        };
        plan.validate()?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use rand_core::OsRng;
    use shieldd_sdk_asset::{Value, BASE_ASSET_ID};
    use shieldd_sdk_keys::test_keys;
    use shieldd_sdk_num::Amount;

    fn two_to_one_plan() -> NoteReshapePlan {
        let mut rng = OsRng;
        let spends = [3u64, 5u64]
            .into_iter()
            .map(|amount| {
                let note = crate::Note::generate(
                    &mut rng,
                    &test_keys::ADDRESS_0,
                    Value {
                        amount: Amount::from(amount),
                        asset_id: *BASE_ASSET_ID,
                    },
                );
                ShieldedInputPlan::new(&mut rng, note, 0u64.into())
            })
            .collect();
        let output = ShieldedOutputPlan::new(
            &mut rng,
            Value {
                amount: Amount::from(8u64),
                asset_id: *BASE_ASSET_ID,
            },
            test_keys::ADDRESS_0.clone(),
        );

        crate::test_plan_helpers::note_reshape(
            NoteReshapeFamilyId::EightByOne,
            spends,
            vec![output],
            Fr::from(7u64),
        )
        .expect("valid two-to-one plan")
    }

    fn plan_for_family(family_id: NoteReshapeFamilyId) -> NoteReshapePlan {
        let real_inputs = family_id.min_real_inputs();
        let real_outputs = family_id.min_real_outputs();
        let mut rng = OsRng;
        let spends = (0..real_inputs)
            .map(|_| {
                let note = crate::Note::generate(
                    &mut rng,
                    &test_keys::ADDRESS_0,
                    Value {
                        amount: Amount::from(real_outputs as u64),
                        asset_id: *BASE_ASSET_ID,
                    },
                );
                ShieldedInputPlan::new(&mut rng, note, 0u64.into())
            })
            .collect();
        let outputs = (0..real_outputs)
            .map(|_| {
                ShieldedOutputPlan::new(
                    &mut rng,
                    Value {
                        amount: Amount::from(real_inputs as u64),
                        asset_id: *BASE_ASSET_ID,
                    },
                    test_keys::ADDRESS_0.clone(),
                )
            })
            .collect();
        crate::test_plan_helpers::note_reshape(family_id, spends, outputs, Fr::from(17u64))
            .expect("family plan must be valid")
    }

    fn assert_validation_and_decode_reject(plan: NoteReshapePlan, expected: &str) {
        let error = plan
            .validate()
            .expect_err("mutated domain plan must fail validation");
        assert!(
            error.to_string().contains(expected),
            "unexpected validation error: {error}"
        );

        let error = NoteReshapePlan::try_from(pb::NoteReshapePlan::from(plan))
            .expect_err("serialized mutated plan must fail decoding");
        assert!(
            error.to_string().contains(expected),
            "unexpected decoding error: {error}"
        );
    }

    #[test]
    fn note_reshape_family_specs_cover_expected_shapes() {
        assert_eq!(NoteReshapeFamilyId::EightByOne.input_count(), 8);
        assert_eq!(
            NoteReshapeFamilyId::smallest_covering(2, 1),
            Some(NoteReshapeFamilyId::EightByOne)
        );
        assert_eq!(
            NoteReshapeFamilyId::smallest_covering(3, 1),
            Some(NoteReshapeFamilyId::EightByOne)
        );
        assert_eq!(
            NoteReshapeFamilyId::smallest_covering(5, 1),
            Some(NoteReshapeFamilyId::EightByOne)
        );
        assert_eq!(
            NoteReshapeFamilyId::smallest_covering(1, 2),
            Some(NoteReshapeFamilyId::OneByEight)
        );
    }

    #[test]
    fn plan_proto_roundtrip_preserves_family_and_derives_balance() {
        let plan = two_to_one_plan();
        let expected_balance = plan.balance();
        let proto: pb::NoteReshapePlan = plan.into();
        assert_eq!(proto.family_id, u32::from(NoteReshapeFamilyId::EightByOne));

        let decoded = NoteReshapePlan::try_from(proto).expect("canonical plan should roundtrip");
        assert_eq!(decoded.family_id(), NoteReshapeFamilyId::EightByOne);
        assert_eq!(decoded.balance(), expected_balance);
    }

    #[test]
    fn plan_proto_rejects_missing_or_unknown_family() {
        let mut missing_family: pb::NoteReshapePlan = two_to_one_plan().into();
        missing_family.family_id = 0;
        let error = NoteReshapePlan::try_from(missing_family)
            .expect_err("missing family selector must be rejected");
        assert!(error
            .to_string()
            .contains("unknown note reshape family id 0"));

        let mut unknown_family: pb::NoteReshapePlan = two_to_one_plan().into();
        unknown_family.family_id = u32::MAX;
        let error = NoteReshapePlan::try_from(unknown_family)
            .expect_err("unknown family selector must be rejected");
        assert!(error
            .to_string()
            .contains("unknown note reshape family id 4294967295"));
    }

    #[test]
    fn validation_rejects_every_cross_record_invariant_mutation() {
        let mut empty_spends = two_to_one_plan();
        empty_spends.spends.clear();
        assert_validation_and_decode_reject(empty_spends, "at least one spend");

        let mut empty_outputs = two_to_one_plan();
        empty_outputs.outputs.clear();
        assert_validation_and_decode_reject(empty_outputs, "at least one output");

        let mut wrong_input_count = two_to_one_plan();
        wrong_input_count.spends.pop();
        assert_validation_and_decode_reject(
            wrong_input_count,
            "does not support 1 real inputs and 1 real outputs",
        );

        let mut wrong_output_count = two_to_one_plan();
        wrong_output_count
            .outputs
            .push(wrong_output_count.outputs[0].clone());
        assert_validation_and_decode_reject(
            wrong_output_count,
            "does not support 2 real inputs and 2 real outputs",
        );

        let mut mismatched_spend_owner = two_to_one_plan();
        let spend_value = mismatched_spend_owner.spends[1].note.value();
        mismatched_spend_owner.spends[1].note =
            crate::Note::generate(&mut OsRng, &test_keys::ADDRESS_1, spend_value);
        assert_validation_and_decode_reject(
            mismatched_spend_owner,
            "spends must all use the same address",
        );

        let mut mismatched_output_owner = two_to_one_plan();
        mismatched_output_owner.outputs[0].dest_address = test_keys::ADDRESS_1.clone();
        assert_validation_and_decode_reject(
            mismatched_output_owner,
            "outputs must all use the same address",
        );

        let alternate_asset = shieldd_sdk_asset::asset::Id(Fq::from(0xA55E7u64));
        let mut mismatched_spend_asset = two_to_one_plan();
        let mut spend_value = mismatched_spend_asset.spends[1].note.value();
        spend_value.asset_id = alternate_asset;
        mismatched_spend_asset.spends[1].note =
            crate::Note::generate(&mut OsRng, &test_keys::ADDRESS_0, spend_value);
        assert_validation_and_decode_reject(
            mismatched_spend_asset,
            "all spends and outputs to use the same asset",
        );

        let mut mismatched_output_asset = two_to_one_plan();
        mismatched_output_asset.outputs[0].value.asset_id = alternate_asset;
        assert_validation_and_decode_reject(
            mismatched_output_asset,
            "all spends and outputs to use the same asset",
        );
    }

    #[test]
    fn materializers_reject_proof_and_auth_count_mismatches() {
        let plan = two_to_one_plan();
        let anchor = tct::Tree::default().root();

        let error = plan
            .note_reshape_public_private(&test_keys::FULL_VIEWING_KEY, &[], anchor, 0)
            .expect_err("proof materialization must require one proof per real spend");
        assert!(error
            .to_string()
            .contains("note_reshape expected 2 state commitment proofs, got 0"));

        let error = plan
            .note_reshape(
                &test_keys::FULL_VIEWING_KEY,
                Vec::new(),
                Vec::new(),
                anchor,
                &PayloadKey::random_key(&mut OsRng),
                0,
            )
            .expect_err("action materialization must require one signature per real spend");
        assert!(error
            .to_string()
            .contains("note_reshape expected 2 auth sigs, got 0"));
    }

    #[test]
    fn all_note_reshape_outputs_wrap_under_serialized_action_commitment() {
        let mut rng = OsRng;
        let memo_key = PayloadKey::random_key(&mut rng);
        for family_id in NoteReshapeFamilyId::ALL {
            let plan = plan_for_family(family_id);
            let body = plan
                .note_reshape_body(
                    &test_keys::FULL_VIEWING_KEY,
                    &memo_key,
                    tct::Tree::default().root(),
                    0,
                )
                .expect("family body must build");
            assert_eq!(body.outputs.len(), family_id.output_count());
            for output in &body.outputs {
                crate::Note::decrypt_key(
                    output.ovk_wrapped_key.clone(),
                    output.note_payload.note_commitment,
                    body.balance_commitment,
                    test_keys::FULL_VIEWING_KEY.outgoing(),
                    &output.note_payload.ephemeral_key,
                )
                .unwrap_or_else(|_| {
                    panic!(
                        "{} output key did not unwrap under the serialized action commitment",
                        family_id.label()
                    )
                });
            }
        }
    }

    #[test]
    fn synthetic_input_slots_have_distinct_nullifiers() {
        for family_id in [NoteReshapeFamilyId::EightByOne] {
            let (public, private) =
                crate::test_proof_helpers::proof_test_helpers::build_note_reshape_roundtrip_inputs(
                    family_id,
                );
            let all = public
                .inputs
                .iter()
                .map(|input| input.nullifier)
                .collect::<HashSet<_>>();
            assert_eq!(
                all.len(),
                public.inputs.len(),
                "{} produced duplicate input nullifiers",
                family_id.label()
            );

            let synthetic = public
                .inputs
                .iter()
                .zip(&private.inputs)
                .filter(|(_, input)| input.is_dummy)
                .map(|(input, _)| input.nullifier)
                .collect::<HashSet<_>>();
            assert_eq!(
                synthetic.len(),
                private.inputs.iter().filter(|input| input.is_dummy).count(),
                "{} produced duplicate synthetic nullifiers",
                family_id.label()
            );
        }
    }
}
