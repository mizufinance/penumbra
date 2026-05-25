use anyhow::Context;
use decaf377_rdsa::{Binding, Signature};
use penumbra_sdk_asset::Balance;
use penumbra_sdk_keys::AddressView;
use penumbra_sdk_proto::{core::transaction::v1 as pbt, DomainType};
use penumbra_sdk_shielded_pool::{
    ConsolidateView, ShieldedIcs20WithdrawalView, SplitView, TransferView,
};
use serde::{Deserialize, Serialize};

pub mod action_view;
mod transaction_perspective;

pub use action_view::ActionView;
use penumbra_sdk_tct as tct;
pub use transaction_perspective::TransactionPerspective;

use crate::{
    fee_funding::FeeFunding,
    memo::MemoCiphertext,
    transaction::{TransactionEffect, TransactionSummary},
    Action, DetectionData, Transaction, TransactionBody, TransactionParameters,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pbt::TransactionView", into = "pbt::TransactionView")]
pub struct TransactionView {
    pub body_view: TransactionBodyView,
    pub binding_sig: Signature<Binding>,
    pub anchor: tct::Root,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    try_from = "pbt::TransactionBodyView",
    into = "pbt::TransactionBodyView"
)]
pub struct TransactionBodyView {
    pub action_views: Vec<ActionView>,
    pub transaction_parameters: TransactionParameters,
    pub fee_funding: Option<TransferView>,
    pub detection_data: Option<DetectionData>,
    pub memo_view: Option<MemoView>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pbt::MemoView", into = "pbt::MemoView")]
#[allow(clippy::large_enum_variant)]
pub enum MemoView {
    Visible {
        plaintext: MemoPlaintextView,
        ciphertext: MemoCiphertext,
    },
    Opaque {
        ciphertext: MemoCiphertext,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pbt::MemoPlaintextView", into = "pbt::MemoPlaintextView")]
pub struct MemoPlaintextView {
    pub return_address: AddressView,
    pub text: String,
}

impl TransactionView {
    pub fn transaction(&self) -> Transaction {
        let actions = self
            .body_view
            .action_views
            .iter()
            .cloned()
            .map(Action::from)
            .collect();

        let memo_ciphertext = match &self.body_view.memo_view {
            Some(MemoView::Visible { ciphertext, .. }) => Some(ciphertext),
            Some(MemoView::Opaque { ciphertext }) => Some(ciphertext),
            None => None,
        };

        Transaction {
            transaction_body: TransactionBody {
                actions,
                transaction_parameters: self.body_view.transaction_parameters.clone(),
                fee_funding: self.body_view.fee_funding.clone().map(|view| FeeFunding {
                    transfer: match view {
                        TransferView::Visible { transfer, .. } => transfer,
                        TransferView::Opaque { transfer } => transfer,
                    },
                }),
                detection_data: self.body_view.detection_data.clone(),
                memo: memo_ciphertext.cloned(),
            },
            binding_sig: self.binding_sig,
            anchor: self.anchor,
        }
    }

    pub fn action_views(&self) -> impl Iterator<Item = &ActionView> {
        self.body_view.action_views.iter()
    }

    fn accumulate_effects(summary: TransactionSummary) -> TransactionSummary {
        use std::collections::BTreeMap;
        let mut keyed_effects: BTreeMap<AddressView, Balance> = BTreeMap::new();
        for effect in summary.effects {
            *keyed_effects.entry(effect.address).or_default() += effect.balance;
        }
        TransactionSummary {
            effects: keyed_effects
                .into_iter()
                .map(|(address, balance)| TransactionEffect { address, balance })
                .collect(),
        }
    }

    /// Produces a summary from visible note-spending and note-creating actions.
    pub fn summary(&self) -> TransactionSummary {
        let mut effects = Vec::new();

        for action_view in &self.body_view.action_views {
            match action_view {
                ActionView::Transfer(transfer_view) => summarize_note_flow(
                    transfer_view,
                    |effects, address, balance| {
                        effects.push(TransactionEffect { address, balance })
                    },
                    &mut effects,
                ),
                ActionView::Consolidate(consolidate_view) => summarize_note_flow(
                    consolidate_view,
                    |effects, address, balance| {
                        effects.push(TransactionEffect { address, balance })
                    },
                    &mut effects,
                ),
                ActionView::Split(split_view) => summarize_note_flow(
                    split_view,
                    |effects, address, balance| {
                        effects.push(TransactionEffect { address, balance })
                    },
                    &mut effects,
                ),
                ActionView::ShieldedIcs20Withdrawal(withdrawal_view) => summarize_note_flow(
                    withdrawal_view,
                    |effects, address, balance| {
                        effects.push(TransactionEffect { address, balance })
                    },
                    &mut effects,
                ),
                _ => {}
            }
        }

        Self::accumulate_effects(TransactionSummary { effects })
    }
}

trait NoteFlowView {
    fn spent_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]>;
    fn created_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]>;
}

impl NoteFlowView for TransferView {
    fn spent_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]> {
        match self {
            TransferView::Visible { spent_notes, .. } => Some(spent_notes),
            TransferView::Opaque { .. } => None,
        }
    }

