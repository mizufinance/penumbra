//! Declarative transaction plans, used for transaction authorization and creation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use shieldd_sdk_keys::{Address, FullViewingKey, PayloadKey};
use shieldd_sdk_proto::{core::transaction::v1 as pb, DomainType};
use shieldd_sdk_sct::nullifier_generation::NullifierWindow;
use shieldd_sdk_shielded_pool::{HostWithdrawal, ShieldedHostWithdrawalPlan, TransferPlan};
use shieldd_sdk_txhash::{EffectHash, EffectingData};

mod action;
mod auth;
mod build;
mod memo;

pub use action::ActionPlan;
pub use memo::MemoPlan;

use crate::FeeFundingPlan;
use crate::TransactionParameters;

/// A declaration of a planned [`Transaction`](crate::Transaction),
/// for use in transaction authorization and creation.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(try_from = "pb::TransactionPlan", into = "pb::TransactionPlan")]
pub struct TransactionPlan {
    pub actions: Vec<ActionPlan>,
    pub transaction_parameters: TransactionParameters,
    pub fee_funding: Option<FeeFundingPlan>,
    pub memo: Option<MemoPlan>,
    pub nullifier_window: Option<NullifierWindow>,
}

pub struct PlannedSpend<'a> {
    pub spend: &'a shieldd_sdk_shielded_pool::ShieldedInputPlan,
    pub witness: &'a shieldd_sdk_shielded_pool::ActionWitness,
}

