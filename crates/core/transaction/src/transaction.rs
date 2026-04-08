use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
};

use anyhow::{Context, Error};
use ark_ff::Zero;
use decaf377::Fr;
use decaf377_rdsa::{Binding, Signature, VerificationKey, VerificationKeyBytes};
use penumbra_sdk_asset::Balance;
use penumbra_sdk_community_pool::{CommunityPoolDeposit, CommunityPoolOutput, CommunityPoolSpend};
use penumbra_sdk_dex::{
    lp::action::{PositionClose, PositionOpen},
    swap::Swap,
};
use penumbra_sdk_governance::{DelegatorVote, ProposalSubmit, ProposalWithdraw, ValidatorVote};
use penumbra_sdk_ibc::IbcRelay;
use penumbra_sdk_keys::{AddressView, FullViewingKey, PayloadKey};
use penumbra_sdk_proto::{
    core::transaction::v1::{self as pbt},
    DomainType, Message,
};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_shielded_pool::{Note, Output, Spend, Transfer};
use penumbra_sdk_stake::{Delegate, Undelegate, UndelegateClaim};
use penumbra_sdk_tct as tct;
use penumbra_sdk_tct::StateCommitment;
use penumbra_sdk_txhash::{
    AuthHash, AuthorizingData, EffectHash, EffectingData, TransactionContext, TransactionId,
};
use serde::{Deserialize, Serialize};

use crate::{
    memo::{MemoCiphertext, MemoPlaintext},
    view::{
        action_view::{OutputView, TransferView},
        MemoView, TransactionBodyView,
    },
    Action, ActionView, DetectionData, IsAction, MemoPlaintextView, TransactionParameters,
    TransactionPerspective, TransactionView,
};

#[derive(Clone, Debug, Default)]
pub struct TransactionBody {
    pub actions: Vec<Action>,
    pub transaction_parameters: TransactionParameters,
    pub detection_data: Option<DetectionData>,
    pub memo: Option<MemoCiphertext>,
}

/// Represents a transaction summary containing multiple effects.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(try_from = "pbt::TransactionSummary", into = "pbt::TransactionSummary")]
pub struct TransactionSummary {
    pub effects: Vec<TransactionEffect>,
}

/// Represents an individual effect of a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionEffect {
    pub address: AddressView,
    pub balance: Balance,
}

impl EffectingData for TransactionBody {
    fn effect_hash(&self) -> EffectHash {
        let mut state = blake2b_simd::Params::new()
            .personal(b"PenumbraEfHs")
            .to_state();

        let parameters_hash = self.transaction_parameters.effect_hash();
        let memo_hash = self
            .memo
            .as_ref()
            .map(|memo| memo.effect_hash())
            // If the memo is not present, use the all-zero hash to record its absence in
            // the overall effect hash.
            .unwrap_or_default();
        let detection_data_hash = self
            .detection_data
            .as_ref()
            .map(|detection_data| detection_data.effect_hash())
            // If the detection data is not present, use the all-zero hash to
            // record its absence in the overall effect hash.
            .unwrap_or_default();

        // Hash the fixed data of the transaction body.
        state.update(parameters_hash.as_bytes());
        state.update(memo_hash.as_bytes());
        state.update(detection_data_hash.as_bytes());

        // Hash the number of actions, then each action.
        let num_actions = self.actions.len() as u32;
        state.update(&num_actions.to_le_bytes());
        for action in &self.actions {
            state.update(action.effect_hash().as_bytes());
        }

        EffectHash(state.finalize().as_array().clone())
    }
}

impl EffectingData for Transaction {
    fn effect_hash(&self) -> EffectHash {
        self.transaction_body.effect_hash()
    }
}

impl AuthorizingData for TransactionBody {
    fn auth_hash(&self) -> AuthHash {
        AuthHash(
            blake2b_simd::Params::default()
                .hash(&self.encode_to_vec())
                .as_bytes()[0..32]
                .try_into()
                .expect("blake2b output is always 32 bytes long"),
        )
    }
}

