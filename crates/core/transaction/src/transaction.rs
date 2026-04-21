use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
};

use anyhow::{Context, Error};
use ark_ff::Zero;
use decaf377::Fr;
use decaf377_rdsa::{Binding, Signature, VerificationKey, VerificationKeyBytes};
use penumbra_sdk_asset::Balance;
use penumbra_sdk_governance::{ProposalSubmit, ValidatorVote};
use penumbra_sdk_ibc::IbcRelay;
use penumbra_sdk_keys::{AddressView, FullViewingKey, PayloadKey};
use penumbra_sdk_proto::{
    core::transaction::v1::{self as pbt},
    DomainType, Message,
};
use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_shielded_pool::{Consolidate, Note, ShieldedIcs20WithdrawalView, Split, Transfer};
use penumbra_sdk_tct as tct;
use penumbra_sdk_tct::StateCommitment;
use penumbra_sdk_txhash::{
    AuthHash, AuthorizingData, EffectHash, EffectingData, TransactionContext, TransactionId,
};
use serde::{Deserialize, Serialize};

use crate::{
    fee_funding::FeeFunding,
    memo::{MemoCiphertext, MemoPlaintext},
    view::{
        action_view::{ConsolidateView, SplitView, TransferView},
        MemoView, TransactionBodyView,
    },
    Action, ActionView, DetectionData, IsAction, MemoPlaintextView, TransactionParameters,
    TransactionPerspective, TransactionView,
};

#[derive(Clone, Debug, Default)]
pub struct TransactionBody {
    pub actions: Vec<Action>,
    pub transaction_parameters: TransactionParameters,
    pub fee_funding: Option<FeeFunding>,
    pub detection_data: Option<DetectionData>,
    pub memo: Option<MemoCiphertext>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(try_from = "pbt::TransactionSummary", into = "pbt::TransactionSummary")]
