use anyhow::{anyhow, ensure, Context, Error};
use decaf377::Fq;
use decaf377::Fr;
use decaf377_rdsa::{Signature, SpendAuth};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::{asset, Balance};
use shieldd_sdk_compliance::{
    derive_withdrawal_encryption_material, encrypt_withdrawal_with_material,
    withdrawal_encryption_key, WithdrawalEncryptionResult,
};
use shieldd_sdk_keys::symmetric::PayloadKey;
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_keys::{symmetric::WrappedMemoKey, Address};
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_tct as tct;
use shieldd_sdk_txhash::EffectingData;

use crate::discovery::{self, Parameters};
use crate::note_reshape_padding::dummy_spend_auth_sig;
use crate::note_reshape_padding::dummy_state_commitment_proof;
use crate::note_reshape_padding::{pad_to_len, HiddenArityPadder};
use crate::{
    Ics20Withdrawal, ShieldedIcs20WithdrawalChangeBody, ShieldedInputPlan, ShieldedOutputPlan,
    TransferInputBody,
};
use crate::{
    ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalChangePrivate,
    ShieldedIcs20WithdrawalChangePublic, ShieldedIcs20WithdrawalInputPublic,
    ShieldedIcs20WithdrawalOptionalInputPrivate, ShieldedIcs20WithdrawalProof,
    ShieldedIcs20WithdrawalProofPrivate, ShieldedIcs20WithdrawalProofPublic,
    ShieldedIcs20WithdrawalRequiredInputPrivate, VolumeAccumulatorPlan,
};

use super::{ShieldedIcs20WithdrawalBody, ShieldedIcs20WithdrawalFamilyId};

const PADDED_ICS20_WITHDRAWAL_INPUTS: usize = 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    try_from = "pb::ShieldedIcs20WithdrawalPlan",
    into = "pb::ShieldedIcs20WithdrawalPlan"
)]
pub struct ShieldedIcs20WithdrawalPlan {
    pub value_blinding: Fr,
    pub spends: Vec<ShieldedInputPlan>,
    pub change_output: Option<ShieldedOutputPlan>,
    pub withdrawal: Ics20Withdrawal,
    pub routing_parameters: Parameters,
    pub compliance: crate::WithdrawalContext,
    pub volume_accumulator: VolumeAccumulatorPlan,
}