impl AuthorizingData for Transaction {
    fn auth_hash(&self) -> AuthHash {
        self.transaction_body.auth_hash()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pbt::Transaction", into = "pbt::Transaction")]
pub struct Transaction {
    pub transaction_body: TransactionBody,
    pub binding_sig: Signature<Binding>,
    pub anchor: tct::Root,
}

impl Default for Transaction {
    fn default() -> Self {
        Transaction {
            transaction_body: Default::default(),
            binding_sig: [0u8; 64].into(),
            anchor: tct::Tree::new().root(),
        }
    }
}

impl Transaction {
    pub fn context(&self) -> TransactionContext {
        TransactionContext {
            anchor: self.anchor,
            effect_hash: self.effect_hash(),
        }
    }

    pub fn num_proofs(&self) -> usize {
        self.transaction_body
            .actions
            .iter()
            .map(|action| match action {
                Action::Spend(_) => 1,
                Action::Output(_) => 1,
                Action::Transfer(_) => 1,
                Action::Swap(_) => 1,
                Action::SwapClaim(_) => 1,
                Action::UndelegateClaim(_) => 1,
                Action::DelegatorVote(_) => 1,
                Action::ActionLiquidityTournamentVote(_) => 1,
                _ => 0,
            })
            .sum()
    }

    pub fn is_aggregate_bundle_tx(&self) -> bool {
        matches!(
            self.transaction_body.actions.as_slice(),
            [Action::AggregateBundle(_)]
        )
    }

    pub fn aggregate_bundle_action(
        &self,
    ) -> Option<&penumbra_sdk_proof_aggregation::AggregateBundle> {
        self.actions().find_map(|action| {
            if let Action::AggregateBundle(bundle) = action {
                Some(bundle)
            } else {
                None
            }
        })
    }

    pub fn contains_aggregate_bundle_action(&self) -> bool {
        self.aggregate_bundle_action().is_some()
    }

    /// Helper function for decrypting the memo on the transaction given an FVK.
    ///
    /// Will return an Error if there is no memo.
    pub fn decrypt_memo(&self, fvk: &FullViewingKey) -> anyhow::Result<MemoPlaintext> {
        // Error if we don't have an encrypted memo field to decrypt.
        if self.transaction_body().memo.is_none() {
            return Err(anyhow::anyhow!("no memo"));
        }

        // Any note-creating shielded action with an outgoing payload key will let us decrypt the memo.
        if let Some((note_payload, ovk_wrapped_key, wrapped_memo_key, balance_commitment)) =
            self.actions().find_map(|action| match action {
                Action::Output(output) => Some((
                    output.body.note_payload.clone(),
                    output.body.ovk_wrapped_key.clone(),
                    output.body.wrapped_memo_key.clone(),
                    output.body.balance_commitment,
                )),
                Action::Transfer(transfer) => transfer.body.outputs.first().map(|output| {
                    (
                        output.note_payload.clone(),
                        output.ovk_wrapped_key.clone(),
                        output.wrapped_memo_key.clone(),
                        transfer.body.balance_commitment,
                    )
                }),
                _ => None,
            })
        {
            let shared_secret = Note::decrypt_key(
                ovk_wrapped_key,
                note_payload.note_commitment,
                balance_commitment,
                fvk.outgoing(),
                &note_payload.ephemeral_key,
            );

            let memo_key: PayloadKey = match shared_secret {
                Ok(shared_secret) => {
                    let payload_key =
                        PayloadKey::derive(&shared_secret, &note_payload.ephemeral_key);
                    wrapped_memo_key.decrypt_outgoing(&payload_key)?
                }
                Err(_) => wrapped_memo_key.decrypt(note_payload.ephemeral_key, fvk.incoming())?,
            };

            // Now we can use the memo key to decrypt the memo.
            let tx_body = self.transaction_body();
            let memo_ciphertext = tx_body
                .memo
                .as_ref()
                .expect("memo field exists on this transaction");
            let decrypted_memo = MemoCiphertext::decrypt(&memo_key, memo_ciphertext.clone())?;

            // The memo is shared across all outputs, so we can stop here.
            return Ok(decrypted_memo);
        }

        // If we got here, we were unable to decrypt the memo.
        Err(anyhow::anyhow!("unable to decrypt memo"))
    }

    pub fn payload_keys(
        &self,
        fvk: &FullViewingKey,
    ) -> anyhow::Result<BTreeMap<StateCommitment, PayloadKey>> {
        let mut result = BTreeMap::new();

        for action in self.actions() {
            match action {
                Action::Swap(swap) => {
                    let commitment = swap.body.payload.commitment;
                    let payload_key = PayloadKey::derive_swap(fvk.outgoing(), commitment);

                    result.insert(commitment, payload_key);
                }
                Action::Output(output) => {
                    // Outputs may be either incoming or outgoing; for an outgoing output
                    // we need to use the ovk_wrapped_key, and for an incoming output we need to
                    // use the IVK to perform key agreement with the ephemeral key.
                    let ovk_wrapped_key = output.body.ovk_wrapped_key.clone();
                    let commitment = output.body.note_payload.note_commitment;
                    let epk = &output.body.note_payload.ephemeral_key;
                    let cv = output.body.balance_commitment;
                    let ovk = fvk.outgoing();
                    let shared_secret =
                        Note::decrypt_key(ovk_wrapped_key, commitment, cv, ovk, epk);

                    match shared_secret {
                        Ok(shared_secret) => {
                            // This is an outgoing output.
                            let payload_key = PayloadKey::derive(&shared_secret, epk);
                            result.insert(commitment, payload_key);
                        }
                        Err(_) => {
                            // This is (maybe) an incoming output, use the ivk.
                            let shared_secret = fvk.incoming().key_agreement_with(epk)?;
                            let payload_key = PayloadKey::derive(&shared_secret, epk);

                            result.insert(commitment, payload_key);
                        }
                    }
                }
                Action::Transfer(transfer) => {
                    for output in &transfer.body.outputs {
                        let ovk_wrapped_key = output.ovk_wrapped_key.clone();
                        let commitment = output.note_payload.note_commitment;
                        let epk = &output.note_payload.ephemeral_key;
                        let cv = transfer.body.balance_commitment;
                        let ovk = fvk.outgoing();
                        let shared_secret =
                            Note::decrypt_key(ovk_wrapped_key, commitment, cv, ovk, epk);

                        match shared_secret {
                            Ok(shared_secret) => {
                                let payload_key = PayloadKey::derive(&shared_secret, epk);
                                result.insert(commitment, payload_key);
                            }
                            Err(_) => {
                                let shared_secret = fvk.incoming().key_agreement_with(epk)?;
                                let payload_key = PayloadKey::derive(&shared_secret, epk);
                                result.insert(commitment, payload_key);
                            }
                        }
                    }
                }
                // These actions have no payload keys; they're listed explicitly
                // for exhaustiveness.
                Action::SwapClaim(_)
                | Action::Spend(_)
                | Action::Delegate(_)
                | Action::Undelegate(_)
                | Action::UndelegateClaim(_)
                | Action::ValidatorDefinition(_)
                | Action::IbcRelay(_)
                | Action::ProposalSubmit(_)
                | Action::ProposalWithdraw(_)
                | Action::ValidatorVote(_)
                | Action::DelegatorVote(_)
                | Action::ProposalDepositClaim(_)
                | Action::PositionOpen(_)
                | Action::PositionClose(_)
                | Action::PositionWithdraw(_)
                | Action::Ics20Withdrawal(_)
                | Action::CommunityPoolSpend(_)
                | Action::CommunityPoolOutput(_)
                | Action::CommunityPoolDeposit(_)
                | Action::ActionDutchAuctionSchedule(_)
                | Action::ActionDutchAuctionEnd(_)
                | Action::ActionDutchAuctionWithdraw(_)
                | Action::ActionLiquidityTournamentVote(_)
                | Action::ComplianceRegisterAsset(_)
                | Action::ComplianceRegisterUser(_)
                | Action::AggregateBundle(_) => {}
            }
        }

        Ok(result)
    }

    pub fn view_from_perspective(&self, txp: &TransactionPerspective) -> TransactionView {
        let mut action_views = Vec::new();

        let mut memo_plaintext: Option<MemoPlaintext> = None;
        let mut memo_ciphertext: Option<MemoCiphertext> = None;

        for action in self.actions() {
            let action_view = action.view_from_perspective(txp);

            // In the case of Output actions, decrypt the transaction memo if this hasn't already been done.
            if matches!(
                &action_view,
                ActionView::Output(_) | ActionView::Transfer(_)
            ) {
                if memo_plaintext.is_none() {
                    memo_plaintext = match self.transaction_body().memo {
                        Some(ciphertext) => {
                            memo_ciphertext = Some(ciphertext.clone());
                            match &action_view {
                                ActionView::Output(OutputView::Visible {
                                    output: _,
                                    note: _,
                                    payload_key: decrypted_memo_key,
                                }) => MemoCiphertext::decrypt(decrypted_memo_key, ciphertext).ok(),
                                ActionView::Output(OutputView::Opaque { output: _ }) => None,
                                ActionView::Transfer(TransferView::Visible {
                                    transfer: _,
                                    spent_notes: _,
                                    created_notes: _,
                                    payload_key: decrypted_memo_key,
                                }) => MemoCiphertext::decrypt(decrypted_memo_key, ciphertext).ok(),
                                ActionView::Transfer(TransferView::Opaque { transfer: _ }) => None,
                                _ => None,
                            }
                        }
                        None => None,
                    }
                }
            }

            action_views.push(action_view);
        }

        let memo_view = match memo_ciphertext {
            Some(ciphertext) => match memo_plaintext {
                Some(plaintext) => {
                    let plaintext_view: MemoPlaintextView = MemoPlaintextView {
                        return_address: txp.view_address(plaintext.return_address()),
                        text: plaintext.text().to_owned(),
                    };
                    Some(MemoView::Visible {
                        plaintext: plaintext_view,
                        ciphertext,
                    })
                }
                None => Some(MemoView::Opaque { ciphertext }),
            },
            None => None,
        };

        let detection_data =
            self.transaction_body()
                .detection_data
                .as_ref()
                .map(|detection_data| DetectionData {
                    fmd_clues: detection_data.fmd_clues.clone(),
                });

        TransactionView {
            body_view: TransactionBodyView {
                action_views,
                transaction_parameters: self.transaction_parameters(),
                detection_data,
                memo_view,
            },
            binding_sig: self.binding_sig,
            anchor: self.anchor,
        }
    }

    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.transaction_body.actions.iter()
    }

    pub fn delegations(&self) -> impl Iterator<Item = &Delegate> {
        self.actions().filter_map(|action| {
            if let Action::Delegate(d) = action {
                Some(d)
            } else {
                None
            }
        })
    }

    pub fn undelegations(&self) -> impl Iterator<Item = &Undelegate> {
        self.actions().filter_map(|action| {
            if let Action::Undelegate(d) = action {
                Some(d)
            } else {
                None
            }
        })
    }

    pub fn undelegate_claims(&self) -> impl Iterator<Item = &UndelegateClaim> {
        self.actions().filter_map(|action| {
            if let Action::UndelegateClaim(d) = action {
                Some(d)
            } else {
                None
            }
        })
    }

    pub fn proposal_submits(&self) -> impl Iterator<Item = &ProposalSubmit> {
        self.actions().filter_map(|action| {
            if let Action::ProposalSubmit(s) = action {
                Some(s)
            } else {
                None
            }
        })
    }

    pub fn proposal_withdraws(&self) -> impl Iterator<Item = &ProposalWithdraw> {
        self.actions().filter_map(|action| {
            if let Action::ProposalWithdraw(w) = action {
                Some(w)
            } else {
                None
            }
        })
    }

    pub fn validator_votes(&self) -> impl Iterator<Item = &ValidatorVote> {
        self.actions().filter_map(|action| {
            if let Action::ValidatorVote(v) = action {
                Some(v)
            } else {
                None
            }
        })
    }

    pub fn delegator_votes(&self) -> impl Iterator<Item = &DelegatorVote> {
        self.actions().filter_map(|action| {
            if let Action::DelegatorVote(v) = action {
                Some(v)
            } else {
                None
            }
        })
    }

    pub fn ibc_actions(&self) -> impl Iterator<Item = &IbcRelay> {
        self.actions().filter_map(|action| {
            if let Action::IbcRelay(ibc_action) = action {
                Some(ibc_action)
            } else {
                None
            }
        })
    }

    pub fn validator_definitions(
        &self,
    ) -> impl Iterator<Item = &penumbra_sdk_stake::validator::Definition> {
        self.actions().filter_map(|action| {
            if let Action::ValidatorDefinition(d) = action {
                Some(d)
            } else {
                None
            }
        })
    }

    pub fn outputs(&self) -> impl Iterator<Item = &Output> {
        self.actions().filter_map(|action| {
            if let Action::Output(d) = action {
                Some(d)
            } else {
                None
            }
        })
    }

    pub fn transfers(&self) -> impl Iterator<Item = &Transfer> {
        self.actions().filter_map(|action| {
            if let Action::Transfer(transfer) = action {
                Some(transfer)
            } else {
                None
            }
        })
    }

    pub fn swaps(&self) -> impl Iterator<Item = &Swap> {
        self.actions().filter_map(|action| {
            if let Action::Swap(s) = action {
                Some(s)
            } else {
                None
            }
        })
    }

    pub fn spent_nullifiers(&self) -> impl Iterator<Item = Nullifier> + '_ {
        self.actions().flat_map(|action| match action {
            Action::Spend(spend) => vec![spend.body.nullifier],
            Action::Transfer(transfer) => transfer
                .body
                .inputs
                .iter()
                .map(|input| input.nullifier)
                .collect(),
            Action::SwapClaim(swap_claim) => vec![swap_claim.body.nullifier],
            _ => Vec::new(),
        })
    }

