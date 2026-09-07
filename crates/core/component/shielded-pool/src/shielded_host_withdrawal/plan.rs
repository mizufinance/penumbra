use anyhow::{anyhow, ensure, Context, Error};
use decaf377::{Fq, Fr};
use decaf377_rdsa::{Signature, SpendAuth};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::{asset, Balance};
use shieldd_sdk_keys::{
    symmetric::{PayloadKey, WrappedMemoKey},
    Address, FullViewingKey,
};
use shieldd_sdk_proto::{core::component::shielded_pool::v1 as pb, DomainType};
use shieldd_sdk_tct as tct;
use shieldd_sdk_txhash::EffectingData;

use crate::{
    discovery::{self, Parameters},
    note_reshape_padding::{
        dummy_spend_auth_sig, dummy_state_commitment_proof, pad_to_len, HiddenArityPadder,
    },
    HostWithdrawal, ShieldedHostWithdrawal, ShieldedIcs20WithdrawalChangeBody,
    ShieldedIcs20WithdrawalChangePrivate, ShieldedIcs20WithdrawalChangePublic,
    ShieldedIcs20WithdrawalFamilyId, ShieldedIcs20WithdrawalInputPublic,
    ShieldedIcs20WithdrawalOptionalInputPrivate, ShieldedIcs20WithdrawalProof,
    ShieldedIcs20WithdrawalProofPrivate, ShieldedIcs20WithdrawalProofPublic,
    ShieldedIcs20WithdrawalRequiredInputPrivate, ShieldedInputPlan, ShieldedOutputPlan,
    TransferInputBody,
};

use super::ShieldedHostWithdrawalBody;

const PADDED_HOST_WITHDRAWAL_INPUTS: usize = 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    try_from = "pb::ShieldedHostWithdrawalPlan",
    into = "pb::ShieldedHostWithdrawalPlan"
)]
pub struct ShieldedHostWithdrawalPlan {
    pub value_blinding: Fr,
    pub spends: Vec<ShieldedInputPlan>,
    pub change_output: Option<ShieldedOutputPlan>,
    pub withdrawal: HostWithdrawal,
    pub routing_parameters: Parameters,
    pub compliance: crate::WithdrawalContext,
}

impl ShieldedHostWithdrawalPlan {
    pub fn new(
        spends: Vec<ShieldedInputPlan>,
        change_output: Option<ShieldedOutputPlan>,
        withdrawal: HostWithdrawal,
        value_blinding: Fr,
        compliance: crate::WithdrawalContext,
        routing_parameters: Parameters,
    ) -> anyhow::Result<Self> {
        let plan = Self {
            value_blinding,
            spends,
            change_output,
            withdrawal,
            routing_parameters,
            compliance,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn family_id(&self) -> ShieldedIcs20WithdrawalFamilyId {
        ShieldedIcs20WithdrawalFamilyId::Canonical
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
            .expect("shielded host withdrawal plan must contain at least one real spend")
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
        self.withdrawal.value.asset_id
    }

    fn padder(&self) -> HiddenArityPadder {
        HiddenArityPadder {
            value_blinding: self.value_blinding,
            first_spend_randomizer: self.first_spend().randomizer,
            sender_address: self.sender_address(),
            asset_id: self.withdrawal_asset_id(),
            // Host withdrawals reuse the ICS-20 circuit and its fixed dummy-nullifier domain.
            nullifier_domain_sep_label:
                b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.nullifier",
            nullifier_seed_label:
                b"shieldd.shielded_host_withdrawal.synthetic_dummy.nullifier_seed",
            spend_auth_key_label:
                b"shieldd.shielded_host_withdrawal.synthetic_dummy.spend_auth_key",
            spend_auth_randomizer_label:
                b"shieldd.shielded_host_withdrawal.synthetic_dummy.spend_auth_randomizer",
            input_note_label: b"shieldd.shielded_host_withdrawal.synthetic_dummy.input_note",
            output_note_label: b"shieldd.shielded_host_withdrawal.synthetic_dummy.output_note",
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        self.withdrawal
            .validate()
            .context("invalid shielded host withdrawal payload")?;
        ensure!(
            !self.spends.is_empty(),
            "shielded host withdrawal requires at least one spend"
        );
        ensure!(
            self.spends.len() <= PADDED_HOST_WITHDRAWAL_INPUTS,
            "shielded host withdrawal supports at most two spends",
        );

        let first_spend = self
            .spends
            .first()
            .ok_or_else(|| anyhow!("shielded host withdrawal requires at least one spend"))?;
        ensure!(
            self.withdrawal.value.asset_id == first_spend.note.asset_id(),
            "shielded host withdrawal payload asset must match spends",
        );

        for spend in &self.spends {
            ensure!(
                spend.note.asset_id() == first_spend.note.asset_id(),
                "shielded host withdrawal spends must use the same asset",
            );
            ensure!(
                spend.note.address() == first_spend.note.address(),
                "shielded host withdrawal spends must use the same sender address",
            );
        }

        if let Some(change_output) = &self.change_output {
            ensure!(
                change_output.value.asset_id == first_spend.note.asset_id(),
                "shielded host withdrawal change must use the same asset as spends",
            );
            ensure!(
                change_output.dest_address == first_spend.note.address(),
                "shielded host withdrawal change must be sender-owned",
            );
        }

        ensure!(
            self.balance() == Balance::default(),
            "shielded host withdrawal must be internally balanced",
        );
        self.compliance
            .witness
            .validate(first_spend.note.asset_id(), &first_spend.note.address())?;
        ensure!(self.compliance.timestamp > 0, "missing action timestamp");
        Ok(())
    }

    fn withdrawal_effect_hash_limbs(&self) -> [Fq; 4] {
        let effect_hash = self.withdrawal.effect_hash();
        crate::shielded_ics20_withdrawal::withdrawal_effect_hash_limbs(effect_hash.as_bytes())
    }

    pub fn shielded_host_withdrawal_public_private(
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
        if state_commitment_proofs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "shielded host withdrawal expected {} state commitment proofs, got {}",
                self.spends.len(),
                state_commitment_proofs.len()
            )));
        }