impl TransactionPlan {
    pub fn spends(&self) -> impl Iterator<Item = PlannedSpend<'_>> {
        self.actions
            .iter()
            .filter_map(|action| match action {
                ActionPlan::Transfer(plan) => Some((&plan.spends, &plan.compliance.witness)),
                ActionPlan::NoteReshape(plan) => Some((&plan.spends, &plan.compliance.witness)),

                ActionPlan::ShieldedHostWithdrawal(plan) => {
                    Some((&plan.spends, &plan.compliance.witness))
                }
                _ => None,
            })
            .chain(
                self.fee_funding
                    .iter()
                    .map(|fee| (&fee.transfer.spends, &fee.transfer.compliance.witness)),
            )
            .flat_map(|(spends, witness)| {
                spends
                    .iter()
                    .map(move |spend| PlannedSpend { spend, witness })
            })
    }

    pub fn sort_actions(&mut self) {
        self.actions.sort_by_key(ActionPlan::variant_index);
    }

    pub fn effect_hash(&self, fvk: &FullViewingKey) -> Result<EffectHash> {
        let recent_position_floor = self.recent_position_floor()?;
        let mut state = blake2b_simd::Params::new()
            .personal(b"ShielddEfHs")
            .to_state();

        let parameters_hash = self.transaction_parameters.effect_hash();
        let memo_hash = match self.memo {
            Some(ref memo) => memo.memo()?.effect_hash(),
            None => EffectHash::default(),
        };
        let memo_key = self.memo_key().unwrap_or([0u8; 32].into());
        let fee_funding_hash = self
            .fee_funding
            .as_ref()
            .map(|plan| plan.effect_hash(fvk, &memo_key, recent_position_floor))
            .transpose()?
            .unwrap_or_default();
        state.update(parameters_hash.as_bytes());
        state.update(memo_hash.as_bytes());
        state.update(fee_funding_hash.as_bytes());
        crate::transaction::update_nullifier_window_effect_hash(
            &mut state,
            self.nullifier_window.as_ref(),
        );

        let num_actions = self.actions.len() as u32;
        state.update(&num_actions.to_le_bytes());
        for action_plan in &self.actions {
            state.update(
                action_plan
                    .effect_hash(fvk, &memo_key, recent_position_floor)?
                    .as_bytes(),
            );
        }

        Ok(EffectHash(state.finalize().as_array().clone()))
    }

    pub fn transfer_plans(&self) -> impl Iterator<Item = &TransferPlan> {
        self.actions.iter().filter_map(|action| {
            if let ActionPlan::Transfer(plan) = action {
                Some(plan)
            } else {
                None
            }
        })
    }

    pub fn shielded_host_withdrawal_plans(
        &self,
    ) -> impl Iterator<Item = &ShieldedHostWithdrawalPlan> {
        self.actions.iter().filter_map(|action| {
            if let ActionPlan::ShieldedHostWithdrawal(plan) = action {
                Some(plan)
            } else {
                None
            }
        })
    }

    pub fn host_withdrawals(&self) -> impl Iterator<Item = &HostWithdrawal> {
        self.actions.iter().filter_map(|action| {
            if let ActionPlan::ShieldedHostWithdrawal(plan) = action {
                Some(&plan.withdrawal)
            } else {
                None
            }
        })
    }

    pub fn dest_addresses(&self) -> Vec<Address> {
        let mut addresses =
            self.actions
                .iter()
                .flat_map(|action| match action {
                    ActionPlan::Transfer(plan) => plan.dest_addresses().collect::<Vec<_>>(),
                    ActionPlan::NoteReshape(plan) => plan
                        .outputs
                        .iter()
                        .map(|output| output.dest_address.clone())
                        .collect::<Vec<_>>(),

                    ActionPlan::ShieldedHostWithdrawal(plan) => vec![plan.created_output_address()],
                    ActionPlan::ComplianceRegisterAsset(_)
                    | ActionPlan::ComplianceRegisterUser(_) => Vec::new(),
                })
                .collect::<Vec<_>>();

        if let Some(fee_funding) = &self.fee_funding {
            addresses.extend(
                fee_funding
                    .transfer
                    .outputs
                    .iter()
                    .map(|output| output.dest_address.clone()),
            );
        }

        addresses
    }

    pub fn num_outputs(&self) -> usize {
        let action_outputs = self
            .actions
            .iter()
            .map(|action| match action {
                ActionPlan::Transfer(plan) => plan.num_outputs(),
                ActionPlan::NoteReshape(plan) => plan.family_id().output_count(),

                ActionPlan::ShieldedHostWithdrawal(plan) => plan.note_creating_output_count(),
                ActionPlan::ComplianceRegisterAsset(_) | ActionPlan::ComplianceRegisterUser(_) => 0,
            })
            .sum::<usize>();

        let fee_funding_outputs = self
            .fee_funding
            .as_ref()
            .map(|fee_funding| fee_funding.transfer.num_outputs())
            .unwrap_or_default();

        action_outputs + fee_funding_outputs
    }

    pub fn num_spends(&self) -> usize {
        let action_spends = self
            .actions
            .iter()
            .map(|action| action.spends().len())
            .sum::<usize>();

        let fee_funding_spends = self
            .fee_funding
            .as_ref()
            .map(|fee_funding| fee_funding.transfer.spends.len())
            .unwrap_or_default();

        action_spends + fee_funding_spends
    }

    pub fn recent_position_floor(&self) -> Result<u64> {
        match (self.num_spends(), self.nullifier_window) {
            (0, None) => Ok(0),
            (0, Some(_)) => anyhow::bail!("spend-free transaction plan has a nullifier window"),
            (_, None) => {
                anyhow::bail!("spend-bearing transaction plan is missing its nullifier window")
            }
            (_, Some(window)) => {
                window.validate()?;
                Ok(window.recent_position_floor)
            }
        }
    }

    pub fn num_proofs(&self) -> usize {
        let action_proofs = self
            .actions
            .iter()
            .map(|action| match action {
                ActionPlan::Transfer(_)
                | ActionPlan::NoteReshape(_)
                | ActionPlan::ShieldedHostWithdrawal(_) => 1,
                ActionPlan::ComplianceRegisterAsset(_) | ActionPlan::ComplianceRegisterUser(_) => 0,
            })
            .sum::<usize>();

        action_proofs + usize::from(self.fee_funding.is_some())
    }

    pub fn memo_key(&self) -> Option<PayloadKey> {
        self.memo.as_ref().map(|memo_plan| memo_plan.key)
    }
}

impl DomainType for TransactionPlan {
    type Proto = pb::TransactionPlan;
}