    pub fn state_commitments(&self) -> impl Iterator<Item = StateCommitment> + '_ {
        self.actions()
            .flat_map(|action| {
                // Note: adding future actions that include state commitments
                // will need to be matched here.
                match action {
                    Action::Output(output) => vec![Some(output.body.note_payload.note_commitment)],
                    Action::Transfer(transfer) => transfer
                        .body
                        .outputs
                        .iter()
                        .map(|output| Some(output.note_payload.note_commitment))
                        .collect::<Vec<_>>(),
                    Action::Swap(swap) => vec![Some(swap.body.payload.commitment)],
                    Action::SwapClaim(claim) => vec![
                        Some(claim.body.output_1_commitment),
                        Some(claim.body.output_2_commitment),
                    ],
                    _ => vec![None],
                }
            })
            .filter_map(|x| x)
    }

    pub fn community_pool_deposits(&self) -> impl Iterator<Item = &CommunityPoolDeposit> {
        self.actions().filter_map(|action| {
            if let Action::CommunityPoolDeposit(d) = action {
                Some(d)
            } else {
                None
            }
        })
    }

    pub fn community_pool_spends(&self) -> impl Iterator<Item = &CommunityPoolSpend> {
        self.actions().filter_map(|action| {
            if let Action::CommunityPoolSpend(s) = action {
                Some(s)
            } else {
                None
            }
        })
    }

    pub fn spends(&self) -> impl Iterator<Item = &Spend> {
        self.actions().filter_map(|action| {
            if let Action::Spend(s) = action {
                Some(s)
            } else {
                None
            }
        })
    }

    pub fn community_pool_outputs(&self) -> impl Iterator<Item = &CommunityPoolOutput> {
        self.actions().filter_map(|action| {
            if let Action::CommunityPoolOutput(o) = action {
                Some(o)
            } else {
                None
            }
        })
    }

    pub fn position_openings(&self) -> impl Iterator<Item = &PositionOpen> {
        self.actions().filter_map(|action| {
            if let Action::PositionOpen(d) = action {
                Some(d)
            } else {
                None
            }
        })
    }

    pub fn position_closings(&self) -> impl Iterator<Item = &PositionClose> {
        self.actions().filter_map(|action| {
            if let Action::PositionClose(d) = action {
                Some(d)
            } else {
                None
            }
        })
    }

    pub fn transaction_body(&self) -> TransactionBody {
        self.transaction_body.clone()
    }

    pub fn transaction_parameters(&self) -> TransactionParameters {
        self.transaction_body.transaction_parameters.clone()
    }

    pub fn binding_sig(&self) -> &Signature<Binding> {
        &self.binding_sig
    }

    pub fn id(&self) -> TransactionId {
        use sha2::{Digest, Sha256};

        let tx_bytes: Vec<u8> = self.clone().try_into().expect("can serialize transaction");
        let mut id_bytes = [0; 32];
        id_bytes[..].copy_from_slice(Sha256::digest(&tx_bytes).as_slice());

        TransactionId(id_bytes)
    }

    /// Compute the binding verification key from the transaction data.
    pub fn binding_verification_key(&self) -> VerificationKey<Binding> {
        let mut balance_commitments = decaf377::Element::default();
        for action in &self.transaction_body.actions {
            balance_commitments += action.balance_commitment().0;
        }

        // Add fee into binding verification key computation.
        let fee_v_blinding = Fr::zero();
        let fee_value_commitment = self
            .transaction_body
            .transaction_parameters
            .fee
            .commit(fee_v_blinding);
        balance_commitments += fee_value_commitment.0;

        let binding_verification_key_bytes: VerificationKeyBytes<Binding> =
            balance_commitments.vartime_compress().0.into();

        binding_verification_key_bytes
            .try_into()
            .expect("verification key is valid")
    }
}

