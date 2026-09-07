use std::{
    collections::BTreeMap,
    convert::{TryFrom, TryInto},
};

use anyhow::{Context, Error};
use ark_ff::Zero;
use decaf377::Fr;
use decaf377_rdsa::{Binding, Signature, VerificationKey, VerificationKeyBytes};
use serde::{Deserialize, Serialize};
use shieldd_sdk_asset::Balance;
use shieldd_sdk_ibc::IbcRelay;
use shieldd_sdk_keys::{AddressView, FullViewingKey, PayloadKey};
use shieldd_sdk_proto::{
    core::transaction::v1::{self as pbt},
    DomainType, Message,
};
use shieldd_sdk_sct::nullifier_generation::{HistoricalNullifierProof, NullifierWindow};
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_shielded_pool::{
    Note, NoteReshape, ShieldedHostWithdrawal, ShieldedHostWithdrawalView,
    ShieldedIcs20WithdrawalView, Transfer,
};
use shieldd_sdk_tct as tct;
use shieldd_sdk_tct::StateCommitment;
use shieldd_sdk_txhash::{
    AuthHash, AuthorizingData, EffectHash, EffectingData, TransactionContext, TransactionId,
};

use crate::{
    fee_funding::FeeFunding,
    memo::{MemoCiphertext, MemoPlaintext},
    view::{
        action_view::{NoteReshapeView, TransferView},
        MemoView, TransactionBodyView,
    },
    Action, ActionView, IsAction, MemoPlaintextView, TransactionParameters, TransactionPerspective,
    TransactionView,
};

#[derive(Clone, Debug, Default)]
pub struct TransactionBody {
    pub actions: Vec<Action>,
    pub transaction_parameters: TransactionParameters,
    pub fee_funding: Option<FeeFunding>,
    pub memo: Option<MemoCiphertext>,
    pub nullifier_window: Option<NullifierWindow>,
    pub historical_nullifier_proofs: Vec<HistoricalNullifierProof>,
}

pub(crate) fn update_nullifier_window_effect_hash(
    state: &mut blake2b_simd::State,
    window: Option<&NullifierWindow>,
) {
    match window {
        None => {
            state.update(&[0]);
        }
        Some(window) => {
            state.update(&[1]);
            state.update(&window.protocol_version.to_le_bytes());
            state.update(&window.current_generation.to_le_bytes());
            state.update(&window.recent_position_floor.to_le_bytes());
            state.update(&window.archived_generation_count.to_le_bytes());
            state.update(&window.archived_history_head);
        }
    }
}

impl TransactionBody {
    fn proof_bound_inputs(&self) -> Vec<(Nullifier, bool)> {
        let mut inputs = Vec::new();
        for action in &self.actions {
            match action {
                Action::Transfer(action) => inputs.extend(
                    action
                        .body
                        .inputs
                        .iter()
                        .map(|input| (input.nullifier, input.history_required)),
                ),
                Action::NoteReshape(action) => inputs.extend(
                    action
                        .body
                        .inputs
                        .iter()
                        .map(|input| (input.nullifier, input.history_required)),
                ),
                Action::ShieldedIcs20Withdrawal(action) => inputs.extend(
                    action
                        .body
                        .inputs
                        .iter()
                        .map(|input| (input.nullifier, input.history_required)),
                ),
                Action::ShieldedHostWithdrawal(action) => inputs.extend(
                    action
                        .body
                        .inputs
                        .iter()
                        .map(|input| (input.nullifier, input.history_required)),
                ),
                _ => {}
            }
        }
        if let Some(fee_funding) = &self.fee_funding {
            inputs.extend(
                fee_funding
                    .transfer
                    .body
                    .inputs
                    .iter()
                    .map(|input| (input.nullifier, input.history_required)),
            );
        }

        inputs
    }

    /// Public nullifiers whose action proofs require complete retired-history proofs.
    pub fn historical_nullifiers(&self) -> Vec<Nullifier> {
        self.proof_bound_inputs()
            .into_iter()
            .filter_map(|(nullifier, required)| required.then_some(nullifier))
            .collect()
    }