impl ShieldedIcs20WithdrawalPlan {
    pub fn new(
        spends: Vec<ShieldedInputPlan>,
        change_output: Option<ShieldedOutputPlan>,
        withdrawal: Ics20Withdrawal,
        value_blinding: Fr,
        compliance: crate::WithdrawalContext,
        volume_accumulator: VolumeAccumulatorPlan,
        routing_parameters: Parameters,
    ) -> anyhow::Result<Self> {
        let plan = Self {
            value_blinding,
            spends,
            change_output,
            withdrawal,
            routing_parameters,
            compliance,
            volume_accumulator,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn family_id(&self) -> ShieldedIcs20WithdrawalFamilyId {
        ShieldedIcs20WithdrawalFamilyId::Canonical
    }

    pub fn accumulator_prior_commitment(&self) -> Option<tct::StateCommitment> {
        matches!(
            self.volume_accumulator,
            VolumeAccumulatorPlan::Continuation { .. }
        )
        .then(|| self.volume_accumulator.prior_commitment())
    }

    pub fn volume_accumulator_payload(
        &self,
        fvk: &FullViewingKey,
    ) -> crate::VolumeAccumulatorPayload {
        self.volume_accumulator.clone().selected_payload(
            fvk.nullifier_key(),
            fvk.outgoing(),
            Fq::from_le_bytes_mod_order(&self.compliance.nonce.to_bytes()),
            crate::TransferProofContext::Ordinary,
        )
    }

    pub fn balance(&self) -> Balance {
        self.spends
            .iter()
            .fold(Balance::default(), |mut acc, spend| {
                acc += spend.balance();
                acc
            })
            + self
                .change_output
                .iter()
                .fold(Balance::default(), |mut acc, output| {
                    acc -= Balance::from(output.value);
                    acc
                })
            + self.withdrawal.balance()
    }

    fn first_spend(&self) -> &ShieldedInputPlan {
        self.spends
            .first()
            .expect("shielded ICS-20 withdrawal plan must contain at least one real spend")
    }

    fn sender_address(&self) -> Address {
        self.first_spend().note.address()
    }

    pub fn created_output_address(&self) -> Address {
        self.change_output
            .as_ref()
            .map(|output| output.dest_address.clone())
            .unwrap_or_else(|| self.sender_address())
    }

    pub fn note_creating_output_count(&self) -> usize {
        1
    }

    fn withdrawal_asset_id(&self) -> asset::Id {
        self.first_spend().note.asset_id()
    }

    fn padder(&self) -> HiddenArityPadder {
        HiddenArityPadder {
            value_blinding: self.value_blinding,
            first_spend_randomizer: self.first_spend().randomizer,
            sender_address: self.sender_address(),
            asset_id: self.withdrawal_asset_id(),
            capk: self.compliance.witness.sender.leaf.capk,
            nullifier_domain_sep_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.nullifier",
            nullifier_seed_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.nullifier_seed",
            spend_auth_key_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.spend_auth_key",
            spend_auth_randomizer_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.spend_auth_randomizer",
            input_note_label: b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.input_note",
            output_note_label: b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.output_note",
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        self.withdrawal
            .validate()
            .context("invalid shielded ICS-20 withdrawal payload")?;
        ensure!(
            !self.spends.is_empty(),
            "shielded ICS-20 withdrawal requires at least one spend"
        );
        ensure!(
            self.spends.len() <= PADDED_ICS20_WITHDRAWAL_INPUTS,
            "shielded ICS-20 withdrawal supports at most two spends",
        );

        let first_spend = self
            .spends
            .first()
            .ok_or_else(|| anyhow!("shielded ICS-20 withdrawal requires at least one spend"))?;
        ensure!(
            self.withdrawal.denom.id() == first_spend.note.asset_id(),
            "shielded ICS-20 withdrawal payload asset must match spends",
        );

        for spend in &self.spends {
            ensure!(
                spend.note.asset_id() == first_spend.note.asset_id(),
                "shielded ICS-20 withdrawal spends must use the same asset",
            );
            ensure!(
                spend.note.address() == first_spend.note.address(),
                "shielded ICS-20 withdrawal spends must use the same sender address",
            );
        }

        if let Some(change_output) = &self.change_output {
            ensure!(
                change_output.value.asset_id == first_spend.note.asset_id(),
                "shielded ICS-20 withdrawal change must use the same asset as spends",
            );
            ensure!(
                change_output.dest_address == first_spend.note.address(),
                "shielded ICS-20 withdrawal change must be sender-owned",
            );
        }

        ensure!(
            self.balance() == Balance::default(),
            "shielded ICS-20 withdrawal must be internally balanced",
        );
        self.compliance
            .witness
            .validate(first_spend.note.asset_id(), &first_spend.note.address())?;
        ensure!(self.compliance.timestamp > 0, "missing action timestamp");
        ensure!(
            self.volume_accumulator.day_start()
                == crate::select_accumulator_day(self.compliance.timestamp),
            "volume accumulator day does not match action timestamp"
        );
        Ok(())
    }

    fn withdrawal_effect_hash_limbs(&self) -> [Fq; 4] {
        let effect_hash = self.withdrawal.effect_hash();
        super::withdrawal_effect_hash_limbs(effect_hash.as_bytes())
    }

    fn withdrawal_compliance_encryption(&self) -> anyhow::Result<WithdrawalEncryptionResult> {
        let sender_leaf = &self.compliance.witness.sender.leaf;
        let (encryption_key, _) = withdrawal_encryption_key(
            self.compliance.witness.asset.is_regulated,
            self.compliance.witness.asset.is_regulated && !self.volume_accumulator.is_real(),
            &sender_leaf,
            &self.compliance.witness.asset.leaf,
        )?;
        let (seed, randomizer) = derive_withdrawal_encryption_material(self.compliance.nonce);
        encrypt_withdrawal_with_material(encryption_key, &self.sender_address(), seed, randomizer)
    }

    pub fn shielded_ics20_withdrawal_public_private(
        &self,
        fvk: &FullViewingKey,
        state_commitment_proofs: &[tct::Proof],
        anchor: tct::Root,
        recent_position_floor: u64,
    ) -> Result<
        (
            ShieldedIcs20WithdrawalProofPublic,
            ShieldedIcs20WithdrawalProofPrivate,
        ),
        crate::ProofError,
    > {
        self.validate()
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        let needs_accumulator_proof = matches!(
            self.volume_accumulator,
            VolumeAccumulatorPlan::Continuation { .. }
        );
        let expected_proofs = self.spends.len() + usize::from(needs_accumulator_proof);
        if state_commitment_proofs.len() != expected_proofs {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "shielded ICS-20 withdrawal expected {} state commitment proofs, got {}",
                expected_proofs,
                state_commitment_proofs.len()
            )));
        }
        let nullifier_key = self
            .compliance
            .witness
            .nullifier_key(fvk)
            .map_err(|error| crate::ProofError::InvalidPrivateInput(error.to_string()))?;

        let mut input_publics = self
            .spends
            .iter()
            .map(|spend| {
                Ok(ShieldedIcs20WithdrawalInputPublic {
                    nullifier: spend.nullifier(&nullifier_key),
                    rk: spend.rk(fvk),
                    history_required: shieldd_sdk_sct::nullifier_generation::is_old(
                        u64::from(spend.position),
                        recent_position_floor,
                    )
                    .map_err(|error| crate::ProofError::InvalidPublicInput(error.to_string()))?,
                })
            })
            .collect::<Result<Vec<_>, crate::ProofError>>()?;
        let padder = self.padder();
        pad_to_len(&mut input_publics, PADDED_ICS20_WITHDRAWAL_INPUTS, |slot| {
            ShieldedIcs20WithdrawalInputPublic {
                nullifier: padder.synthetic_dummy_nullifier(slot),
                rk: padder.synthetic_dummy_verification_key(slot),
                history_required: false,
            }
        });

        let mut real_input_privates = self
            .spends
            .iter()
            .zip(state_commitment_proofs.iter().cloned())
            .map(
                |(spend, state_commitment_proof)| ShieldedIcs20WithdrawalRequiredInputPrivate {
                    state_commitment_proof,
                    spent_note: spend.note.clone(),
                    spend_auth_randomizer: spend.randomizer,
                },
            )
            .collect::<Vec<_>>()
            .into_iter();
        let required_input = real_input_privates
            .next()
            .expect("validated withdrawal plans always contain a required spend");
        let optional_input = real_input_privates.next().map_or_else(
            || {
                let slot = 1;
                let dummy_note = padder.synthetic_dummy_input_note(slot);
                ShieldedIcs20WithdrawalOptionalInputPrivate {
                    spend: ShieldedIcs20WithdrawalRequiredInputPrivate {
                        state_commitment_proof: dummy_state_commitment_proof(dummy_note.commit()),
                        spent_note: dummy_note,
                        spend_auth_randomizer: padder.synthetic_dummy_spend_auth_randomizer(slot),
                    },
                    is_dummy: true,
                    dummy_nullifier_seed: padder.synthetic_dummy_nullifier_seed(slot),
                }
            },
            |spend| ShieldedIcs20WithdrawalOptionalInputPrivate {
                spend,
                is_dummy: false,
                dummy_nullifier_seed: Fq::from(0u64),
            },
        );
        debug_assert!(real_input_privates.next().is_none());

        let change_note = self
            .change_output
            .as_ref()
            .map(|output| output.output_note(self.compliance.witness.sender.leaf.capk))
            .unwrap_or_else(|| self.padder().synthetic_dummy_output_note(1));
        let withdrawal_effect_hash_limbs = self.withdrawal_effect_hash_limbs();
        let routing_nonce = Fq::from_le_bytes_mod_order(&self.compliance.nonce.to_bytes());
        let routing_tag = discovery::single_tag(
            &self.sender_address(),
            self.compliance.witness.asset.is_regulated,
            &self.routing_parameters,
            routing_nonce,
        );
        let withdrawal_compliance = self
            .withdrawal_compliance_encryption()
            .map_err(|error| crate::ProofError::InvalidPrivateInput(error.to_string()))?;
        let volume_plan = self.volume_accumulator.clone();
        let volume_payload = self.volume_accumulator_payload(fvk);
        let volume_prior_proof = if needs_accumulator_proof {
            state_commitment_proofs[self.spends.len()].clone()
        } else {
            dummy_state_commitment_proof(volume_plan.prior_commitment())
        };

        Ok((
            ShieldedIcs20WithdrawalProofPublic {
                family_id: ShieldedIcs20WithdrawalFamilyId::Canonical,
                anchor,
                balance_commitment: Balance::default().commit(self.value_blinding),
                asset_anchor: self.compliance.witness.asset.root,
                compliance_anchor: self.compliance.witness.user_root,
                target_timestamp: Fq::from(self.compliance.timestamp),
                inputs: input_publics,
                change_output: ShieldedIcs20WithdrawalChangePublic {
                    note_commitment: change_note.commit(),
                    recovery_commitment: change_note.recovery_commitment(),
                },
                outbound_asset_id: self.withdrawal.denom.id().0,
                outbound_amount: Fq::from(self.withdrawal.amount),
                withdrawal_effect_hash_limbs,
                routing_tag,
                routing_parameter_set_id: self.routing_parameters.id(),
                withdrawal_compliance_ciphertext: withdrawal_compliance.ciphertext.clone(),
                recent_position_floor,
                volume_accumulator: crate::VolumeAccumulatorPublic {
                    nullifier: volume_payload.nullifier,
                    commitment: volume_payload.commitment,
                    day_start: volume_payload.day_start,
                },
            },
            ShieldedIcs20WithdrawalProofPrivate {
                family_id: ShieldedIcs20WithdrawalFamilyId::Canonical,
                action_balance_blinding: self.value_blinding,
                ak: *fvk.spend_verification_key(),
                nk: *fvk.nullifier_key(),
                asset_path: self.compliance.witness.asset.path.clone(),
                asset_position: self.compliance.witness.asset.position,
                asset_indexed_leaf: self.compliance.witness.asset.leaf.clone(),
                is_regulated: self.compliance.witness.asset.is_regulated,
                routing_parameters: self.routing_parameters.clone(),
                routing_nonce,
                sender_compliance_path: self.compliance.witness.sender.path.clone(),
                sender_compliance_position: self.compliance.witness.sender.position,
                sender_leaf: self.compliance.witness.sender.leaf.clone(),
                withdrawal_seed: withdrawal_compliance.seed,
                withdrawal_randomizer: withdrawal_compliance.r,
                required_input,
                optional_input,
                change_output: ShieldedIcs20WithdrawalChangePrivate {
                    created_note: change_note,
                },
                volume_accumulator_seed: Fq::from_le_bytes_mod_order(
                    &self.compliance.nonce.to_bytes(),
                ),
                volume_accumulator: crate::VolumeAccumulatorPrivate {
                    plan: volume_plan,
                    prior_proof: volume_prior_proof,
                },
            },
        ))
    }