#[cfg(test)]
mod tests {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use penumbra_sdk_asset::{asset, Balance, Value};
    use penumbra_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
    use penumbra_sdk_sct::Nullifier;

    use super::{Action, Transaction, TransactionBody};

    #[test]
    fn transfer_counts_as_nullifier_and_state_commitment_source() {
        let transfer = penumbra_sdk_shielded_pool::Transfer {
            body: penumbra_sdk_shielded_pool::TransferBody {
                family_id: penumbra_sdk_shielded_pool::TransferFamilyId::OneByOne,
                anchor: penumbra_sdk_tct::Tree::default().root(),
                balance_commitment: Balance::from(Value {
                    amount: 9u64.into(),
                    asset_id: asset::Id(decaf377::Fq::from(1u64)),
                })
                .commit(decaf377::Fr::from(2u64)),
                inputs: vec![penumbra_sdk_shielded_pool::TransferInputBody {
                    nullifier: Nullifier(decaf377::Fq::from(3u64)),
                    rk: VerificationKey::from(SigningKey::<SpendAuth>::from(decaf377::Fr::from(
                        4u64,
                    ))),
                    encrypted_backref: penumbra_sdk_shielded_pool::EncryptedBackref::dummy(),
                    compliance_ciphertext: vec![1, 2, 3],
                    dleq_proof: vec![13, 14],
                }],
                outputs: vec![penumbra_sdk_shielded_pool::TransferOutputBody {
                    note_payload: penumbra_sdk_shielded_pool::NotePayload {
                        note_commitment: penumbra_sdk_tct::StateCommitment(decaf377::Fq::from(
                            5u64,
                        )),
                        ephemeral_key: decaf377_ka::Public([6u8; 32]),
                        encrypted_note: penumbra_sdk_shielded_pool::NoteCiphertext([7u8; 176]),
                    },
                    wrapped_memo_key: WrappedMemoKey([8u8; 48]),
                    ovk_wrapped_key: OvkWrappedKey([9u8; 48]),
                    compliance_ciphertext: vec![4, 5, 6],
                    dleq_proofs: vec![15, 16],
                }],
                target_timestamp: 10,
                compliance_anchor: penumbra_sdk_tct::StateCommitment(decaf377::Fq::from(11u64)),
                asset_anchor: penumbra_sdk_tct::StateCommitment(decaf377::Fq::from(12u64)),
            },
            auth_sigs: vec![[17u8; 64].into()],
            proof: penumbra_sdk_shielded_pool::TransferProof::default(),
        };

        let tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![Action::Transfer(transfer)],
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(tx.spent_nullifiers().collect::<Vec<_>>().len(), 1);
        assert_eq!(tx.state_commitments().collect::<Vec<_>>().len(), 1);
    }
}