    pub fn validate_nullifier_history(&self) -> anyhow::Result<()> {
        let inputs = self.proof_bound_inputs();

        if inputs.is_empty() {
            anyhow::ensure!(
                self.nullifier_window.is_none(),
                "spend-free transaction has a nullifier window"
            );
            anyhow::ensure!(
                self.historical_nullifier_proofs.is_empty(),
                "spend-free transaction has historical nullifier proofs"
            );
            return Ok(());
        }

        let window = self
            .nullifier_window
            .context("proof-bearing transaction is missing its nullifier window")?;
        window.validate()?;
        let old_nullifiers = self.historical_nullifiers();
        anyhow::ensure!(
            old_nullifiers.len() == self.historical_nullifier_proofs.len(),
            "historical proof count does not match old input count"
        );
        for (index, (nullifier, proof)) in old_nullifiers
            .into_iter()
            .zip(&self.historical_nullifier_proofs)
            .enumerate()
        {
            anyhow::ensure!(
                proof.nullifier == nullifier,
                "historical proof {index} does not match its old input nullifier"
            );
            proof
                .validate_structure(window)
                .with_context(|| format!("historical proof {index} has invalid structure"))?;
        }
        Ok(())
    }
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
            .personal(b"ShielddEfHs")
            .to_state();

        let parameters_hash = self.transaction_parameters.effect_hash();
        let memo_hash = self
            .memo
            .as_ref()
            .map(|memo| memo.effect_hash())
            .unwrap_or_default();
        let fee_funding_hash = self
            .fee_funding
            .as_ref()
            .map(EffectingData::effect_hash)
            .unwrap_or_default();

        state.update(parameters_hash.as_bytes());
        state.update(memo_hash.as_bytes());
        state.update(fee_funding_hash.as_bytes());
        update_nullifier_window_effect_hash(&mut state, self.nullifier_window.as_ref());

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

/// Canonical placeholder for no-proof transactions whose binding key is identity.
pub fn no_binding_signature() -> Signature<Binding> {
    [0u8; 64].into()
}

/// Returns whether a binding signature is the canonical no-proof placeholder.
pub fn is_no_binding_signature(signature: &Signature<Binding>) -> bool {
    signature.to_bytes() == [0u8; 64]
}

impl Default for Transaction {
    fn default() -> Self {
        Transaction {
            transaction_body: Default::default(),
            binding_sig: no_binding_signature(),
            anchor: tct::Tree::new().root(),
        }
    }
}

impl Transaction {
    /// Decodes only the unique protobuf encoding emitted by this domain type.
    pub fn decode_canonical(bytes: &[u8]) -> anyhow::Result<Self> {
        let tx: Self = pbt::Transaction::decode(bytes)
            .context("decoding transaction protobuf")?
            .try_into()?;
        let canonical: Vec<u8> = (&tx).into();
        anyhow::ensure!(
            canonical == bytes,
            "transaction bytes are not the canonical protobuf encoding"
        );
        Ok(tx)
    }

    pub fn context(&self) -> TransactionContext {
        TransactionContext {
            anchor: self.anchor,
            effect_hash: self.effect_hash(),
            recent_position_floor: self
                .transaction_body
                .nullifier_window
                .map_or(0, |window| window.recent_position_floor),
        }
    }