    fn created_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]> {
        match self {
            TransferView::Visible { created_notes, .. } => Some(created_notes),
            TransferView::Opaque { .. } => None,
        }
    }
}

impl NoteFlowView for ConsolidateView {
    fn spent_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]> {
        match self {
            ConsolidateView::Visible { spent_notes, .. } => Some(spent_notes),
            ConsolidateView::Opaque { .. } => None,
        }
    }

    fn created_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]> {
        match self {
            ConsolidateView::Visible { created_notes, .. } => Some(created_notes),
            ConsolidateView::Opaque { .. } => None,
        }
    }
}

impl NoteFlowView for SplitView {
    fn spent_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]> {
        match self {
            SplitView::Visible { spent_notes, .. } => Some(spent_notes),
            SplitView::Opaque { .. } => None,
        }
    }

    fn created_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]> {
        match self {
            SplitView::Visible { created_notes, .. } => Some(created_notes),
            SplitView::Opaque { .. } => None,
        }
    }
}

impl NoteFlowView for ShieldedIcs20WithdrawalView {
    fn spent_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]> {
        match self {
            ShieldedIcs20WithdrawalView::Visible { spent_notes, .. } => Some(spent_notes),
            ShieldedIcs20WithdrawalView::Opaque { .. } => None,
        }
    }

    fn created_notes(&self) -> Option<&[penumbra_sdk_shielded_pool::NoteView]> {
        match self {
            ShieldedIcs20WithdrawalView::Visible { change_note, .. } => {
                Some(std::slice::from_ref(change_note))
            }
            ShieldedIcs20WithdrawalView::Opaque { .. } => None,
        }
    }
}