impl DomainType for TransactionSummary {
    type Proto = pbt::TransactionSummary;
}

impl From<TransactionSummary> for pbt::TransactionSummary {
    fn from(summary: TransactionSummary) -> Self {
        pbt::TransactionSummary {
            effects: summary
                .effects
                .into_iter()
                .map(|effect| pbt::transaction_summary::Effects {
                    address: Some(effect.address.into()),
                    balance: Some(effect.balance.into()),
                })
                .collect(),
        }
    }
}

impl TryFrom<pbt::TransactionSummary> for TransactionSummary {
    type Error = anyhow::Error;

    fn try_from(pbt: pbt::TransactionSummary) -> Result<Self, Self::Error> {
        let effects = pbt
            .effects
            .into_iter()
            .map(|effect| {
                Ok(TransactionEffect {
                    address: effect
                        .address
                        .ok_or_else(|| anyhow::anyhow!("missing address field"))?
                        .try_into()?,
                    balance: effect
                        .balance
                        .ok_or_else(|| anyhow::anyhow!("missing balance field"))?
                        .try_into()?,
                })
            })
            .collect::<Result<Vec<TransactionEffect>, anyhow::Error>>()?;

        Ok(Self { effects })
    }
}

impl DomainType for TransactionBody {
    type Proto = pbt::TransactionBody;
}