    pub fn num_proofs(&self) -> usize {
        self.transaction_body
            .actions
            .iter()
            .map(|action| match action {
                Action::Transfer(_)
                | Action::NoteReshape(_)
                | Action::ShieldedIcs20Withdrawal(_)
                | Action::ShieldedHostWithdrawal(_) => 1,
                Action::IbcRelay(_)
                | Action::ComplianceRegisterAsset(_)
                | Action::ComplianceRegisterUser(_)
                | Action::AggregateBundle(_) => 0,
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
    ) -> Option<&shieldd_sdk_proof_aggregation::AggregateBundle> {
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
                Action::NoteReshape(note_reshape) => {
                    note_reshape.body.outputs.iter().next().map(|output| {
                        (
                            output.note_payload.clone(),
                            output.ovk_wrapped_key.clone(),
                            output.wrapped_memo_key.clone(),
                            note_reshape.body.balance_commitment,
                        )
                    })
                }
                Action::ShieldedIcs20Withdrawal(withdrawal) => Some((
                    withdrawal.body.change_output.note_payload.clone(),
                    withdrawal.body.change_output.ovk_wrapped_key.clone(),
                    withdrawal.body.change_output.wrapped_memo_key.clone(),
                    withdrawal.body.balance_commitment,
                )),
                Action::ShieldedHostWithdrawal(withdrawal) => Some((
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
                Action::NoteReshape(note_reshape) => {
                    insert_payload_keys_for_outputs(
                        &mut result,
                        &note_reshape.body.outputs,
                        note_reshape.body.balance_commitment,
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
                Action::ShieldedHostWithdrawal(withdrawal) => {
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
                Action::IbcRelay(_)
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
                    | ActionView::NoteReshape(_)
                    | ActionView::ShieldedIcs20Withdrawal(_)
                    | ActionView::ShieldedHostWithdrawal(_)
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

        TransactionView {
            body_view: TransactionBodyView {
                action_views,
                transaction_parameters: self.transaction_parameters(),
                fee_funding,
                memo_view,
                nullifier_window: self.transaction_body.nullifier_window,
                historical_nullifier_proofs: self
                    .transaction_body
                    .historical_nullifier_proofs
                    .clone(),
            },
            binding_sig: self.binding_sig,
            anchor: self.anchor,
        }
    }

    pub fn actions(&self) -> impl Iterator<Item = &Action> {
        self.transaction_body.actions.iter()
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

    pub fn transfers(&self) -> impl Iterator<Item = &Transfer> {
        self.actions().filter_map(|action| {
            if let Action::Transfer(transfer) = action {
                Some(transfer)
            } else {
                None
            }
        })
    }

    pub fn note_reshapes(&self) -> impl Iterator<Item = &NoteReshape> {
        self.actions().filter_map(|action| {
            if let Action::NoteReshape(note_reshape) = action {
                Some(note_reshape)
            } else {
                None
            }
        })
    }

    pub fn shielded_host_withdrawals(&self) -> impl Iterator<Item = &ShieldedHostWithdrawal> {
        self.actions().filter_map(|action| {
            if let Action::ShieldedHostWithdrawal(withdrawal) = action {
                Some(withdrawal)
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
                    .map(|input| input.nullifier)
                    .collect(),
                Action::NoteReshape(note_reshape) => note_reshape
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
                Action::ShieldedHostWithdrawal(withdrawal) => withdrawal
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
                    .map(|input| input.nullifier),
            );
        }

        nullifiers.into_iter()
    }

    /// Counts every proof-bound spend without allocating the iterator's buffer.
    pub fn spent_nullifier_count(&self) -> usize {
        let body_count = self.actions().fold(0usize, |count, action| {
            let action_count = match action {
                Action::Transfer(transfer) => transfer.body.inputs.len(),
                Action::NoteReshape(note_reshape) => note_reshape.body.inputs.len(),
                Action::ShieldedIcs20Withdrawal(withdrawal) => withdrawal.body.inputs.len(),
                Action::ShieldedHostWithdrawal(withdrawal) => withdrawal.body.inputs.len(),
                Action::IbcRelay(_)
                | Action::ComplianceRegisterAsset(_)
                | Action::ComplianceRegisterUser(_)
                | Action::AggregateBundle(_) => 0,
            };
            count.saturating_add(action_count)
        });
        let fee_count = self
            .transaction_body
            .fee_funding
            .as_ref()
            .map(|fee_funding| fee_funding.transfer.body.inputs.len())
            .unwrap_or_default();
        body_count.saturating_add(fee_count)
    }

    pub fn state_commitments(&self) -> impl Iterator<Item = StateCommitment> + '_ {
        let mut commitments = self
            .actions()
            .flat_map(|action| match action {
                Action::Transfer(transfer) => transfer
                    .body
                    .outputs
                    .iter()
                    .map(|output| Some(output.note_payload.note_commitment))
                    .chain(std::iter::once(Some(
                        transfer.body.volume_accumulator.commitment,
                    )))
                    .collect::<Vec<_>>(),
                Action::NoteReshape(note_reshape) => note_reshape
                    .body
                    .outputs
                    .iter()
                    .map(|output| Some(output.note_payload.note_commitment))
                    .collect::<Vec<_>>(),
                Action::ShieldedIcs20Withdrawal(withdrawal) => vec![Some(
                    withdrawal.body.change_output.note_payload.note_commitment,
                )],
                Action::ShieldedHostWithdrawal(withdrawal) => vec![Some(
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
    balance_commitment: shieldd_sdk_asset::balance::Commitment,
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
    note_payload: &'a shieldd_sdk_shielded_pool::NotePayload,
    ovk_wrapped_key: &'a shieldd_sdk_keys::symmetric::OvkWrappedKey,
}

impl<'a> IntoOutputRef<'a> for &'a shieldd_sdk_shielded_pool::TransferOutputBody {
    fn into_output_ref(self) -> OutputRef<'a> {
        OutputRef {
            note_payload: &self.note_payload,
            ovk_wrapped_key: &self.ovk_wrapped_key,
        }
    }
}

impl<'a> IntoOutputRef<'a> for &'a shieldd_sdk_shielded_pool::NoteReshapeOutputBody {
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
        ActionView::NoteReshape(NoteReshapeView::Visible { payload_key, .. }) => Some(payload_key),
        ActionView::NoteReshape(NoteReshapeView::Opaque { .. }) => None,
        ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Visible {
            payload_key,
            ..
        }) => Some(payload_key),
        ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Opaque { .. }) => None,
        ActionView::ShieldedHostWithdrawal(ShieldedHostWithdrawalView::Visible {
            payload_key,
            ..
        }) => Some(payload_key),
        ActionView::ShieldedHostWithdrawal(ShieldedHostWithdrawalView::Opaque { .. }) => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use shieldd_sdk_asset::{asset, Balance, Value, BASE_ASSET_DENOM};
    use shieldd_sdk_compliance::WithdrawalComplianceCiphertext;
    use shieldd_sdk_keys::symmetric::{OvkWrappedKey, WrappedMemoKey};
    use shieldd_sdk_keys::Address;
    use shieldd_sdk_proto::DomainType as _;
    use shieldd_sdk_sct::Nullifier;
    use shieldd_sdk_shielded_pool::{
        backref::ENCRYPTED_BACKREF_LEN, note::NOTE_CIPHERTEXT_BYTES, RecoveryCapsule,
    };

    use super::{Action, Transaction, TransactionBody};

    fn recovery_capsule(seed: u64) -> RecoveryCapsule {
        RecoveryCapsule {
            epk: decaf377::Element::GENERATOR * decaf377::Fr::from(seed),
            c2: decaf377::Fq::from(seed + 1),
            salt: decaf377::Fq::from(seed + 2),
            key_confirmation: decaf377::Fq::from(seed + 3),
            encrypted_amount: decaf377::Fq::from(seed + 4),
            encrypted_note_blinding: decaf377::Fq::from(seed + 5),
        }
    }

    fn withdrawal_compliance_ciphertext(seed: u64) -> WithdrawalComplianceCiphertext {
        WithdrawalComplianceCiphertext {
            epk: decaf377::Element::GENERATOR * decaf377::Fr::from(seed),
            c2: decaf377::Fq::from(seed + 1),
            key_confirmation: decaf377::Fq::from(seed + 2),
            encrypted_sender_address: [0u8; 96],
        }
    }

    #[test]
    fn canonical_decode_accepts_exact_encoding_and_rejects_unknown_fields() {
        let tx = Transaction::default();
        let canonical: Vec<u8> = (&tx).into();
        assert_eq!(
            Transaction::decode_canonical(&canonical)
                .expect("canonical transaction must decode")
                .encode_to_vec(),
            canonical
        );

        // Unknown field 127, varint value 0. Prost accepts and drops it, so
        // decode followed by canonical re-encoding must reject this wire form.
        let mut noncanonical = canonical;
        noncanonical.extend_from_slice(&[0xf8, 0x07, 0x00]);
        let error = Transaction::decode_canonical(&noncanonical)
            .expect_err("unknown protobuf fields must not cross transaction ingress");
        assert!(
            error
                .to_string()
                .contains("not the canonical protobuf encoding"),
            "unexpected error: {error:#}"
        );
    }

    #[test]
    fn transfer_counts_as_nullifier_and_state_commitment_source() {
        let transfer = shieldd_sdk_shielded_pool::Transfer {
            body: shieldd_sdk_shielded_pool::TransferBody {
                anchor: shieldd_sdk_tct::Tree::default().root(),
                balance_commitment: Balance::from(Value {
                    amount: 9u64.into(),
                    asset_id: asset::Id(decaf377::Fq::from(1u64)),
                })
                .commit(decaf377::Fr::from(2u64)),
                inputs: vec![
                    shieldd_sdk_shielded_pool::TransferInputBody {
                        nullifier: Nullifier(decaf377::Fq::from(3u64)),
                        rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                            decaf377::Fr::from(4u64),
                        )),
                        encrypted_backref: shieldd_sdk_shielded_pool::EncryptedBackref::try_from(
                            [1u8; ENCRYPTED_BACKREF_LEN],
                        )
                        .expect("valid encrypted backref"),
                        compliance_ciphertext: vec![1, 2, 3],
                        history_required: false,
                    },
                    shieldd_sdk_shielded_pool::TransferInputBody {
                        nullifier: Nullifier(decaf377::Fq::from(30u64)),
                        rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                            decaf377::Fr::from(40u64),
                        )),
                        encrypted_backref: shieldd_sdk_shielded_pool::EncryptedBackref::try_from(
                            [2u8; 48],
                        )
                        .expect("fixed-size encrypted backref"),
                        compliance_ciphertext: vec![],
                        history_required: false,
                    },
                ],
                outputs: vec![
                    shieldd_sdk_shielded_pool::TransferOutputBody {
                        note_payload: shieldd_sdk_shielded_pool::NotePayload {
                            note_commitment: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(
                                5u64,
                            )),
                            ephemeral_key: decaf377_ka::Public([6u8; 32]),
                            encrypted_note: shieldd_sdk_shielded_pool::NoteCiphertext(
                                [7u8; NOTE_CIPHERTEXT_BYTES],
                            ),
                            recovery_capsule: Some(recovery_capsule(7)),
                        },
                        wrapped_memo_key: WrappedMemoKey([8u8; 48]),
                        ovk_wrapped_key: OvkWrappedKey([9u8; 48]),
                        compliance_ciphertext: vec![4, 5, 6],
                        compliance_metadata: vec![15, 16],
                    },
                    shieldd_sdk_shielded_pool::TransferOutputBody {
                        note_payload: shieldd_sdk_shielded_pool::NotePayload {
                            note_commitment: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(
                                50u64,
                            )),
                            ephemeral_key: decaf377_ka::Public([60u8; 32]),
                            encrypted_note: shieldd_sdk_shielded_pool::NoteCiphertext(
                                [70u8; NOTE_CIPHERTEXT_BYTES],
                            ),
                            recovery_capsule: Some(recovery_capsule(70)),
                        },
                        wrapped_memo_key: WrappedMemoKey([80u8; 48]),
                        ovk_wrapped_key: OvkWrappedKey([90u8; 48]),
                        compliance_ciphertext: vec![],
                        compliance_metadata: vec![],
                    },
                ],
                target_timestamp: 10,
                compliance_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(11u64)),
                asset_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(12u64)),
                routing: Default::default(),
                routing_parameter_set_id: decaf377::Fq::from(0u64),
                volume_accumulator:
                    shieldd_sdk_shielded_pool::VolumeAccumulatorPayload::canonical_fee_funding(),
                proof_context: shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
            },
            auth_sigs: vec![[17u8; 64].into(), [0u8; 64].into()],
            proof: shieldd_sdk_shielded_pool::TransferProof::default(),
        };
        assert_eq!(transfer.body.inputs.len(), 2);
        assert_eq!(transfer.body.outputs.len(), 2);

        let tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![Action::Transfer(transfer.clone())],
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(tx.spent_nullifiers().collect::<Vec<_>>().len(), 2);
        assert_eq!(
            tx.spent_nullifier_count(),
            tx.spent_nullifiers().count(),
            "zero-allocation count must match the canonical iterator"
        );
        assert_eq!(tx.state_commitments().collect::<Vec<_>>().len(), 3);

        assert!(
            tx.transaction_body.validate_nullifier_history().is_err(),
            "every proof-bearing transaction must bind a nullifier window"
        );
        let mut history_tx = tx.clone();
        history_tx.transaction_body.nullifier_window =
            Some(shieldd_sdk_sct::nullifier_generation::NullifierWindow {
                protocol_version: shieldd_sdk_sct::nullifier_generation::PROTOCOL_VERSION,
                current_generation: 0,
                recent_position_floor: 0,
                archived_generation_count: 0,
                archived_history_head: shieldd_sdk_sct::nullifier_generation::empty_history_head(),
            });
        history_tx
            .transaction_body
            .validate_nullifier_history()
            .expect("recent inputs require the window but no historical bundles");
        let Action::Transfer(history_transfer) = &mut history_tx.transaction_body.actions[0] else {
            panic!("fixture action must remain a Transfer")
        };
        history_transfer.body.inputs[0].history_required = true;
        assert!(
            history_tx
                .transaction_body
                .validate_nullifier_history()
                .is_err(),
            "an old input must have exactly one matching historical bundle"
        );

        let fee_funded_tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![Action::Transfer(transfer.clone())],
                fee_funding: Some(crate::FeeFunding { transfer }),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            fee_funded_tx.spent_nullifiers().collect::<Vec<_>>().len(),
            4
        );
        assert_eq!(
            fee_funded_tx.spent_nullifier_count(),
            fee_funded_tx.spent_nullifiers().count(),
            "body and fee-funding nullifiers must share one canonical count"
        );
        assert_eq!(
            fee_funded_tx.state_commitments().collect::<Vec<_>>().len(),
            5
        );
    }