fn summarize_note_flow<V: NoteFlowView>(
    view: &V,
    mut push: impl FnMut(&mut Vec<TransactionEffect>, AddressView, Balance),
    effects: &mut Vec<TransactionEffect>,
) {
    if let Some(spent_notes) = view.spent_notes() {
        for spent_note in spent_notes {
            push(
                effects,
                spent_note.address.clone(),
                Balance::from(spent_note.value.value()),
            );
        }
    }

    if let Some(created_notes) = view.created_notes() {
        for created_note in created_notes {
            push(
                effects,
                created_note.address.clone(),
                -Balance::from(created_note.value.value()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use decaf377::{Fq, Fr};
    use decaf377_rdsa::{SigningKey, SpendAuth};
    use ibc_types::{core::channel::ChannelId, core::client::Height};
    use penumbra_sdk_asset::{Balance, Value, ValueView, BASE_ASSET_DENOM, BASE_ASSET_ID};
    use penumbra_sdk_keys::{
        symmetric::{OvkWrappedKey, PayloadKey, WrappedMemoKey},
        AddressView,
    };
    use penumbra_sdk_num::Amount;
    use penumbra_sdk_sct::Nullifier;
    use penumbra_sdk_shielded_pool::{
        EncryptedBackref, Ics20Withdrawal, Note, NotePayload, NoteView, Rseed,
        ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalBody, ShieldedIcs20WithdrawalChangeBody,
        ShieldedIcs20WithdrawalProof, ShieldedIcs20WithdrawalView, TransferInputBody,
    };
    use penumbra_sdk_tct::{StateCommitment, Tree};

    use super::{ActionView, TransactionBodyView, TransactionView};

    fn note_view(note: &Note) -> NoteView {
        NoteView {
            value: ValueView::UnknownAssetId {
                amount: note.amount(),
                asset_id: note.asset_id(),
            },
            rseed: note.rseed(),
            address: AddressView::Opaque {
                address: note.address(),
            },
        }
    }

    #[test]
    fn summary_includes_visible_shielded_ics20_withdrawal_note_flow() {
        let spent_note = Note::from_parts(
            penumbra_sdk_keys::test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(10u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed([1u8; 32]),
        )
        .expect("valid spent note");
        let change_note = Note::from_parts(
            penumbra_sdk_keys::test_keys::ADDRESS_0.clone(),
            Value {
                amount: Amount::from(3u64),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed([2u8; 32]),
        )
        .expect("valid change note");

        let txv = TransactionView {
            body_view: TransactionBodyView {
                action_views: vec![ActionView::ShieldedIcs20Withdrawal(
                    ShieldedIcs20WithdrawalView::Visible {
                        withdrawal: ShieldedIcs20Withdrawal {
                            body: ShieldedIcs20WithdrawalBody {
                                family_id: penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalFamilyId::Canonical,
                                anchor: Tree::default().root(),
                                balance_commitment: Balance::default().commit(Fr::from(1u64)),
                                inputs: vec![TransferInputBody {
                                    nullifier: Nullifier(Fq::from(1u64)),
                                    rk: decaf377_rdsa::VerificationKey::from(
                                        SigningKey::<SpendAuth>::from(Fr::from(2u64)),
                                    ),
                                    encrypted_backref: EncryptedBackref::dummy(),
                                    compliance_ciphertext: Vec::new(),
                                }],
                                withdrawal: Ics20Withdrawal {
                                    destination_chain_address: "cosmos1destination".to_string(),
                                    denom: BASE_ASSET_DENOM.clone(),
                                    amount: Amount::from(7u64),
                                    timeout_height: Height::new(1, 10).expect("valid height"),
                                    timeout_time: 10,
                                    return_address: penumbra_sdk_keys::test_keys::ADDRESS_0.clone(),
                                    source_channel: "channel-0".parse::<ChannelId>().expect("valid channel"),
                                    use_compat_address: false,
                                    ics20_memo: String::new(),
                                    use_transparent_address: false,
                                },
                                change_output: ShieldedIcs20WithdrawalChangeBody {
                                    note_payload: NotePayload {
                                        note_commitment: StateCommitment(Fq::from(3u64)),
                                        ephemeral_key: change_note.ephemeral_public_key(),
                                        encrypted_note: change_note.encrypt(),
                                    },
                                    wrapped_memo_key: WrappedMemoKey([0u8; 48]),
                                    ovk_wrapped_key: OvkWrappedKey([0u8; 48]),
                                },
                                target_timestamp: 0,
                                compliance_anchor: StateCommitment(Fq::from(4u64)),
                                asset_anchor: StateCommitment(Fq::from(5u64)),
                            },
                            auth_sigs: vec![[0u8; 64].into()],
                            proof: ShieldedIcs20WithdrawalProof::default(),
                        },
                        spent_notes: vec![note_view(&spent_note)],
                        change_note: note_view(&change_note),
                        payload_key: PayloadKey::from([0u8; 32]),
                    },
                )],
                transaction_parameters: Default::default(),
                fee_funding: None,
                detection_data: None,
                memo_view: None,
            },
            binding_sig: [0u8; 64].into(),
            anchor: Tree::default().root(),
        };

        let summary = txv.summary();
        assert_eq!(summary.effects.len(), 1);
        assert_eq!(
            summary.effects[0].balance,
            Balance::from(spent_note.value()) - Balance::from(change_note.value())
        );
    }
}

impl DomainType for TransactionView {
    type Proto = pbt::TransactionView;
}

impl TryFrom<pbt::TransactionView> for TransactionView {
    type Error = anyhow::Error;

    fn try_from(v: pbt::TransactionView) -> Result<Self, Self::Error> {
        let binding_sig = v
            .binding_sig
            .ok_or_else(|| anyhow::anyhow!("transaction view missing binding signature"))?
            .try_into()
            .context("transaction binding signature malformed")?;

        let anchor = v
            .anchor
            .ok_or_else(|| anyhow::anyhow!("transaction view missing anchor"))?
            .try_into()
            .context("transaction anchor malformed")?;

        let body_view = v
            .body_view
            .ok_or_else(|| anyhow::anyhow!("transaction view missing body"))?
            .try_into()
            .context("transaction body malformed")?;

        Ok(Self {
            body_view,
            binding_sig,
            anchor,
        })
    }
}

impl TryFrom<pbt::TransactionBodyView> for TransactionBodyView {
    type Error = anyhow::Error;

    fn try_from(body_view: pbt::TransactionBodyView) -> Result<Self, Self::Error> {
        let action_views = body_view
            .action_views
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;

        let memo_view: Option<MemoView> = match body_view.memo_view {
            Some(mv) => match mv.memo_view {
                Some(x) => match x {
                    pbt::memo_view::MemoView::Visible(v) => Some(MemoView::Visible {
                        plaintext: v
                            .plaintext
                            .ok_or_else(|| {
                                anyhow::anyhow!("transaction view memo missing memo plaintext")
                            })?
                            .try_into()?,
                        ciphertext: v
                            .ciphertext
                            .ok_or_else(|| {
                                anyhow::anyhow!("transaction view memo missing memo ciphertext")
                            })?
                            .try_into()?,
                    }),
                    pbt::memo_view::MemoView::Opaque(v) => Some(MemoView::Opaque {
                        ciphertext: v
                            .ciphertext
                            .ok_or_else(|| {
                                anyhow::anyhow!("transaction view memo missing memo ciphertext")
                            })?
                            .try_into()?,
                    }),
                },
                None => None,
            },
            None => None,
        };

        let transaction_parameters = body_view
            .transaction_parameters
            .ok_or_else(|| anyhow::anyhow!("transaction view missing transaction parameters view"))?
            .try_into()?;
        let fee_funding = body_view.fee_funding.map(TryInto::try_into).transpose()?;

        let fmd_clues = body_view
            .detection_data
            .map(|dd| {
                dd.fmd_clues
                    .into_iter()
                    .map(|fmd| fmd.try_into())
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;

        let detection_data = fmd_clues.map(|fmd_clues| DetectionData { fmd_clues });

        Ok(TransactionBodyView {
            action_views,
            transaction_parameters,
            fee_funding,
            detection_data,
            memo_view,
        })
    }
}

impl From<TransactionView> for pbt::TransactionView {
    fn from(v: TransactionView) -> Self {
        Self {
            body_view: Some(v.body_view.into()),
            anchor: Some(v.anchor.into()),
            binding_sig: Some(v.binding_sig.into()),
        }
    }
}

impl From<TransactionBodyView> for pbt::TransactionBodyView {
    fn from(v: TransactionBodyView) -> Self {
        Self {
            action_views: v.action_views.into_iter().map(Into::into).collect(),
            transaction_parameters: Some(v.transaction_parameters.into()),
            fee_funding: v.fee_funding.map(Into::into),
            detection_data: v.detection_data.map(Into::into),
            memo_view: v.memo_view.map(Into::into),
        }
    }
}

impl From<MemoView> for pbt::MemoView {
    fn from(v: MemoView) -> Self {
        Self {
            memo_view: match v {
                MemoView::Visible {
                    plaintext,
                    ciphertext,
                } => Some(pbt::memo_view::MemoView::Visible(pbt::memo_view::Visible {
                    plaintext: Some(plaintext.into()),
                    ciphertext: Some(ciphertext.into()),
                })),
                MemoView::Opaque { ciphertext } => {
                    Some(pbt::memo_view::MemoView::Opaque(pbt::memo_view::Opaque {
                        ciphertext: Some(ciphertext.into()),
                    }))
                }
            },
        }
    }
}

impl TryFrom<pbt::MemoView> for MemoView {
    type Error = anyhow::Error;

    fn try_from(v: pbt::MemoView) -> Result<Self, Self::Error> {
        match v
            .memo_view
            .ok_or_else(|| anyhow::anyhow!("missing memo field"))?
        {
            pbt::memo_view::MemoView::Visible(x) => Ok(MemoView::Visible {
                plaintext: x
                    .plaintext
                    .ok_or_else(|| anyhow::anyhow!("missing plaintext field"))?
                    .try_into()?,
                ciphertext: x
                    .ciphertext
                    .ok_or_else(|| anyhow::anyhow!("missing ciphertext field"))?
                    .try_into()?,
            }),
            pbt::memo_view::MemoView::Opaque(x) => Ok(MemoView::Opaque {
                ciphertext: x
                    .ciphertext
                    .ok_or_else(|| anyhow::anyhow!("missing ciphertext field"))?
                    .try_into()?,
            }),
        }
    }
}

impl From<MemoPlaintextView> for pbt::MemoPlaintextView {
    fn from(v: MemoPlaintextView) -> Self {
        Self {
            return_address: Some(v.return_address.into()),
            text: v.text,
        }
    }
}

impl TryFrom<pbt::MemoPlaintextView> for MemoPlaintextView {
    type Error = anyhow::Error;

    fn try_from(v: pbt::MemoPlaintextView) -> Result<Self, Self::Error> {
        let sender: AddressView = v
            .return_address
            .ok_or_else(|| anyhow::anyhow!("memo plan missing memo plaintext"))?
            .try_into()
            .context("return address malformed")?;

        Ok(Self {
            return_address: sender,
            text: v.text,
        })
    }
}