    pub fn action_body(
        &self,
        fvk: &FullViewingKey,
        memo_key: &PayloadKey,
        anchor: tct::Root,
        recent_position_floor: u64,
    ) -> anyhow::Result<ShieldedIcs20WithdrawalBody> {
        self.validate()?;

        let nullifier_key = self.compliance.witness.nullifier_key(fvk)?;
        let mut inputs = self
            .spends
            .iter()
            .map(|spend| spend.action_input_body(fvk, &nullifier_key, recent_position_floor))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let padder = self.padder();
        pad_to_len(&mut inputs, PADDED_ICS20_WITHDRAWAL_INPUTS, |slot| {
            let dummy_note = padder.synthetic_dummy_input_note(slot);
            let nullifier = padder.synthetic_dummy_nullifier(slot);
            TransferInputBody {
                nullifier,
                rk: padder.synthetic_dummy_verification_key(slot),
                encrypted_backref: crate::Backref::new(dummy_note.commit())
                    .encrypt(&fvk.backref_key(), &nullifier),
                compliance_ciphertext: Vec::new(),
                history_required: false,
            }
        });

        let (change_note, recovery_capsule) = self
            .change_output
            .as_ref()
            .map(|output| output.output_note_and_capsule(self.compliance.witness.sender.leaf.capk))
            .unwrap_or_else(|| padder.synthetic_dummy_output_note_and_capsule(1));
        let esk = change_note.ephemeral_secret_key();
        let ovk_wrapped_key = change_note.encrypt_key(
            fvk.outgoing(),
            Balance::default().commit(self.value_blinding),
        );
        let wrapped_memo_key = WrappedMemoKey::encrypt(
            memo_key,
            esk,
            change_note.transmission_key(),
            &change_note.diversified_generator(),
        );
        let change_output = ShieldedIcs20WithdrawalChangeBody {
            note_payload: change_note.payload(recovery_capsule),
            wrapped_memo_key,
            ovk_wrapped_key,
        };

        let routing_nonce = Fq::from_le_bytes_mod_order(&self.compliance.nonce.to_bytes());
        let routing_tag = discovery::single_tag(
            &self.sender_address(),
            self.compliance.witness.asset.is_regulated,
            &self.routing_parameters,
            routing_nonce,
        );
        let withdrawal_compliance = self.withdrawal_compliance_encryption()?;

        Ok(ShieldedIcs20WithdrawalBody {
            family_id: ShieldedIcs20WithdrawalFamilyId::Canonical,
            anchor,
            balance_commitment: Balance::default().commit(self.value_blinding),
            inputs,
            withdrawal: self.withdrawal.clone(),
            change_output,
            target_timestamp: self.compliance.timestamp,
            compliance_anchor: self.compliance.witness.user_root,
            asset_anchor: self.compliance.witness.asset.root,
            routing_tag,
            routing_parameter_set_id: self.routing_parameters.id(),
            withdrawal_compliance_ciphertext: withdrawal_compliance.ciphertext,
            volume_accumulator: self.volume_accumulator_payload(fvk),
        })
    }