    #[test]
    fn note_reshape_fixed_slots_do_not_filter_spent_nullifiers() {
        let inputs = (0..8)
            .map(|index| shieldd_sdk_shielded_pool::NoteReshapeInputBody {
                nullifier: Nullifier(decaf377::Fq::from(100u64 + index)),
                rk: VerificationKey::from(SigningKey::<SpendAuth>::from(decaf377::Fr::from(
                    200u64 + index,
                ))),
                encrypted_backref: shieldd_sdk_shielded_pool::EncryptedBackref::try_from(
                    [u8::try_from(index + 1).expect("small test index"); 48],
                )
                .expect("fixed-size encrypted backref"),
                history_required: false,
            })
            .collect::<Vec<_>>();
        assert!(inputs
            .iter()
            .all(|input| input.encrypted_backref.len() == 48));

        let note_reshape = shieldd_sdk_shielded_pool::NoteReshape {
            body: shieldd_sdk_shielded_pool::NoteReshapeBody {
                family_id: shieldd_sdk_shielded_pool::NoteReshapeFamilyId::EightByOne,
                anchor: shieldd_sdk_tct::Tree::default().root(),
                balance_commitment: Balance::default().commit(decaf377::Fr::from(1u64)),
                inputs,
                outputs: vec![shieldd_sdk_shielded_pool::NoteReshapeOutputBody {
                    note_payload: shieldd_sdk_shielded_pool::NotePayload {
                        note_commitment: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(
                            300u64,
                        )),
                        ephemeral_key: decaf377_ka::Public([3u8; 32]),
                        encrypted_note: shieldd_sdk_shielded_pool::NoteCiphertext(
                            [4u8; NOTE_CIPHERTEXT_BYTES],
                        ),
                        recovery_capsule: Some(recovery_capsule(4)),
                    },
                    wrapped_memo_key: WrappedMemoKey([5u8; 48]),
                    ovk_wrapped_key: OvkWrappedKey([6u8; 48]),
                }],
                routing_tag: Default::default(),
                routing_parameter_set_id: decaf377::Fq::from(0u64),
                asset_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(0u64)),
                compliance_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(0u64)),
            },
            auth_sigs: vec![[0u8; 64].into(); 8],
            proof: shieldd_sdk_shielded_pool::NoteReshapeProof::default(),
        };
        let tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![Action::NoteReshape(note_reshape)],
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(tx.spent_nullifiers().collect::<Vec<_>>().len(), 8);
        assert_eq!(tx.spent_nullifier_count(), tx.spent_nullifiers().count());
    }

    #[test]
    fn compliance_scanner_transaction_id_matches_canonical_transaction_id() {
        let tx = Transaction::default();
        let proto: shieldd_sdk_proto::core::transaction::v1::Transaction = (&tx).into();
        assert_eq!(
            shieldd_sdk_compliance::scanner_transaction_id_from_proto(&proto),
            tx.id()
        );
    }

    #[test]
    fn proof_and_nullifier_counts_cover_mixed_shielded_families() {
        let tx = Transaction {
            transaction_body: TransactionBody {
                actions: vec![
                    Action::NoteReshape(shieldd_sdk_shielded_pool::NoteReshape {
                        body: shieldd_sdk_shielded_pool::NoteReshapeBody {
                            family_id: shieldd_sdk_shielded_pool::NoteReshapeFamilyId::EightByOne,
                            anchor: shieldd_sdk_tct::Tree::default().root(),
                            balance_commitment: Balance::default().commit(decaf377::Fr::from(1u64)),
                            inputs: vec![
                                shieldd_sdk_shielded_pool::NoteReshapeInputBody {
                                    nullifier: Nullifier(decaf377::Fq::from(2u64)),
                                    rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                        decaf377::Fr::from(3u64),
                                    )),
                                    encrypted_backref:
                                        shieldd_sdk_shielded_pool::EncryptedBackref::dummy(),
                                    history_required: false,
                                },
                                shieldd_sdk_shielded_pool::NoteReshapeInputBody {
                                    nullifier: Nullifier(decaf377::Fq::from(4u64)),
                                    rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                        decaf377::Fr::from(5u64),
                                    )),
                                    encrypted_backref:
                                        shieldd_sdk_shielded_pool::EncryptedBackref::dummy(),
                                    history_required: false,
                                },
                            ],
                            outputs: vec![shieldd_sdk_shielded_pool::NoteReshapeOutputBody {
                                note_payload: shieldd_sdk_shielded_pool::NotePayload {
                                    note_commitment: shieldd_sdk_tct::StateCommitment(
                                        decaf377::Fq::from(6u64),
                                    ),
                                    ephemeral_key: decaf377_ka::Public([7u8; 32]),
                                    encrypted_note:
                                        shieldd_sdk_shielded_pool::NoteCiphertext(
                                            [8u8; NOTE_CIPHERTEXT_BYTES],
                                        ),
                                    recovery_capsule: Some(recovery_capsule(8)),
                                },
                                wrapped_memo_key: WrappedMemoKey([9u8; 48]),
                                ovk_wrapped_key: OvkWrappedKey([10u8; 48]),
                            }],
                            routing_tag: Default::default(),
                            routing_parameter_set_id: decaf377::Fq::from(0u64),
                            asset_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(0u64)),
                            compliance_anchor: shieldd_sdk_tct::StateCommitment(
                                decaf377::Fq::from(0u64),
                            ),
                        },
                        auth_sigs: vec![[11u8; 64].into(), [12u8; 64].into()],
                        proof: shieldd_sdk_shielded_pool::NoteReshapeProof::default(),
                    }),
                    Action::NoteReshape(shieldd_sdk_shielded_pool::NoteReshape {
                        body: shieldd_sdk_shielded_pool::NoteReshapeBody {
                            family_id: shieldd_sdk_shielded_pool::NoteReshapeFamilyId::OneByEight,
                            anchor: shieldd_sdk_tct::Tree::default().root(),
                            balance_commitment: Balance::default().commit(decaf377::Fr::from(13u64)),
                            inputs: vec![shieldd_sdk_shielded_pool::NoteReshapeInputBody {
                                nullifier: Nullifier(decaf377::Fq::from(14u64)),
                                rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                    decaf377::Fr::from(15u64),
                                )),
                                encrypted_backref:
                                    shieldd_sdk_shielded_pool::EncryptedBackref::dummy(),
                                history_required: false,
                            }],
                            outputs: vec![
                                shieldd_sdk_shielded_pool::NoteReshapeOutputBody {
                                    note_payload: shieldd_sdk_shielded_pool::NotePayload {
                                        note_commitment: shieldd_sdk_tct::StateCommitment(
                                            decaf377::Fq::from(16u64),
                                        ),
                                        ephemeral_key: decaf377_ka::Public([17u8; 32]),
                                        encrypted_note: shieldd_sdk_shielded_pool::NoteCiphertext(
                                            [18u8; NOTE_CIPHERTEXT_BYTES],
                                        ),
                                        recovery_capsule: Some(recovery_capsule(18)),
                                    },
                                    wrapped_memo_key: WrappedMemoKey([19u8; 48]),
                                    ovk_wrapped_key: OvkWrappedKey([20u8; 48]),
                                };
                                4
                            ],
                            routing_tag: Default::default(),
                            routing_parameter_set_id: decaf377::Fq::from(0u64),
                            asset_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(0u64)),
                            compliance_anchor: shieldd_sdk_tct::StateCommitment(
                                decaf377::Fq::from(0u64),
                            ),
                        },
                        auth_sigs: vec![[21u8; 64].into()],
                        proof: shieldd_sdk_shielded_pool::NoteReshapeProof::default(),
                    }),
                    Action::ShieldedIcs20Withdrawal(
                        shieldd_sdk_shielded_pool::ShieldedIcs20Withdrawal {
                            body: shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalBody {
                                family_id:
                                    shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId::Canonical,
                                anchor: shieldd_sdk_tct::Tree::default().root(),
                                balance_commitment: Balance::default().commit(decaf377::Fr::from(22u64)),
                                inputs: vec![
                                    shieldd_sdk_shielded_pool::TransferInputBody {
                                        nullifier: Nullifier(decaf377::Fq::from(23u64)),
                                        rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                            decaf377::Fr::from(24u64),
                                        )),
                                        encrypted_backref:
                                            shieldd_sdk_shielded_pool::EncryptedBackref::try_from(
                                                [23u8; 48],
                                            )
                                            .expect("fixed-size encrypted backref"),
                                        compliance_ciphertext: vec![],
                                        history_required: false,
                                    },
                                    shieldd_sdk_shielded_pool::TransferInputBody {
                                        nullifier: Nullifier(decaf377::Fq::from(25u64)),
                                        rk: VerificationKey::from(SigningKey::<SpendAuth>::from(
                                            decaf377::Fr::from(26u64),
                                        )),
                                        encrypted_backref:
                                            shieldd_sdk_shielded_pool::EncryptedBackref::try_from(
                                                [25u8; 48],
                                            )
                                            .expect("fixed-size encrypted backref"),
                                        compliance_ciphertext: vec![],
                                        history_required: false,
                                    },
                                ],
                                withdrawal: shieldd_sdk_shielded_pool::Ics20Withdrawal {
                                    amount: 1u64.into(),
                                    denom: BASE_ASSET_DENOM.clone(),
                                    destination_chain_address: "cosmos1deadbeef".to_string(),
                                    return_address: Address::dummy(&mut rand_core::OsRng),
                                    timeout_height: ibc_types::core::client::Height::new(0, 10)
                                        .expect("valid timeout height"),
                                    timeout_time: 1,
                                    source_channel: ibc_types::core::channel::ChannelId::new(7),
                                    ics20_memo: String::new(),
                                    use_transparent_address: false,
                                },
                                change_output:
                                    shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalChangeBody {
                                        note_payload: shieldd_sdk_shielded_pool::NotePayload {
                                            note_commitment: shieldd_sdk_tct::StateCommitment(
                                                decaf377::Fq::from(27u64),
                                            ),
                                            ephemeral_key: decaf377_ka::Public([28u8; 32]),
                                            encrypted_note:
                                                shieldd_sdk_shielded_pool::NoteCiphertext(
                                                    [29u8; NOTE_CIPHERTEXT_BYTES],
                                                ),
                                            recovery_capsule: Some(recovery_capsule(29)),
                                        },
                                        wrapped_memo_key: WrappedMemoKey([30u8; 48]),
                                        ovk_wrapped_key: OvkWrappedKey([31u8; 48]),
                                    },
                                target_timestamp: 0,
                                compliance_anchor: shieldd_sdk_tct::StateCommitment(
                                    decaf377::Fq::from(32u64),
                                ),
                                asset_anchor: shieldd_sdk_tct::StateCommitment(decaf377::Fq::from(33u64)),
                                routing_tag: Default::default(),
                                routing_parameter_set_id: decaf377::Fq::from(0u64),
                                withdrawal_compliance_ciphertext:
                                    withdrawal_compliance_ciphertext(36),
                                volume_accumulator:
                                    shieldd_sdk_shielded_pool::VolumeAccumulatorPayload::canonical_fee_funding(),
                            },
                            auth_sigs: vec![[34u8; 64].into(), [35u8; 64].into()],
                            proof: shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalProof::default(),
                        },
                    ),
                ],
                ..Default::default()
            },
            ..Default::default()
        };

        assert_eq!(tx.num_proofs(), 3);
        assert_eq!(
            tx.spent_nullifiers().collect::<Vec<_>>(),
            [
                Nullifier(decaf377::Fq::from(2u64)),
                Nullifier(decaf377::Fq::from(4u64)),
                Nullifier(decaf377::Fq::from(14u64)),
                Nullifier(decaf377::Fq::from(23u64)),
                Nullifier(decaf377::Fq::from(25u64)),
            ],
            "transaction-wide duplicate detection must see every fixed-slot \
             NoteReshape and shielded-withdrawal nullifier"
        );
        assert_eq!(
            tx.spent_nullifier_count(),
            tx.spent_nullifiers().count(),
            "mixed-family count must match the canonical iterator"
        );
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
            memo: msg.memo.map(Into::into),
            nullifier_window: msg.nullifier_window.map(Into::into),
            historical_nullifier_proofs: msg
                .historical_nullifier_proofs
                .into_iter()
                .map(Into::into)
                .collect(),
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

        let body = TransactionBody {
            actions,
            transaction_parameters,
            fee_funding,
            memo,
            nullifier_window: proto
                .nullifier_window
                .map(TryInto::try_into)
                .transpose()
                .context("nullifier window malformed")?,
            historical_nullifier_proofs: proto
                .historical_nullifier_proofs
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        };
        body.validate_nullifier_history()?;
        Ok(body)
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
        Self::decode_canonical(bytes)
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