impl From<TransactionPlan> for pb::TransactionPlan {
    fn from(msg: TransactionPlan) -> Self {
        Self {
            actions: msg.actions.into_iter().map(Into::into).collect(),
            transaction_parameters: Some(msg.transaction_parameters.into()),
            fee_funding: msg.fee_funding.map(Into::into),
            memo: msg.memo.map(Into::into),
            nullifier_window: msg.nullifier_window.map(Into::into),
        }
    }
}

impl TryFrom<pb::TransactionPlan> for TransactionPlan {
    type Error = anyhow::Error;

    fn try_from(value: pb::TransactionPlan) -> Result<Self, Self::Error> {
        Ok(Self {
            actions: value
                .actions
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            transaction_parameters: value
                .transaction_parameters
                .ok_or_else(|| anyhow::anyhow!("missing transaction parameters"))?
                .try_into()?,
            fee_funding: value.fee_funding.map(TryInto::try_into).transpose()?,
            memo: value.memo.map(TryInto::try_into).transpose()?,
            nullifier_window: value.nullifier_window.map(TryInto::try_into).transpose()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Transaction, TransactionBody};
    use decaf377::Fr;
    use rand_core::OsRng;
    use shieldd_sdk_asset::{Value, BASE_ASSET_ID};
    use shieldd_sdk_keys::keys::{AddressIndex, Bip44Path, SeedPhrase, SpendKey};
    use shieldd_sdk_keys::test_keys;
    use shieldd_sdk_shielded_pool::{
        discovery::{Parameters, Precision},
        HostTransfer, HostWithdrawalDestination, Note, NoteReshape, NoteReshapeFamilyId,
        NoteReshapeProof, RecoveryCommitment, Rseed, ShieldedInputPlan, ShieldedOutputPlan,
    };
    use shieldd_sdk_txhash::EffectHash;
    use std::ops::Deref;

    fn note_reshape_eight_by_one_fixture() -> (TransactionPlan, Transaction) {
        let mut rng = OsRng;
        let input_value = Value {
            amount: 1u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let spends = (0..5)
            .map(|_| {
                let note = Note::generate(&mut rng, &test_keys::ADDRESS_0, input_value);
                ShieldedInputPlan::new(&mut rng, note, 0u64.into())
            })
            .collect::<Vec<_>>();
        let output = ShieldedOutputPlan::new(
            &mut rng,
            Value {
                amount: 5u64.into(),
                asset_id: *BASE_ASSET_ID,
            },
            test_keys::ADDRESS_0.deref().clone(),
        );
        let note_reshape = shieldd_sdk_shielded_pool::test_plan_helpers::note_reshape(
            NoteReshapeFamilyId::EightByOne,
            spends,
            vec![output],
            Fr::from(17u64),
        )
        .expect("valid padded NoteReshape plan");
        let placeholder = NoteReshape {
            body: note_reshape
                .note_reshape_body(
                    &test_keys::FULL_VIEWING_KEY,
                    &PayloadKey::from([0u8; 32]),
                    shieldd_sdk_tct::Tree::default().root(),
                    0,
                )
                .expect("note reshape body materialization succeeds"),
            auth_sigs: vec![[0u8; 64].into(); NoteReshapeFamilyId::EightByOne.auth_sig_count()],
            proof: NoteReshapeProof::default(),
        };
        (
            TransactionPlan {
                actions: vec![ActionPlan::NoteReshape(note_reshape)],
                ..Default::default()
            },
            Transaction {
                transaction_body: TransactionBody {
                    actions: vec![crate::Action::NoteReshape(placeholder)],
                    ..Default::default()
                },
                ..Default::default()
            },
        )
    }

    fn fee_funding_plan_fixture() -> FeeFundingPlan {
        let mut rng = OsRng;
        let sender_sk =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(&mut rng), &Bip44Path::new(0))
                .expect("test spend key should satisfy key refinements");
        let sender = sender_sk
            .full_viewing_key()
            .incoming()
            .payment_address(AddressIndex::from(0u32));
        let recipient = sender.clone();
        let value = Value {
            amount: 100u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let note = Note::from_parts(
            sender,
            value,
            Rseed::generate(&mut rng),
            RecoveryCommitment::unavailable(),
        )
        .expect("valid note");
        let spend = ShieldedInputPlan::new(&mut rng, note, 0u64.into());
        let output = ShieldedOutputPlan::new(&mut rng, value, recipient);

        FeeFundingPlan {
            transfer: shieldd_sdk_shielded_pool::test_plan_helpers::fee_funding(
                vec![spend],
                vec![output],
                Fr::from(19u64),
            )
            .expect("fee-funding transfer plan"),
        }
    }

    #[test]
    fn routing_parameters_propagate_to_transfer_family() {
        let parameters =
            Parameters::new(Precision::new(12).unwrap(), Precision::new(20).unwrap(), 42).unwrap();

        let mut rng = OsRng;
        let sender_sk =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(&mut rng), &Bip44Path::new(0))
                .expect("test spend key should satisfy key refinements");
        let recipient_sk =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(&mut rng), &Bip44Path::new(0))
                .expect("test spend key should satisfy key refinements");
        let sender = sender_sk
            .full_viewing_key()
            .incoming()
            .payment_address(AddressIndex::from(0u32));
        let recipient = recipient_sk
            .full_viewing_key()
            .incoming()
            .payment_address(AddressIndex::from(0u32));
        let value = Value {
            amount: 100u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let note = Note::from_parts(
            sender,
            value,
            Rseed::generate(&mut rng),
            RecoveryCommitment::unavailable(),
        )
        .expect("valid note");
        let spend = ShieldedInputPlan::new(&mut rng, note, 0u64.into());
        let output = ShieldedOutputPlan::new(&mut rng, value, recipient);

        let transfer = {
            let context = shieldd_sdk_shielded_pool::test_plan_helpers::transfer_context(
                &spend,
                &output.dest_address,
            );
            shieldd_sdk_shielded_pool::TransferPlan::new(
                vec![spend],
                vec![output],
                Fr::rand(&mut rng),
                context.clone(),
                shieldd_sdk_shielded_pool::VolumeAccumulatorPlan::padding(context.timestamp),
                shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
                parameters.clone(),
            )
        }
        .expect("plan");

        let plan = TransactionPlan {
            actions: vec![ActionPlan::Transfer(transfer)],
            transaction_parameters: Default::default(),
            fee_funding: None,
            memo: None,
            nullifier_window: None,
        };

        assert_eq!(
            plan.num_outputs(),
            shieldd_sdk_shielded_pool::PADDED_TRANSFER_OUTPUTS,
            "transfer must count every proof-bound output slot"
        );
        assert!(matches!(
            &plan.actions[0],
            ActionPlan::Transfer(transfer) if transfer.routing_parameters == parameters
        ));
    }

    #[test]
    fn shielded_withdrawal_counts_change_output_for_routing() {
        let parameters =
            Parameters::new(Precision::new(12).unwrap(), Precision::new(18).unwrap(), 42).unwrap();

        let spend_value = Value {
            amount: 50_000u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let change_value = Value {
            amount: 10_000u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, spend_value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let change = ShieldedOutputPlan::new(
            &mut OsRng,
            change_value,
            test_keys::ADDRESS_0.deref().clone(),
        );
        let withdrawal = shieldd_sdk_shielded_pool::HostWithdrawal {
            value: Value {
                amount: 40_000u64.into(),
                asset_id: *BASE_ASSET_ID,
            },
            destination: HostWithdrawalDestination::Transfer(HostTransfer {
                recipient: "bank1destination".to_owned(),
            }),
        };

        let withdrawal = {
            let context = shieldd_sdk_shielded_pool::test_plan_helpers::withdrawal_context(&spend);
            shieldd_sdk_shielded_pool::ShieldedHostWithdrawalPlan::new(
                vec![spend],
                Some(change),
                withdrawal,
                Fr::from(7u64),
                context.clone(),
                shieldd_sdk_shielded_pool::VolumeAccumulatorPlan::padding(context.timestamp),
                parameters.clone(),
            )
        }
        .expect("plan should be valid");

        let plan = TransactionPlan {
            actions: vec![ActionPlan::ShieldedHostWithdrawal(withdrawal)],
            transaction_parameters: Default::default(),
            fee_funding: None,
            memo: None,
            nullifier_window: None,
        };

        assert_eq!(plan.num_outputs(), 1);
        assert_eq!(
            plan.dest_addresses(),
            vec![test_keys::ADDRESS_0.deref().clone()]
        );
        assert!(matches!(
            &plan.actions[0],
            ActionPlan::ShieldedHostWithdrawal(withdrawal)
                if withdrawal.routing_parameters == parameters
        ));
    }

    #[test]
    fn shielded_withdrawal_without_explicit_change_still_counts_hidden_routing_note() {
        let parameters =
            Parameters::new(Precision::new(10).unwrap(), Precision::new(14).unwrap(), 42).unwrap();

        let spend_value = Value {
            amount: 40_000u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, spend_value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let withdrawal = shieldd_sdk_shielded_pool::HostWithdrawal {
            value: Value {
                amount: 40_000u64.into(),
                asset_id: *BASE_ASSET_ID,
            },
            destination: HostWithdrawalDestination::Transfer(HostTransfer {
                recipient: "bank1destination".to_owned(),
            }),
        };

        let withdrawal = {
            let context = shieldd_sdk_shielded_pool::test_plan_helpers::withdrawal_context(&spend);
            shieldd_sdk_shielded_pool::ShieldedHostWithdrawalPlan::new(
                vec![spend],
                None,
                withdrawal,
                Fr::from(7u64),
                context.clone(),
                shieldd_sdk_shielded_pool::VolumeAccumulatorPlan::padding(context.timestamp),
                parameters.clone(),
            )
        }
        .expect("plan should be valid");

        let plan = TransactionPlan {
            actions: vec![ActionPlan::ShieldedHostWithdrawal(withdrawal)],
            transaction_parameters: Default::default(),
            fee_funding: None,
            memo: None,
            nullifier_window: None,
        };

        assert_eq!(plan.num_outputs(), 1);
        assert_eq!(
            plan.dest_addresses(),
            vec![test_keys::ADDRESS_0.deref().clone()]
        );
        assert!(matches!(
            &plan.actions[0],
            ActionPlan::ShieldedHostWithdrawal(withdrawal)
                if withdrawal.routing_parameters == parameters
        ));
    }

    #[test]
    fn shielded_host_withdrawal_uses_current_routing_parameters() {
        let parameters =
            Parameters::new(Precision::new(11).unwrap(), Precision::new(19).unwrap(), 42).unwrap();

        let value = Value {
            amount: 40_000u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let note = Note::generate(&mut OsRng, &test_keys::ADDRESS_0, value);
        let spend = ShieldedInputPlan::new(&mut OsRng, note, 0u64.into());
        let withdrawal = HostWithdrawal {
            value,
            destination: HostWithdrawalDestination::Transfer(HostTransfer {
                recipient: "bank1destination".to_owned(),
            }),
        };
        let withdrawal = {
            let context = shieldd_sdk_shielded_pool::test_plan_helpers::withdrawal_context(&spend);
            shieldd_sdk_shielded_pool::ShieldedHostWithdrawalPlan::new(
                vec![spend],
                None,
                withdrawal,
                Fr::from(7u64),
                context.clone(),
                shieldd_sdk_shielded_pool::VolumeAccumulatorPlan::padding(context.timestamp),
                parameters.clone(),
            )
        }
        .expect("plan should be valid");

        let plan = TransactionPlan {
            actions: vec![ActionPlan::ShieldedHostWithdrawal(withdrawal)],
            transaction_parameters: Default::default(),
            fee_funding: None,
            memo: None,
            nullifier_window: None,
        };

        assert_eq!(plan.num_outputs(), 1);
        assert_eq!(
            plan.dest_addresses(),
            vec![test_keys::ADDRESS_0.deref().clone()]
        );
        assert!(matches!(
            &plan.actions[0],
            ActionPlan::ShieldedHostWithdrawal(withdrawal)
                if withdrawal.routing_parameters == parameters
        ));
    }

    #[test]
    fn apply_auth_data_rebinds_note_reshape_dummy_signatures_to_transaction_hash() {
        let mut rng = OsRng;
        let input_value = Value {
            amount: 1u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let spends = (0..5)
            .map(|_| {
                let note = Note::generate(&mut rng, &test_keys::ADDRESS_0, input_value);
                ShieldedInputPlan::new(&mut rng, note, 0u64.into())
            })
            .collect::<Vec<_>>();
        let output = ShieldedOutputPlan::new(
            &mut rng,
            Value {
                amount: 5u64.into(),
                asset_id: *BASE_ASSET_ID,
            },
            test_keys::ADDRESS_0.deref().clone(),
        );
        let note_reshape = shieldd_sdk_shielded_pool::test_plan_helpers::note_reshape(
            NoteReshapeFamilyId::EightByOne,
            spends,
            vec![output],
            Fr::from(17u64),
        )
        .expect("valid padded NoteReshape plan");
        let placeholder = NoteReshape {
            body: note_reshape
                .note_reshape_body(
                    &test_keys::FULL_VIEWING_KEY,
                    &PayloadKey::from([0u8; 32]),
                    shieldd_sdk_tct::Tree::default().root(),
                    0,
                )
                .expect("note reshape body materialization succeeds"),
            auth_sigs: vec![[0u8; 64].into(); NoteReshapeFamilyId::EightByOne.auth_sig_count()],
            proof: NoteReshapeProof::default(),
        };
        let plan = TransactionPlan {
            actions: vec![ActionPlan::NoteReshape(note_reshape.clone())],
            ..Default::default()
        };
        let transaction = Transaction {
            transaction_body: TransactionBody {
                actions: vec![crate::Action::NoteReshape(placeholder)],
                ..Default::default()
            },
            ..Default::default()
        };
        let effect_hash = transaction.effect_hash();
        let authorized = plan
            .apply_auth_data(
                &crate::AuthorizationData {
                    effect_hash: Some(effect_hash),
                    spend_auths: vec![[0u8; 64].into(); note_reshape.spends.len()],
                },
                transaction,
            )
            .expect("apply authorization data");
        let crate::Action::NoteReshape(action) = &authorized.transaction_body.actions[0] else {
            panic!("expected NoteReshape action");
        };

        for index in note_reshape.spends.len()..action.body.inputs.len() {
            action.body.inputs[index]
                .rk
                .verify(effect_hash.as_ref(), &action.auth_sigs[index])
                .unwrap_or_else(|error| {
                    panic!("dummy signature {index} was not rebound to transaction hash: {error}")
                });
        }
    }

    #[test]
    fn apply_auth_data_emits_canonical_no_binding_signature_for_zero_blinding() {
        let plan = TransactionPlan::default();
        let transaction = Transaction::default();
        let authorized = plan
            .apply_auth_data(
                &crate::AuthorizationData {
                    effect_hash: None,
                    spend_auths: Vec::new(),
                },
                transaction,
            )
            .expect("zero-blinding authorization data must apply");

        assert!(
            crate::is_no_binding_signature(authorized.binding_sig()),
            "zero aggregate blinding must use the canonical no-binding placeholder"
        );
    }

    #[test]
    fn apply_auth_data_rejects_zero_blinding_for_proof_bearing_transaction() {
        let mut rng = OsRng;
        let value = Value {
            amount: 1u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let spends = (0..2)
            .map(|_| {
                let note = Note::generate(&mut rng, &test_keys::ADDRESS_0, value);
                ShieldedInputPlan::new(&mut rng, note, 0u64.into())
            })
            .collect();
        let output = ShieldedOutputPlan::new(&mut rng, value, test_keys::ADDRESS_0.deref().clone());
        let note_reshape = shieldd_sdk_shielded_pool::test_plan_helpers::note_reshape(
            NoteReshapeFamilyId::EightByOne,
            spends,
            vec![output],
            Fr::from(0u64),
        )
        .expect("valid zero-blinding plan");
        let plan = TransactionPlan {
            actions: vec![ActionPlan::NoteReshape(note_reshape)],
            ..Default::default()
        };

        let error = plan
            .apply_auth_data(
                &crate::AuthorizationData {
                    effect_hash: None,
                    spend_auths: vec![[0u8; 64].into(); 2],
                },
                Transaction::default(),
            )
            .expect_err("proof-bearing plans must not emit identity binding mode");
        assert!(
            error
                .to_string()
                .contains("proof-bearing transaction plan has an identity aggregate binding key"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn apply_auth_data_rejects_action_count_mismatch() {
        let (plan, mut transaction) = note_reshape_eight_by_one_fixture();
        transaction.transaction_body.actions.clear();
        let effect_hash = transaction.effect_hash();

        let error = plan
            .apply_auth_data(
                &crate::AuthorizationData {
                    effect_hash: Some(effect_hash),
                    spend_auths: vec![[0u8; 64].into(); plan.num_spends()],
                },
                transaction,
            )
            .expect_err("a transaction may not omit a planned proof-bearing action");

        assert!(
            error.to_string().contains("action count"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn apply_auth_data_rejects_note_reshape_family_mismatch() {
        let (plan, mut transaction) = note_reshape_eight_by_one_fixture();
        let crate::Action::NoteReshape(action) = &mut transaction.transaction_body.actions[0]
        else {
            panic!("expected NoteReshape action");
        };
        action.body.family_id = NoteReshapeFamilyId::OneByEight;
        let effect_hash = transaction.effect_hash();

        let error = plan
            .apply_auth_data(
                &crate::AuthorizationData {
                    effect_hash: Some(effect_hash),
                    spend_auths: vec![[0u8; 64].into(); plan.num_spends()],
                },
                transaction,
            )
            .expect_err("authorization may not cross NoteReshape families");

        assert!(
            error.to_string().contains("family"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn apply_auth_data_rejects_fee_funding_presence_mismatch() {
        let plan = TransactionPlan {
            fee_funding: Some(fee_funding_plan_fixture()),
            ..Default::default()
        };
        let transaction = Transaction::default();
        let effect_hash = transaction.effect_hash();

        let error = plan
            .apply_auth_data(
                &crate::AuthorizationData {
                    effect_hash: Some(effect_hash),
                    spend_auths: vec![[0u8; 64].into(); plan.num_spends()],
                },
                transaction,
            )
            .expect_err("a transaction may not omit planned fee funding");

        assert!(
            error.to_string().contains("fee-funding presence"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn apply_auth_data_rejects_missing_effect_hash_for_proof_bearing_plan() {
        let (plan, transaction) = note_reshape_eight_by_one_fixture();

        let error = plan
            .apply_auth_data(
                &crate::AuthorizationData {
                    effect_hash: None,
                    spend_auths: vec![[0u8; 64].into(); plan.num_spends()],
                },
                transaction,
            )
            .expect_err("proof-bearing authorization must name the approved transaction");

        assert!(
            error.to_string().contains("approved effect hash"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn apply_auth_data_rejects_supplied_effect_hash_mismatch() {
        let transaction = Transaction::default();
        let actual_effect_hash = transaction.effect_hash();
        let supplied_effect_hash = EffectHash([0x5au8; 64]);
        assert_ne!(
            supplied_effect_hash, actual_effect_hash,
            "test vectors must exercise an actual mismatch"
        );

        let error = TransactionPlan::default()
            .apply_auth_data(
                &crate::AuthorizationData {
                    effect_hash: Some(supplied_effect_hash),
                    spend_auths: Vec::new(),
                },
                transaction,
            )
            .expect_err("authorization data may not name a different transaction");

        assert!(
            error.to_string().contains("effect hash"),
            "unexpected rejection reason: {error:#}"
        );
    }
}