impl From<TransactionBody> for pbt::TransactionBody {
    fn from(msg: TransactionBody) -> Self {
        pbt::TransactionBody {
            actions: msg.actions.into_iter().map(|x| x.into()).collect(),
            transaction_parameters: Some(msg.transaction_parameters.into()),
            detection_data: msg.detection_data.map(|x| x.into()),
            memo: msg.memo.map(Into::into),
        }
    }
}

impl TryFrom<pbt::TransactionBody> for TransactionBody {
    type Error = Error;

    fn try_from(proto: pbt::TransactionBody) -> anyhow::Result<Self, Self::Error> {
        let mut actions = Vec::<Action>::new();
        for action in proto.actions {
            actions.push(
                action
                    .try_into()
                    .context("action malformed while parsing transaction body")?,
            );
        }

        let memo = proto
            .memo
            .map(TryFrom::try_from)
            .transpose()
            .context("encrypted memo malformed while parsing transaction body")?;

        let detection_data = proto
            .detection_data
            .map(TryFrom::try_from)
            .transpose()
            .context("detection data malformed while parsing transaction body")?;

        let transaction_parameters = proto
            .transaction_parameters
            .ok_or_else(|| anyhow::anyhow!("transaction body missing transaction parameters"))?
            .try_into()
            .context("transaction parameters malformed")?;

        Ok(TransactionBody {
            actions,
            transaction_parameters,
            detection_data,
            memo,
        })
    }
}