    #[cfg(all(feature = "prover", any(unix, windows)))]
    pub fn build_unauth_shielded_ics20_withdrawal(
        &self,
        fvk: &FullViewingKey,
        auth_sigs: Vec<Signature<SpendAuth>>,
        state_commitment_proofs: Vec<tct::Proof>,
        anchor: tct::Root,
        memo_key: &PayloadKey,
        recent_position_floor: u64,
    ) -> Result<ShieldedIcs20Withdrawal, crate::ProofError> {
        let body = self
            .action_body(fvk, memo_key, anchor, recent_position_floor)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if auth_sigs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "shielded ICS-20 withdrawal expected {} auth sigs, got {}",
                self.spends.len(),
                auth_sigs.len()
            )));
        }
        let (public, private) = self.shielded_ics20_withdrawal_public_private(
            fvk,
            &state_commitment_proofs,
            anchor,
            recent_position_floor,
        )?;
        let proof = ShieldedIcs20WithdrawalProof::prove(public, private)?;
        let mut auth_sigs = auth_sigs;
        while auth_sigs.len() < PADDED_ICS20_WITHDRAWAL_INPUTS {
            auth_sigs.push(dummy_spend_auth_sig());
        }

        Ok(ShieldedIcs20Withdrawal {
            body,
            auth_sigs,
            proof,
        })
    }

    pub fn shielded_ics20_withdrawal_witness_payload(
        &self,
        fvk: &FullViewingKey,
        state_commitment_proofs: Vec<tct::Proof>,
        anchor: tct::Root,
        recent_position_floor: u64,
    ) -> Result<Vec<u8>, crate::ProofError> {
        let (public, private) = self.shielded_ics20_withdrawal_public_private(
            fvk,
            &state_commitment_proofs,
            anchor,
            recent_position_floor,
        )?;
        crate::gnark::encode_shielded_ics20_withdrawal_witness(&public, &private)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))
    }

    pub fn build_unauth_shielded_ics20_withdrawal_with_proof(
        &self,
        fvk: &FullViewingKey,
        auth_sigs: Vec<Signature<SpendAuth>>,
        anchor: tct::Root,
        memo_key: &PayloadKey,
        proof: ShieldedIcs20WithdrawalProof,
        recent_position_floor: u64,
    ) -> Result<ShieldedIcs20Withdrawal, crate::ProofError> {
        let body = self
            .action_body(fvk, memo_key, anchor, recent_position_floor)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if auth_sigs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "shielded ICS-20 withdrawal expected {} auth sigs, got {}",
                self.spends.len(),
                auth_sigs.len()
            )));
        }
        let mut auth_sigs = auth_sigs;
        while auth_sigs.len() < PADDED_ICS20_WITHDRAWAL_INPUTS {
            auth_sigs.push(dummy_spend_auth_sig());
        }

        Ok(ShieldedIcs20Withdrawal {
            body,
            auth_sigs,
            proof,
        })
    }

    pub fn synthetic_dummy_auth_sig(
        &self,
        slot: usize,
        effect_hash: &[u8],
    ) -> Signature<SpendAuth> {
        self.padder().synthetic_dummy_auth_sig(slot, effect_hash)
    }
}