pub struct TransactionSummary {
    pub effects: Vec<TransactionEffect>,
}

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
            .unwrap_or_default();
        let detection_data_hash = self
            .detection_data
            .as_ref()
            .map(|detection_data| detection_data.effect_hash())
            .unwrap_or_default();
        let fee_funding_hash = self
            .fee_funding
            .as_ref()
            .map(EffectingData::effect_hash)
            .unwrap_or_default();

        state.update(parameters_hash.as_bytes());
        state.update(memo_hash.as_bytes());
        state.update(detection_data_hash.as_bytes());
        state.update(fee_funding_hash.as_bytes());

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
                Action::Transfer(_)
                | Action::Consolidate(_)
                | Action::Split(_)
                | Action::ShieldedIcs20Withdrawal(_) => 1,
                _ => 0,
            })
            .sum::<usize>()
            + usize::from(self.transaction_body.fee_funding.is_some())
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

    pub fn decrypt_memo(&self, fvk: &FullViewingKey) -> anyhow::Result<MemoPlaintext> {
        if self.transaction_body().memo.is_none() {
            return Err(anyhow::anyhow!("no memo"));
        }

        if let Some((note_payload, ovk_wrapped_key, wrapped_memo_key, balance_commitment)) = self
            .actions()
            .find_map(|action| match action {
                Action::Transfer(transfer) => transfer.body.outputs.iter().next().map(|output| {
                    (
                        output.note_payload.clone(),
                        output.ovk_wrapped_key.clone(),
                        output.wrapped_memo_key.clone(),
                        transfer.body.balance_commitment,
                    )
                }),
                Action::Consolidate(consolidate) => {
                    consolidate.body.outputs.iter().next().map(|output| {
                        (
                            output.note_payload.clone(),
                            output.ovk_wrapped_key.clone(),
                            output.wrapped_memo_key.clone(),
                            consolidate.body.balance_commitment,
                        )
                    })
                }
                Action::Split(split) => split.body.outputs.iter().next().map(|output| {
                    (
                        output.note_payload.clone(),
                        output.ovk_wrapped_key.clone(),
                        output.wrapped_memo_key.clone(),
                        split.body.balance_commitment,
                    )
                }),
                Action::ShieldedIcs20Withdrawal(withdrawal) => Some((
                    withdrawal.body.change_output.note_payload.clone(),
                    withdrawal.body.change_output.ovk_wrapped_key.clone(),
                    withdrawal.body.change_output.wrapped_memo_key.clone(),
                    withdrawal.body.balance_commitment,
                )),
                _ => None,
            })
            .or_else(|| {
                self.transaction_body
                    .fee_funding
                    .as_ref()
                    .and_then(|fee_funding| {
                        fee_funding
                            .transfer
                            .body
                            .outputs
                            .iter()
                            .next()
                            .map(|output| {
                                (
                                    output.note_payload.clone(),
                                    output.ovk_wrapped_key.clone(),
                                    output.wrapped_memo_key.clone(),
                                    fee_funding.transfer.body.balance_commitment,
                                )
                            })
                    })
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

            let tx_body = self.transaction_body();
            let memo_ciphertext = tx_body
                .memo
                .as_ref()
                .expect("memo field exists on this transaction");
            return MemoCiphertext::decrypt(&memo_key, memo_ciphertext.clone());
        }

        Err(anyhow::anyhow!("unable to decrypt memo"))
    }

    pub fn payload_keys(
        &self,
        fvk: &FullViewingKey,
    ) -> anyhow::Result<BTreeMap<StateCommitment, PayloadKey>> {
        let mut result = BTreeMap::new();

        for action in self.actions() {
            match action {
                Action::Transfer(transfer) => {
                    insert_payload_keys_for_outputs(
                        &mut result,
                        &transfer.body.outputs,
                        transfer.body.balance_commitment,
                        fvk,
                    )?;
                }
                Action::Consolidate(consolidate) => {
                    insert_payload_keys_for_outputs(
                        &mut result,
                        &consolidate.body.outputs,
                        consolidate.body.balance_commitment,
                        fvk,
                    )?;
                }
                Action::Split(split) => {
                    insert_payload_keys_for_outputs(
                        &mut result,
                        &split.body.outputs,
                        split.body.balance_commitment,
                        fvk,
                    )?;
                }
                Action::ShieldedIcs20Withdrawal(withdrawal) => {
                    let output = &withdrawal.body.change_output;
                    let ovk_wrapped_key = output.ovk_wrapped_key.clone();
                    let commitment = output.note_payload.note_commitment;
                    let epk = &output.note_payload.ephemeral_key;
                    let cv = withdrawal.body.balance_commitment;
                    let shared_secret =
                        Note::decrypt_key(ovk_wrapped_key, commitment, cv, fvk.outgoing(), epk);

                    match shared_secret {
                        Ok(shared_secret) => {
                            result.insert(commitment, PayloadKey::derive(&shared_secret, epk));
                        }
                        Err(_) => {
                            let shared_secret = fvk.incoming().key_agreement_with(epk)?;
                            result.insert(commitment, PayloadKey::derive(&shared_secret, epk));
                        }
                    }
                }
                Action::ValidatorDefinition(_)
                | Action::IbcRelay(_)
                | Action::ProposalSubmit(_)
                | Action::ValidatorVote(_)
                | Action::ComplianceRegisterAsset(_)
                | Action::ComplianceRegisterUser(_)
                | Action::AggregateBundle(_) => {}
            }
        }

        if let Some(fee_funding) = &self.transaction_body.fee_funding {
            insert_payload_keys_for_outputs(
                &mut result,
                &fee_funding.transfer.body.outputs,
                fee_funding.transfer.body.balance_commitment,
                fvk,
            )?;
        }

        Ok(result)
    }

    pub fn view_from_perspective(&self, txp: &TransactionPerspective) -> TransactionView {
        let mut action_views = Vec::new();
        let mut memo_plaintext: Option<MemoPlaintext> = None;
        let mut memo_ciphertext: Option<MemoCiphertext> = None;

        for action in self.actions() {
            let action_view = action.view_from_perspective(txp);

            if matches!(
                &action_view,
                ActionView::Transfer(_)
                    | ActionView::Consolidate(_)
                    | ActionView::Split(_)
                    | ActionView::ShieldedIcs20Withdrawal(_)
            ) && memo_plaintext.is_none()
            {
                memo_plaintext = match self.transaction_body().memo {
                    Some(ciphertext) => {
                        memo_ciphertext = Some(ciphertext.clone());
                        payload_key_from_view(&action_view).and_then(|payload_key| {
                            MemoCiphertext::decrypt(payload_key, ciphertext).ok()
                        })
                    }
                    None => None,
                }
            }

            action_views.push(action_view);
        }

        let fee_funding = self
            .transaction_body
            .fee_funding
            .as_ref()
            .map(|fee_funding| fee_funding.view_from_perspective(txp));
        if memo_plaintext.is_none() {
            if let (Some(ciphertext), Some(TransferView::Visible { payload_key, .. })) =
                (self.transaction_body().memo, fee_funding.as_ref())
            {
                memo_ciphertext = Some(ciphertext.clone());
                memo_plaintext = MemoCiphertext::decrypt(payload_key, ciphertext).ok();
            }
        }

        let memo_view = match memo_ciphertext {
            Some(ciphertext) => match memo_plaintext {
                Some(plaintext) => Some(MemoView::Visible {
                    plaintext: MemoPlaintextView {
                        return_address: txp.view_address(plaintext.return_address()),
                        text: plaintext.text().to_owned(),
                    },
                    ciphertext,
                }),
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
                fee_funding,
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

    pub fn proposal_submits(&self) -> impl Iterator<Item = &ProposalSubmit> {
        self.actions().filter_map(|action| {
            if let Action::ProposalSubmit(submit) = action {
                Some(submit)
            } else {
                None
            }
        })
    }

    pub fn validator_votes(&self) -> impl Iterator<Item = &ValidatorVote> {
        self.actions().filter_map(|action| {
            if let Action::ValidatorVote(vote) = action {
                Some(vote)
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
    ) -> impl Iterator<Item = &penumbra_sdk_validator::validator::Definition> {
        self.actions().filter_map(|action| {
            if let Action::ValidatorDefinition(definition) = action {
                Some(definition)
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

    pub fn consolidations(&self) -> impl Iterator<Item = &Consolidate> {
        self.actions().filter_map(|action| {
            if let Action::Consolidate(consolidate) = action {
                Some(consolidate)
            } else {
                None
            }
        })
    }

    pub fn splits(&self) -> impl Iterator<Item = &Split> {
        self.actions().filter_map(|action| {
            if let Action::Split(split) = action {
                Some(split)
            } else {
                None
            }
        })
    }

    pub fn spent_nullifiers(&self) -> impl Iterator<Item = Nullifier> + '_ {
        let mut nullifiers = self
            .actions()
            .flat_map(|action| match action {
                Action::Transfer(transfer) => transfer
                    .body
                    .inputs
                    .iter()
                    .filter(|input| !input.is_dummy())
                    .map(|input| input.nullifier)
                    .collect(),
                Action::Consolidate(consolidate) => consolidate
                    .body
                    .inputs
                    .iter()
                    .map(|input| input.nullifier)
                    .collect(),
                Action::Split(split) => split
                    .body
                    .inputs
                    .iter()
                    .map(|input| input.nullifier)
                    .collect(),
                Action::ShieldedIcs20Withdrawal(withdrawal) => withdrawal
                    .body
                    .inputs
                    .iter()
                    .map(|input| input.nullifier)
                    .collect(),
                _ => Vec::new(),
            })
            .collect::<Vec<_>>();

        if let Some(fee_funding) = &self.transaction_body.fee_funding {
            nullifiers.extend(
                fee_funding
                    .transfer
                    .body
                    .inputs
                    .iter()
                    .filter(|input| !input.is_dummy())
                    .map(|input| input.nullifier),
            );
        }

        nullifiers.into_iter()
    }

    pub fn state_commitments(&self) -> impl Iterator<Item = StateCommitment> + '_ {
        let mut commitments = self
            .actions()
            .flat_map(|action| match action {
                Action::Transfer(transfer) => transfer
                    .body
                    .outputs
                    .iter()
                    .filter(|output| !output.is_dummy())
                    .map(|output| Some(output.note_payload.note_commitment))
                    .collect::<Vec<_>>(),
                Action::Consolidate(consolidate) => consolidate
                    .body
                    .outputs
                    .iter()
                    .map(|output| Some(output.note_payload.note_commitment))
                    .collect::<Vec<_>>(),
                Action::Split(split) => split
                    .body
                    .outputs
                    .iter()
                    .map(|output| Some(output.note_payload.note_commitment))
                    .collect::<Vec<_>>(),
                Action::ShieldedIcs20Withdrawal(withdrawal) => vec![Some(
                    withdrawal.body.change_output.note_payload.note_commitment,
                )],
                _ => vec![None],
            })
            .filter_map(|x| x)
            .collect::<Vec<_>>();

        if let Some(fee_funding) = &self.transaction_body.fee_funding {
            commitments.extend(
                fee_funding
                    .transfer
                    .body
                    .outputs
                    .iter()
                    .filter(|output| !output.is_dummy())
                    .map(|output| output.note_payload.note_commitment),
            );
        }

        commitments.into_iter()
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

    pub fn binding_verification_key(&self) -> VerificationKey<Binding> {
        let mut balance_commitments = decaf377::Element::default();
        for action in &self.transaction_body.actions {
            balance_commitments += action.balance_commitment().0;
        }
        if let Some(fee_funding) = &self.transaction_body.fee_funding {
            balance_commitments += fee_funding.balance_commitment().0;
        }

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

fn insert_payload_keys_for_outputs<Output>(
    result: &mut BTreeMap<StateCommitment, PayloadKey>,
    outputs: &[Output],
    balance_commitment: penumbra_sdk_asset::balance::Commitment,
    fvk: &FullViewingKey,
) -> anyhow::Result<()>
where
    for<'a> &'a Output: IntoOutputRef<'a>,
{
    for output in outputs {
        let output = output.into_output_ref();
        let commitment = output.note_payload.note_commitment;
        let epk = &output.note_payload.ephemeral_key;
        let shared_secret = Note::decrypt_key(
            output.ovk_wrapped_key.clone(),
            commitment,
            balance_commitment,
            fvk.outgoing(),
            epk,
        );

        match shared_secret {
            Ok(shared_secret) => {
                result.insert(commitment, PayloadKey::derive(&shared_secret, epk));
            }
            Err(_) => {
                let shared_secret = fvk.incoming().key_agreement_with(epk)?;
                result.insert(commitment, PayloadKey::derive(&shared_secret, epk));
            }
        }
    }

    Ok(())
}

trait IntoOutputRef<'a> {
    fn into_output_ref(self) -> OutputRef<'a>;
}

struct OutputRef<'a> {
    note_payload: &'a penumbra_sdk_shielded_pool::NotePayload,
    ovk_wrapped_key: &'a penumbra_sdk_keys::symmetric::OvkWrappedKey,
}

impl<'a> IntoOutputRef<'a> for &'a penumbra_sdk_shielded_pool::TransferOutputBody {
    fn into_output_ref(self) -> OutputRef<'a> {
        OutputRef {
            note_payload: &self.note_payload,
            ovk_wrapped_key: &self.ovk_wrapped_key,
        }
    }
}

impl<'a> IntoOutputRef<'a> for &'a penumbra_sdk_shielded_pool::ConsolidateOutputBody {
    fn into_output_ref(self) -> OutputRef<'a> {
        OutputRef {
            note_payload: &self.note_payload,
            ovk_wrapped_key: &self.ovk_wrapped_key,
        }
    }
}

impl<'a> IntoOutputRef<'a> for &'a penumbra_sdk_shielded_pool::SplitOutputBody {
    fn into_output_ref(self) -> OutputRef<'a> {
        OutputRef {
            note_payload: &self.note_payload,
            ovk_wrapped_key: &self.ovk_wrapped_key,
        }
    }
}

fn payload_key_from_view(action_view: &ActionView) -> Option<&PayloadKey> {
    match action_view {
        ActionView::Transfer(TransferView::Visible { payload_key, .. }) => Some(payload_key),
        ActionView::Transfer(TransferView::Opaque { .. }) => None,
        ActionView::Consolidate(ConsolidateView::Visible { payload_key, .. }) => Some(payload_key),
        ActionView::Consolidate(ConsolidateView::Opaque { .. }) => None,
        ActionView::Split(SplitView::Visible { payload_key, .. }) => Some(payload_key),
        ActionView::Split(SplitView::Opaque { .. }) => None,
        ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Visible {
            payload_key,
            ..
        }) => Some(payload_key),
        ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Opaque { .. }) => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use penumbra_sdk_asset::{asset, Balance, Value, BASE_ASSET_DENOM};
    use penumbra_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
    use penumbra_sdk_keys::Address;
    use penumbra_sdk_sct::Nullifier;
    use penumbra_sdk_shielded_pool::backref::ENCRYPTED_BACKREF_LEN;

    use super::{Action, Transaction, TransactionBody};

    #[test]
    fn transfer_counts_as_nullifier_and_state_commitment_source() {
        let transfer = penumbra_sdk_shielded_pool::Transfer {
            body: penumbra_sdk_shielded_pool::TransferBody {
                anchor: penumbra_sdk_tct::Tree::default().root(),
                balance_commitment: Balance::from(Value {
                    amount: 9u64.into(),
                    asset_id: asset::Id(decaf377::Fq::from(1u64)),
                })
                .commit(decaf377::Fr::from(2u64)),
                inputs: vec![
                    penumbra_sdk_shielded_pool::TransferInputBody {
                        nullifier: Nullifier(decaf377::Fq::from(3u64)),
                        rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                            decaf377::Fr::from(4u64),
                        )),
                        encrypted_backref: penumbra_sdk_shielded_pool::EncryptedBackref::try_from(
                            [1u8; ENCRYPTED_BACKREF_LEN],
                        )
                        .expect("valid encrypted backref"),
                        compliance_ciphertext: vec![1, 2, 3],
                        dleq_proof: vec![13, 14],
                    },
                    penumbra_sdk_shielded_pool::TransferInputBody {
                        nullifier: Nullifier(decaf377::Fq::from(30u64)),
                        rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                            decaf377::Fr::from(40u64),
                        )),
                        encrypted_backref: penumbra_sdk_shielded_pool::EncryptedBackref::try_from(
                            [2u8; ENCRYPTED_BACKREF_LEN],
                        )
                        .expect("valid encrypted backref"),
                        compliance_ciphertext: vec![],
                        dleq_proof: vec![],
                    },
                ],
                outputs: vec![
                    penumbra_sdk_shielded_pool::TransferOutputBody {
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
                    },
                    penumbra_sdk_shielded_pool::TransferOutputBody {
                        note_payload: penumbra_sdk_shielded_pool::NotePayload {
                            note_commitment: penumbra_sdk_tct::StateCommitment(decaf377::Fq::from(
                                50u64,
                            )),
                            ephemeral_key: decaf377_ka::Public([60u8; 32]),
                            encrypted_note: penumbra_sdk_shielded_pool::NoteCiphertext([70u8; 176]),
                        },
                        wrapped_memo_key: WrappedMemoKey([80u8; 48]),
                        ovk_wrapped_key: OvkWrappedKey([90u8; 48]),
                        compliance_ciphertext: vec![],
                        dleq_proofs: vec![],
                    },
                ],
                target_timestamp: 10,
                compliance_anchor: penumbra_sdk_tct::StateCommitment(decaf377::Fq::from(11u64)),
                asset_anchor: penumbra_sdk_tct::StateCommitment(decaf377::Fq::from(12u64)),
            },
            auth_sigs: vec![[17u8; 64].into(), [0u8; 64].into()],
            proof: penumbra_sdk_shielded_pool::TransferProof::default(),
        };

        let tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![Action::Transfer(transfer)],
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(tx.spent_nullifiers().collect::<Vec<_>>().len(), 2);
        assert_eq!(tx.state_commitments().collect::<Vec<_>>().len(), 2);
    }

    #[test]
    fn num_proofs_counts_new_shielded_action_families() {
        let tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![
                    Action::Consolidate(penumbra_sdk_shielded_pool::Consolidate {
                        body: penumbra_sdk_shielded_pool::ConsolidateBody {
                            family_id: penumbra_sdk_shielded_pool::ConsolidateFamilyId::TwoByOne,
                            anchor: penumbra_sdk_tct::Tree::default().root(),
                            balance_commitment: Balance::default().commit(decaf377::Fr::from(1u64)),
                            inputs: vec![
                                penumbra_sdk_shielded_pool::ConsolidateInputBody {
                                    nullifier: Nullifier(decaf377::Fq::from(2u64)),
                                    rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                        decaf377::Fr::from(3u64),
                                    )),
                                    encrypted_backref:
                                        penumbra_sdk_shielded_pool::EncryptedBackref::dummy(),
                                },
                                penumbra_sdk_shielded_pool::ConsolidateInputBody {
                                    nullifier: Nullifier(decaf377::Fq::from(4u64)),
                                    rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                        decaf377::Fr::from(5u64),
                                    )),
                                    encrypted_backref:
                                        penumbra_sdk_shielded_pool::EncryptedBackref::dummy(),
                                },
                            ],
                            outputs: vec![penumbra_sdk_shielded_pool::ConsolidateOutputBody {
                                note_payload: penumbra_sdk_shielded_pool::NotePayload {
                                    note_commitment: penumbra_sdk_tct::StateCommitment(
                                        decaf377::Fq::from(6u64),
                                    ),
                                    ephemeral_key: decaf377_ka::Public([7u8; 32]),
                                    encrypted_note:
                                        penumbra_sdk_shielded_pool::NoteCiphertext([8u8; 176]),
                                },
                                wrapped_memo_key: WrappedMemoKey([9u8; 48]),
                                ovk_wrapped_key: OvkWrappedKey([10u8; 48]),
                            }],
                        },
                        auth_sigs: vec![[11u8; 64].into(), [12u8; 64].into()],
                        proof: penumbra_sdk_shielded_pool::ConsolidateProof::default(),
                    }),
                    Action::Split(penumbra_sdk_shielded_pool::Split {
                        body: penumbra_sdk_shielded_pool::SplitBody {
                            family_id: penumbra_sdk_shielded_pool::SplitFamilyId::OneByFour,
                            anchor: penumbra_sdk_tct::Tree::default().root(),
                            balance_commitment: Balance::default().commit(decaf377::Fr::from(13u64)),
                            inputs: vec![penumbra_sdk_shielded_pool::SplitInputBody {
                                nullifier: Nullifier(decaf377::Fq::from(14u64)),
                                rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                    decaf377::Fr::from(15u64),
                                )),
                                encrypted_backref:
                                    penumbra_sdk_shielded_pool::EncryptedBackref::dummy(),
                            }],
                            outputs: vec![
                                penumbra_sdk_shielded_pool::SplitOutputBody {
                                    note_payload: penumbra_sdk_shielded_pool::NotePayload {
                                        note_commitment: penumbra_sdk_tct::StateCommitment(
                                            decaf377::Fq::from(16u64),
                                        ),
                                        ephemeral_key: decaf377_ka::Public([17u8; 32]),
                                        encrypted_note: penumbra_sdk_shielded_pool::NoteCiphertext(
                                            [18u8; 176],
                                        ),
                                    },
                                    wrapped_memo_key: WrappedMemoKey([19u8; 48]),
                                    ovk_wrapped_key: OvkWrappedKey([20u8; 48]),
                                };
                                4
                            ],
                        },
                        auth_sigs: vec![[21u8; 64].into()],
                        proof: penumbra_sdk_shielded_pool::SplitProof::default(),
                    }),
                    Action::ShieldedIcs20Withdrawal(
                        penumbra_sdk_shielded_pool::ShieldedIcs20Withdrawal {
                            body: penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalBody {
                                family_id:
                                    penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId::Canonical,
                                anchor: penumbra_sdk_tct::Tree::default().root(),
                                balance_commitment: Balance::default().commit(decaf377::Fr::from(22u64)),
                                inputs: vec![
                                    penumbra_sdk_shielded_pool::TransferInputBody {
                                        nullifier: Nullifier(decaf377::Fq::from(23u64)),
                                        rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                            decaf377::Fr::from(24u64),
                                        )),
                                        encrypted_backref:
                                            penumbra_sdk_shielded_pool::EncryptedBackref::dummy(),
                                        compliance_ciphertext: vec![],
                                        dleq_proof: vec![],
                                    },
                                    penumbra_sdk_shielded_pool::TransferInputBody {
                                        nullifier: Nullifier(decaf377::Fq::from(25u64)),
                                        rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                            decaf377::Fr::from(26u64),
                                        )),
                                        encrypted_backref:
                                            penumbra_sdk_shielded_pool::EncryptedBackref::dummy(),
                                        compliance_ciphertext: vec![],
                                        dleq_proof: vec![],
                                    },
                                ],
                                withdrawal: penumbra_sdk_shielded_pool::Ics20Withdrawal {
                                    amount: 1u64.into(),
                                    denom: BASE_ASSET_DENOM.clone(),
                                    destination_chain_address: "cosmos1deadbeef".to_string(),
                                    return_address: Address::dummy(&mut rand_core::OsRng),
                                    timeout_height: ibc_types::core::client::Height::new(0, 10)
                                        .expect("valid timeout height"),
                                    timeout_time: 1,
                                    source_channel: ibc_types::core::channel::ChannelId::new(7),
                                    use_compat_address: false,
                                    ics20_memo: String::new(),
                                    use_transparent_address: false,
                                },
                                change_output:
                                    penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalChangeBody {
                                        note_payload: penumbra_sdk_shielded_pool::NotePayload {
                                            note_commitment: penumbra_sdk_tct::StateCommitment(
                                                decaf377::Fq::from(27u64),
                                            ),
                                            ephemeral_key: decaf377_ka::Public([28u8; 32]),
                                            encrypted_note:
                                                penumbra_sdk_shielded_pool::NoteCiphertext(
                                                    [29u8; 176],
                                                ),
                                        },
                                        wrapped_memo_key: WrappedMemoKey([30u8; 48]),
                                        ovk_wrapped_key: OvkWrappedKey([31u8; 48]),
                                    },
                                target_timestamp: 0,
                                compliance_anchor: penumbra_sdk_tct::StateCommitment(
                                    decaf377::Fq::from(32u64),
                                ),
                                asset_anchor: penumbra_sdk_tct::StateCommitment(decaf377::Fq::from(33u64)),
                            },
                            auth_sigs: vec![[34u8; 64].into(), [35u8; 64].into()],
                            proof: penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalProof::default(),
                        },
                    ),
                ],
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(tx.num_proofs(), 3);
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
            actions: msg.actions.into_iter().map(Into::into).collect(),
            transaction_parameters: Some(msg.transaction_parameters.into()),
            fee_funding: msg.fee_funding.map(Into::into),
            detection_data: msg.detection_data.map(Into::into),
            memo: msg.memo.map(Into::into),
        }
    }
}

impl TryFrom<pbt::TransactionBody> for TransactionBody {
    type Error = Error;

    fn try_from(proto: pbt::TransactionBody) -> anyhow::Result<Self, Self::Error> {
        let actions = proto
            .actions
            .into_iter()
            .map(|action| {
                action
                    .try_into()
                    .context("action malformed while parsing transaction body")
            })
            .collect::<Result<Vec<_>, _>>()?;

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
        let fee_funding = proto
            .fee_funding
            .map(TryFrom::try_from)
            .transpose()
            .context("fee funding malformed while parsing transaction body")?;

        let transaction_parameters = proto
            .transaction_parameters
            .ok_or_else(|| anyhow::anyhow!("transaction body missing transaction parameters"))?
            .try_into()
            .context("transaction parameters malformed")?;

        Ok(TransactionBody {
            actions,
            transaction_parameters,
            fee_funding,
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
            anchor: msg.anchor,
            binding_sig: msg.binding_sig,
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