impl DomainType for Transaction {
    type Proto = pbt::Transaction;
}

impl From<Transaction> for pbt::Transaction {
    fn from(msg: Transaction) -> Self {
        pbt::Transaction {
            body: Some(msg.transaction_body.into()),
            anchor: Some(msg.anchor.into()),
            binding_sig: Some(msg.binding_sig.into()),
        }
    }
}

impl From<&Transaction> for pbt::Transaction {
    fn from(msg: &Transaction) -> Self {
        Transaction {
            transaction_body: msg.transaction_body.clone(),
            anchor: msg.anchor.clone(),
            binding_sig: msg.binding_sig.clone(),
        }
        .into()
    }
}

impl TryFrom<pbt::Transaction> for Transaction {
    type Error = Error;

    fn try_from(proto: pbt::Transaction) -> anyhow::Result<Self, Self::Error> {
        let transaction_body = proto
            .body
            .ok_or_else(|| anyhow::anyhow!("transaction missing body"))?
            .try_into()
            .context("transaction body malformed")?;

        let binding_sig = proto
            .binding_sig
            .ok_or_else(|| anyhow::anyhow!("transaction missing binding signature"))?
            .try_into()
            .context("transaction binding signature malformed")?;

        let anchor = proto
            .anchor
            .ok_or_else(|| anyhow::anyhow!("transaction missing anchor"))?
            .try_into()
            .context("transaction anchor malformed")?;

        Ok(Transaction {
            transaction_body,
            binding_sig,
            anchor,
        })
    }
}

impl TryFrom<&[u8]> for Transaction {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Transaction, Self::Error> {
        pbt::Transaction::decode(bytes)?.try_into()
    }
}

impl TryFrom<Vec<u8>> for Transaction {
    type Error = Error;

    fn try_from(bytes: Vec<u8>) -> Result<Transaction, Self::Error> {
        Self::try_from(&bytes[..])
    }
}

impl From<Transaction> for Vec<u8> {
    fn from(transaction: Transaction) -> Vec<u8> {
        let protobuf_serialized: pbt::Transaction = transaction.into();
        protobuf_serialized.encode_to_vec()
    }
}

impl From<&Transaction> for Vec<u8> {
    fn from(transaction: &Transaction) -> Vec<u8> {
        let protobuf_serialized: pbt::Transaction = transaction.into();
        protobuf_serialized.encode_to_vec()
    }
}