        let mut input_publics = self
            .spends
            .iter()
            .map(|spend| {
                Ok(ShieldedIcs20WithdrawalInputPublic {
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
        let padder = self.padder();
        pad_to_len(&mut input_publics, PADDED_HOST_WITHDRAWAL_INPUTS, |slot| {
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
            .map(ShieldedOutputPlan::output_note)
            .unwrap_or_else(|| self.padder().synthetic_dummy_output_note(1));
        let withdrawal_effect_hash_limbs = self.withdrawal_effect_hash_limbs();
        let routing_nonce = Fq::from_le_bytes_mod_order(&self.compliance.nonce.to_bytes());
        let routing_tag = discovery::single_tag(
            &self.sender_address(),
            self.compliance.witness.asset.is_regulated,
            &self.routing_parameters,
            routing_nonce,
        );

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
                },
                outbound_asset_id: self.withdrawal.value.asset_id.0,
                outbound_amount: Fq::from(self.withdrawal.value.amount),
                withdrawal_effect_hash_limbs,
                routing_tag,
                routing_parameter_set_id: self.routing_parameters.id(),
                recent_position_floor,
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
                required_input,
                optional_input,
                change_output: ShieldedIcs20WithdrawalChangePrivate {
                    created_note: change_note,
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
    ) -> anyhow::Result<ShieldedHostWithdrawalBody> {
        self.validate()?;

        let mut inputs = self
            .spends
            .iter()
            .map(|spend| spend.action_input_body(fvk, recent_position_floor))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let padder = self.padder();
        pad_to_len(&mut inputs, PADDED_HOST_WITHDRAWAL_INPUTS, |slot| {
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

        let change_note = self
            .change_output
            .as_ref()
            .map(ShieldedOutputPlan::output_note)
            .unwrap_or_else(|| padder.synthetic_dummy_output_note(1));
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
            note_payload: change_note.payload(),
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

        Ok(ShieldedHostWithdrawalBody {
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
        })
    }

    #[cfg(any(unix, windows))]
    pub fn build_unauth_shielded_host_withdrawal(
        &self,
        fvk: &FullViewingKey,
        auth_sigs: Vec<Signature<SpendAuth>>,
        state_commitment_proofs: Vec<tct::Proof>,
        anchor: tct::Root,
        memo_key: &PayloadKey,
        recent_position_floor: u64,
    ) -> Result<ShieldedHostWithdrawal, crate::ProofError> {
        let body = self
            .action_body(fvk, memo_key, anchor, recent_position_floor)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if auth_sigs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "shielded host withdrawal expected {} auth sigs, got {}",
                self.spends.len(),
                auth_sigs.len()
            )));
        }
        let (public, private) = self.shielded_host_withdrawal_public_private(
            fvk,
            &state_commitment_proofs,
            anchor,
            recent_position_floor,
        )?;
        let proof = ShieldedIcs20WithdrawalProof::prove(public, private)?;
        let mut auth_sigs = auth_sigs;
        while auth_sigs.len() < PADDED_HOST_WITHDRAWAL_INPUTS {
            auth_sigs.push(dummy_spend_auth_sig());
        }

        Ok(ShieldedHostWithdrawal {
            body,
            auth_sigs,
            proof,
        })
    }

    pub fn build_unauth_shielded_host_withdrawal_with_proof(
        &self,
        fvk: &FullViewingKey,
        auth_sigs: Vec<Signature<SpendAuth>>,
        anchor: tct::Root,
        memo_key: &PayloadKey,
        proof: ShieldedIcs20WithdrawalProof,
        recent_position_floor: u64,
    ) -> Result<ShieldedHostWithdrawal, crate::ProofError> {
        let body = self
            .action_body(fvk, memo_key, anchor, recent_position_floor)
            .map_err(|e| crate::ProofError::InvalidPublicInput(e.to_string()))?;
        if auth_sigs.len() != self.spends.len() {
            return Err(crate::ProofError::InvalidPublicInput(format!(
                "shielded host withdrawal expected {} auth sigs, got {}",
                self.spends.len(),
                auth_sigs.len()
            )));
        }
        let mut auth_sigs = auth_sigs;
        while auth_sigs.len() < PADDED_HOST_WITHDRAWAL_INPUTS {
            auth_sigs.push(dummy_spend_auth_sig());
        }

        Ok(ShieldedHostWithdrawal {
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

impl DomainType for ShieldedHostWithdrawalPlan {
    type Proto = pb::ShieldedHostWithdrawalPlan;
}

impl From<ShieldedHostWithdrawalPlan> for pb::ShieldedHostWithdrawalPlan {
    fn from(value: ShieldedHostWithdrawalPlan) -> Self {
        Self {
            value_blinding: value.value_blinding.to_bytes().to_vec(),
            spends: value.spends.into_iter().map(Into::into).collect(),
            change_output: value.change_output.map(Into::into),
            withdrawal: Some(value.withdrawal.into()),
            compliance: Some(value.compliance.into()),
            routing_parameters: Some(value.routing_parameters.into()),
        }
    }
}

impl TryFrom<pb::ShieldedHostWithdrawalPlan> for ShieldedHostWithdrawalPlan {
    type Error = Error;

    fn try_from(value: pb::ShieldedHostWithdrawalPlan) -> Result<Self, Self::Error> {
        let value_blinding_bytes: [u8; 32] = value
            .value_blinding
            .try_into()
            .map_err(|_| anyhow!("malformed shielded host withdrawal value blinding"))?;

        let plan = Self {
            value_blinding: Fr::from_bytes_checked(&value_blinding_bytes).map_err(|_| {
                anyhow!("malformed canonical shielded host withdrawal value blinding")
            })?,
            spends: value
                .spends
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, _>>()?,
            change_output: value.change_output.map(TryInto::try_into).transpose()?,
            withdrawal: value
                .withdrawal
                .ok_or_else(|| anyhow!("missing embedded shielded host withdrawal payload"))?
                .try_into()?,
            compliance: value
                .compliance
                .ok_or_else(|| anyhow!("missing action compliance context"))?
                .try_into()?,
            routing_parameters: value
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
    use std::ops::Deref;

    use rand_core::OsRng;
    use shieldd_sdk_asset::{Value, BASE_ASSET_DENOM};
    use shieldd_sdk_keys::test_keys;
    use shieldd_sdk_sct::Nullifier;

    use super::*;
    use crate::{
        HostTransfer, HostWithdrawalDestination, Note, ShieldedIcs20WithdrawalProof,
        ShieldedIcs20WithdrawalProofPrivate, ShieldedIcs20WithdrawalProofPublic,
    };

    fn padded_proof_inputs() -> (
        ShieldedIcs20WithdrawalProofPublic,
        ShieldedIcs20WithdrawalProofPrivate,
    ) {
        let value = Value {
            amount: 42u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let mut tree = tct::Tree::new();
        tree.insert(tct::Witness::Keep, note.commit())
            .expect("insert withdrawal note");
        let state_commitment_proof = tree
            .witness(note.commit())
            .expect("witness withdrawal note");
        let spend = ShieldedInputPlan::new(&mut OsRng, note, state_commitment_proof.position());
        let withdrawal = HostWithdrawal {
            value,
            destination: HostWithdrawalDestination::Transfer(HostTransfer {
                recipient: "bank1recipient".to_owned(),
            }),
        };
        let mut plan = crate::test_plan_helpers::host_withdrawal(
            vec![spend],
            None,
            withdrawal,
            Fr::from(7u64),
        )
        .expect("plan should be valid");
        plan.routing_parameters = Parameters::new(
            crate::discovery::Precision::new(11).unwrap(),
            crate::discovery::Precision::new(19).unwrap(),
            42,
        )
        .unwrap();

        plan.shielded_host_withdrawal_public_private(
            &test_keys::FULL_VIEWING_KEY,
            &[state_commitment_proof],
            tree.root(),
            0,
        )
        .expect("derive host withdrawal proof inputs")
    }

    #[test]
    fn new_plan_builds_padded_host_withdrawal_body() {
        let value = Value {
            amount: 42u64.into(),
            asset_id: BASE_ASSET_DENOM.id(),
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let withdrawal = HostWithdrawal {
            value,
            destination: HostWithdrawalDestination::Transfer(HostTransfer {
                recipient: "bank1recipient".to_owned(),
            }),
        };

        let plan = crate::test_plan_helpers::host_withdrawal(
            vec![spend],
            None,
            withdrawal.clone(),
            Fr::from(7u64),
        )
        .expect("plan should be valid");

        let body = plan
            .action_body(
                &test_keys::FULL_VIEWING_KEY,
                &[7u8; 32].into(),
                tct::Tree::default().root(),
                0,
            )
            .expect("body should build");
        assert_eq!(body.inputs.len(), 2);
        assert_eq!(body.withdrawal.effect_hash(), withdrawal.effect_hash());
        assert_eq!(
            plan.created_output_address(),
            test_keys::ADDRESS_0.deref().clone()
        );
    }

    #[test]
    fn padded_spend_uses_shared_withdrawal_circuit_nullifier_domain() {
        let (public, private) = padded_proof_inputs();
        let dummy = &private.optional_input;
        let domain = Fq::from_le_bytes_mod_order(
            blake2b_simd::blake2b(b"shieldd.shielded_ics20_withdrawal.synthetic_dummy.nullifier")
                .as_bytes(),
        );
        let expected = Nullifier(poseidon377::hash_3(
            &domain,
            (
                dummy.dummy_nullifier_seed,
                Fq::from_le_bytes_mod_order(&dummy.spend.spend_auth_randomizer.to_bytes()),
                Fq::from(1u64),
            ),
        ));

        assert_eq!(public.inputs[1].nullifier, expected);
    }

    #[test]
    fn host_withdrawal_binds_routing_parameters() {
        let (public, private) = padded_proof_inputs();

        assert_eq!(
            public.routing_parameter_set_id,
            private.routing_parameters.id()
        );
        assert_eq!(
            public.routing_tag,
            discovery::single_tag(
                &test_keys::ADDRESS_0,
                private.is_regulated,
                &private.routing_parameters,
                private.routing_nonce,
            )
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    #[ignore = "expensive: real release-mode Gnark proof generation"]
    fn gnark_proof_padded_host_withdrawal_proof_roundtrip() {
        crate::gnark::require_proof_test_runtime(crate::gnark::ProofTestFamily::Withdrawal)
            .expect("proof test prerequisites must be present");

        let (public, private) = padded_proof_inputs();
        let proof = ShieldedIcs20WithdrawalProof::prove(public.clone(), private)
            .expect("padded host withdrawal proof should generate");
        proof
            .verify(&public)
            .expect("padded host withdrawal proof should verify");
    }
}