impl DomainType for ShieldedIcs20WithdrawalPlan {
    type Proto = pb::ShieldedIcs20WithdrawalPlan;
}

impl From<ShieldedIcs20WithdrawalPlan> for pb::ShieldedIcs20WithdrawalPlan {
    fn from(value: ShieldedIcs20WithdrawalPlan) -> Self {
        Self {
            value_blinding: value.value_blinding.to_bytes().to_vec(),
            spends: value.spends.into_iter().map(Into::into).collect(),
            change_output: value.change_output.map(Into::into),
            withdrawal: Some(value.withdrawal.into()),
            compliance: Some(value.compliance.into()),
            routing_parameters: Some(value.routing_parameters.into()),
            volume_accumulator: Some(value.volume_accumulator.into()),
        }
    }
}

impl TryFrom<pb::ShieldedIcs20WithdrawalPlan> for ShieldedIcs20WithdrawalPlan {
    type Error = Error;

    fn try_from(value: pb::ShieldedIcs20WithdrawalPlan) -> Result<Self, Self::Error> {
        let value_blinding_bytes: [u8; 32] = value
            .value_blinding
            .try_into()
            .map_err(|_| anyhow!("malformed shielded ICS-20 withdrawal value blinding"))?;

        let plan = Self {
            value_blinding: Fr::from_bytes_checked(&value_blinding_bytes).map_err(|_| {
                anyhow!("malformed canonical shielded ICS-20 withdrawal value blinding")
            })?,
            spends: value
                .spends
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            change_output: value.change_output.map(TryInto::try_into).transpose()?,
            withdrawal: value
                .withdrawal
                .ok_or_else(|| anyhow!("missing embedded shielded ICS-20 withdrawal payload"))?
                .try_into()?,
            compliance: value
                .compliance
                .ok_or_else(|| anyhow!("missing action compliance context"))?
                .try_into()?,
            routing_parameters: value
                .routing_parameters
                .ok_or_else(|| anyhow!("missing routing parameters"))?
                .try_into()?,
            volume_accumulator: value
                .volume_accumulator
                .ok_or_else(|| anyhow!("missing volume accumulator plan"))?
                .try_into()?,
        };
        plan.validate()?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use std::{ops::Deref, str::FromStr};

    use decaf377::Fr;
    use ibc_types::core::{channel::ChannelId, client::Height as IbcHeight};
    use rand_core::OsRng;
    use shieldd_sdk_asset::{Value, BASE_ASSET_DENOM, TEST_USD_DENOM};
    use shieldd_sdk_keys::test_keys;
    use shieldd_sdk_txhash::EffectingData;

    use super::*;
    use crate::Note;

    fn test_withdrawal(amount: u64) -> Ics20Withdrawal {
        Ics20Withdrawal {
            amount: amount.into(),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_string(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        }
    }

    #[test]
    fn zero_value_withdrawal_is_rejected() {
        let error = test_withdrawal(0)
            .validate()
            .expect_err("zero-value withdrawals must not create outbound packets");
        assert!(error.to_string().contains("amount must be non-zero"));
    }

    fn two_spends(second_address: &Address) -> (ShieldedInputPlan, ShieldedInputPlan) {
        let value = Value {
            amount: 20u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let first_note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let second_note = Note::generate(&mut OsRng, second_address, value);
        let first = ShieldedInputPlan::new(&mut OsRng, first_note, 0u64.into());
        let second = ShieldedInputPlan::new(&mut OsRng, second_note, 1u64.into());
        (first, second)
    }

    fn one_spend_plan() -> ShieldedIcs20WithdrawalPlan {
        let value = Value {
            amount: 40u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        crate::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            None,
            test_withdrawal(40),
            Fr::from(7u64),
        )
        .expect("one-spend withdrawal plan should be valid")
    }

    fn two_spend_plan() -> ShieldedIcs20WithdrawalPlan {
        let (first, second) = two_spends(&test_keys::ADDRESS_0);
        crate::test_plan_helpers::ics20_withdrawal(
            vec![first, second],
            None,
            test_withdrawal(40),
            Fr::from(7u64),
        )
        .expect("aligned two-spend withdrawal plan should be valid")
    }

    fn assert_validation_and_decode_reject(plan: ShieldedIcs20WithdrawalPlan, expected: &str) {
        let error = plan
            .validate()
            .expect_err("mutated domain plan must fail validation");
        assert!(
            error.to_string().contains(expected),
            "unexpected validation error: {error}"
        );

        let error =
            ShieldedIcs20WithdrawalPlan::try_from(pb::ShieldedIcs20WithdrawalPlan::from(plan))
                .expect_err("serialized mutated plan must fail decoding");
        assert!(
            format!("{error:#}").contains(expected),
            "unexpected decoding error: {error:#}"
        );
    }

    #[test]
    fn new_plan_builds_padded_body_with_change() {
        let spend_value = Value {
            amount: 50_000u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let change_value = Value {
            amount: 10_000u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, spend_value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let change = ShieldedOutputPlan::new(
            &mut OsRng,
            change_value,
            test_keys::ADDRESS_0.deref().clone(),
        );
        let withdrawal = Ics20Withdrawal {
            amount: 40_000u64.into(),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_string(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        };

        let plan = crate::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            Some(change),
            withdrawal.clone(),
            Fr::from(7u64),
        )
        .expect("plan should be valid");

        let body = plan
            .action_body(
                &test_keys::FULL_VIEWING_KEY,
                &[7u8; 32].into(),
                shieldd_sdk_tct::Tree::default().root(),
                0,
            )
            .expect("body should build");
        assert_eq!(body.inputs.len(), 2);
        assert_eq!(body.withdrawal.effect_hash(), withdrawal.effect_hash());
        assert!(body
            .inputs
            .iter()
            .all(|input| input.encrypted_backref.len() == crate::backref::ENCRYPTED_BACKREF_LEN));
        Note::decrypt_key(
            body.change_output.ovk_wrapped_key.clone(),
            body.change_output.note_payload.note_commitment,
            body.balance_commitment,
            test_keys::FULL_VIEWING_KEY.outgoing(),
            &body.change_output.note_payload.ephemeral_key,
        )
        .expect("withdrawal change key must unwrap with the serialized action commitment");
        assert!(
            !body.inputs[1].encrypted_backref.is_empty(),
            "synthetic input backrefs must be indistinguishable in length from real inputs"
        );
        let expected_dummy_note = plan.padder().synthetic_dummy_input_note(1);
        let decrypted = body.inputs[1]
            .encrypted_backref
            .decrypt(
                &test_keys::FULL_VIEWING_KEY.backref_key(),
                &body.inputs[1].nullifier,
            )
            .expect("decrypt synthetic input backref");
        assert_eq!(
            decrypted,
            Some(crate::Backref::new(expected_dummy_note.commit()))
        );
    }

    #[test]
    fn new_plan_rejects_multi_spend_sender_mismatch() {
        let (first, second) = two_spends(&test_keys::ADDRESS_1);
        let err = crate::test_plan_helpers::ics20_withdrawal(
            vec![first, second],
            None,
            test_withdrawal(40),
            Fr::from(7u64),
        )
        .expect_err("sender mismatch must fail before proving");
        assert!(err
            .to_string()
            .contains("spends must use the same sender address"));
    }

    #[test]
    fn new_plan_rejects_non_sender_owned_change() {
        let spend_value = Value {
            amount: 50_000u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let change_value = Value {
            amount: 10_000u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, spend_value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let bad_change = ShieldedOutputPlan::new(
            &mut OsRng,
            change_value,
            test_keys::ADDRESS_1.deref().clone(),
        );
        let withdrawal = Ics20Withdrawal {
            amount: 40_000u64.into(),
            denom: BASE_ASSET_DENOM.clone(),
            destination_chain_address: "cosmos1destination".to_string(),
            return_address: test_keys::ADDRESS_0.deref().clone(),
            timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
            timeout_time: 60_000_000_000,
            source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
            ics20_memo: String::new(),
            use_transparent_address: false,
        };

        let err = crate::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            Some(bad_change),
            withdrawal,
            Fr::from(7u64),
        )
        .expect_err("non-sender-owned change must be rejected");

        assert!(err
            .to_string()
            .contains("shielded ICS-20 withdrawal change must be sender-owned"));
    }

    #[test]
    fn validation_rejects_every_remaining_cross_record_invariant_mutation() {
        let mut empty_spends = one_spend_plan();
        empty_spends.spends.clear();
        assert_validation_and_decode_reject(empty_spends, "at least one spend");

        let mut too_many_spends = two_spend_plan();
        too_many_spends
            .spends
            .push(too_many_spends.spends[0].clone());
        assert_validation_and_decode_reject(too_many_spends, "at most two spends");

        let mut payload_asset = two_spend_plan();
        payload_asset.withdrawal.denom = TEST_USD_DENOM.clone();
        assert_validation_and_decode_reject(payload_asset, "payload asset must match spends");

        let mut spend_asset = two_spend_plan();
        let mut value = spend_asset.spends[1].note.value();
        value.asset_id = asset::Id(Fq::from(0xA55E7u64));
        spend_asset.spends[1].note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        assert_validation_and_decode_reject(spend_asset, "spends must use the same asset");

        let mut timestamp = two_spend_plan();
        timestamp.compliance.timestamp = 0;
        assert_validation_and_decode_reject(timestamp, "timestamp");

        let mut asset_path = two_spend_plan();
        asset_path.compliance.witness.asset.path.layers[0].siblings[0] =
            Fq::from(0xA55E8u64).to_bytes().to_vec();
        assert_validation_and_decode_reject(asset_path, "asset witness");

        let mut change_asset = one_spend_plan();
        let change = ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: 1u64.into(),
                asset_id: asset::Id(Fq::from(0xA55E7u64)),
            },
            test_keys::ADDRESS_0.deref().clone(),
        );
        change_asset.change_output = Some(change);
        assert_validation_and_decode_reject(change_asset, "change must use the same asset");

        let mut unbalanced = one_spend_plan();
        unbalanced.withdrawal.amount = 39u64.into();
        assert_validation_and_decode_reject(unbalanced, "must be internally balanced");
    }

    #[test]
    fn materializers_reject_proof_and_auth_count_mismatches() {
        let plan = one_spend_plan();
        let anchor = tct::Tree::default().root();

        let error = plan
            .shielded_ics20_withdrawal_public_private(&test_keys::FULL_VIEWING_KEY, &[], anchor, 0)
            .expect_err("proof materialization must require one proof per real spend");
        assert!(error
            .to_string()
            .contains("shielded ICS-20 withdrawal expected 1 state commitment proofs, got 0"));

        let error = plan
            .build_unauth_shielded_ics20_withdrawal_with_proof(
                &test_keys::FULL_VIEWING_KEY,
                Vec::new(),
                anchor,
                &PayloadKey::random_key(&mut OsRng),
                ShieldedIcs20WithdrawalProof::default(),
                0,
            )
            .expect_err("action materialization must require one signature per real spend");
        assert!(error
            .to_string()
            .contains("shielded ICS-20 withdrawal expected 1 auth sigs, got 0"));
    }

    #[test]
    fn plan_construction_and_decode_reject_invalid_withdrawal_payload() {
        let value = Value {
            amount: 40u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let mut invalid_withdrawal = test_withdrawal(40);
        invalid_withdrawal.timeout_time = 0;
        let err = crate::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            None,
            invalid_withdrawal,
            Fr::from(7u64),
        )
        .expect_err("plan construction must reject an invalid withdrawal payload");
        assert!(format!("{err:#}").contains("timeout time must be non-zero"));

        let mut proto: pb::ShieldedIcs20WithdrawalPlan = one_spend_plan().into();
        proto
            .withdrawal
            .as_mut()
            .expect("plan withdrawal")
            .timeout_time = 0;
        let err = ShieldedIcs20WithdrawalPlan::try_from(proto)
            .expect_err("plan decoding must reject an invalid withdrawal payload");
        assert!(format!("{err:#}").contains("timeout time must be non-zero"));
    }

    #[test]
    fn body_proto_rejects_wrong_fixed_input_count() {
        let plan = one_spend_plan();
        let mut proto: pb::ShieldedIcs20WithdrawalBody = plan
            .action_body(
                &test_keys::FULL_VIEWING_KEY,
                &[7u8; 32].into(),
                shieldd_sdk_tct::Tree::default().root(),
                0,
            )
            .expect("derive action body")
            .into();
        proto.inputs.pop();

        let err = ShieldedIcs20WithdrawalBody::try_from(proto)
            .expect_err("body decoding must enforce the fixed family shape");
        assert!(err.to_string().contains("expects 2 inputs, got 1"));
    }

    #[test]
    fn action_body_uses_action_context() {
        let mut plan = one_spend_plan();
        let new_asset_anchor = plan.compliance.witness.asset.root;
        let new_compliance_anchor = tct::StateCommitment(Fq::from(0xC0FF1u64));
        let new_timestamp = plan.compliance.timestamp + 42;
        plan.compliance.witness.user_root = new_compliance_anchor;
        plan.compliance.timestamp = new_timestamp;
        plan.withdrawal.ics20_memo = "withdrawal memo".to_owned();

        plan.validate().expect("action context stay valid");
        let body = plan
            .action_body(
                &test_keys::FULL_VIEWING_KEY,
                &[7u8; 32].into(),
                shieldd_sdk_tct::Tree::default().root(),
                0,
            )
            .expect("derive body from complete plan");
        assert_eq!(body.asset_anchor, new_asset_anchor);
        assert_eq!(body.compliance_anchor, new_compliance_anchor);
        assert_eq!(body.target_timestamp, new_timestamp);
        assert_eq!(body.withdrawal.effect_hash(), plan.withdrawal.effect_hash());
        assert_eq!(
            body.balance_commitment,
            Balance::default().commit(plan.value_blinding)
        );
    }
}
