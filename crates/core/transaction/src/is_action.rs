use penumbra_sdk_asset::balance;
use penumbra_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use penumbra_sdk_governance::{ProposalSubmit, ValidatorVote};
use penumbra_sdk_ibc::IbcRelay;
use penumbra_sdk_proof_aggregation::AggregateBundle;
use penumbra_sdk_shielded_pool::{
    Consolidate, ConsolidateView, Note, ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalView,
    Split, SplitView, Transfer, TransferView,
};

use crate::{ActionView, TransactionPerspective};

/// Common behavior between Penumbra actions.
pub trait IsAction {
    fn balance_commitment(&self) -> balance::Commitment;
    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView;
}

impl IsAction for ProposalSubmit {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(decaf377::Fr::from(0u64))
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ProposalSubmit(self.to_owned())
    }
}

impl IsAction for ValidatorVote {
    fn balance_commitment(&self) -> balance::Commitment {
        Default::default()
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ValidatorVote(self.to_owned())
    }
}

impl IsAction for Transfer {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let Some(first_output) = self.body.outputs.first() else {
            return ActionView::Transfer(TransferView::Opaque {
                transfer: self.to_owned(),
            });
        };
        let Some(payload_key) = txp
            .payload_keys
            .get(&first_output.note_payload.note_commitment)
        else {
            return ActionView::Transfer(TransferView::Opaque {
                transfer: self.to_owned(),
            });
        };
        let spent_notes = self
            .body
            .inputs
            .iter()
            .filter_map(|input| txp.spend_nullifiers.get(&input.nullifier).cloned())
            .collect::<Vec<_>>();
        if spent_notes.is_empty() {
            return ActionView::Transfer(TransferView::Opaque {
                transfer: self.to_owned(),
            });
        }

        let sender_address = spent_notes[0].address();
        let mut created_notes = Vec::with_capacity(self.body.outputs.len());
        for output in &self.body.outputs {
            let Some(output_payload_key) =
                txp.payload_keys.get(&output.note_payload.note_commitment)
            else {
                return ActionView::Transfer(TransferView::Opaque {
                    transfer: self.to_owned(),
                });
            };
            let Ok(note) = Note::decrypt_with_payload_key(
                &output.note_payload.encrypted_note,
                output_payload_key,
                &output.note_payload.ephemeral_key,
            ) else {
                return ActionView::Transfer(TransferView::Opaque {
                    transfer: self.to_owned(),
                });
            };
            if note.amount() == penumbra_sdk_num::Amount::zero() && note.address() == sender_address
            {
                continue;
            }
            created_notes.push(note);
        }

        match first_output.wrapped_memo_key.decrypt_outgoing(payload_key) {
            Ok(decrypted_memo_key) => ActionView::Transfer(TransferView::Visible {
                transfer: self.to_owned(),
                spent_notes: spent_notes
                    .into_iter()
                    .map(|note| txp.view_note(note))
                    .collect(),
                created_notes: created_notes
                    .into_iter()
                    .map(|note| txp.view_note(note))
                    .collect(),
                payload_key: decrypted_memo_key,
            }),
            Err(_) => ActionView::Transfer(TransferView::Opaque {
                transfer: self.to_owned(),
            }),
        }
    }
}

impl IsAction for Consolidate {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let Some(first_output) = self.body.outputs.first() else {
            return ActionView::Consolidate(ConsolidateView::Opaque {
                consolidate: self.to_owned(),
            });
        };
        let Some(payload_key) = txp
            .payload_keys
            .get(&first_output.note_payload.note_commitment)
        else {
            return ActionView::Consolidate(ConsolidateView::Opaque {
                consolidate: self.to_owned(),
            });
        };

        let Some(spent_notes) = self
            .body
            .inputs
            .iter()
            .map(|input| txp.spend_nullifiers.get(&input.nullifier).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            return ActionView::Consolidate(ConsolidateView::Opaque {
                consolidate: self.to_owned(),
            });
        };

        let Some(created_notes) = self
            .body
            .outputs
            .iter()
            .map(|output| {
                txp.payload_keys
                    .get(&output.note_payload.note_commitment)
                    .and_then(|output_payload_key| {
                        Note::decrypt_with_payload_key(
                            &output.note_payload.encrypted_note,
                            output_payload_key,
                            &output.note_payload.ephemeral_key,
                        )
                        .ok()
                    })
            })
            .collect::<Option<Vec<_>>>()
        else {
            return ActionView::Consolidate(ConsolidateView::Opaque {
                consolidate: self.to_owned(),
            });
        };

        match first_output.wrapped_memo_key.decrypt_outgoing(payload_key) {
            Ok(decrypted_memo_key) => ActionView::Consolidate(ConsolidateView::Visible {
                consolidate: self.to_owned(),
                spent_notes: spent_notes
                    .into_iter()
                    .map(|note| txp.view_note(note))
                    .collect(),
                created_notes: created_notes
                    .into_iter()
                    .map(|note| txp.view_note(note))
                    .collect(),
                payload_key: decrypted_memo_key,
            }),
            Err(_) => ActionView::Consolidate(ConsolidateView::Opaque {
                consolidate: self.to_owned(),
            }),
        }
    }
}

impl IsAction for Split {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let Some(first_output) = self.body.outputs.first() else {
            return ActionView::Split(SplitView::Opaque {
                split: self.to_owned(),
            });
        };
        let Some(payload_key) = txp
            .payload_keys
            .get(&first_output.note_payload.note_commitment)
        else {
            return ActionView::Split(SplitView::Opaque {
                split: self.to_owned(),
            });
        };

        let Some(spent_notes) = self
            .body
            .inputs
            .iter()
            .map(|input| txp.spend_nullifiers.get(&input.nullifier).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            return ActionView::Split(SplitView::Opaque {
                split: self.to_owned(),
            });
        };

        let Some(created_notes) = self
            .body
            .outputs
            .iter()
            .map(|output| {
                txp.payload_keys
                    .get(&output.note_payload.note_commitment)
                    .and_then(|output_payload_key| {
                        Note::decrypt_with_payload_key(
                            &output.note_payload.encrypted_note,
                            output_payload_key,
                            &output.note_payload.ephemeral_key,
                        )
                        .ok()
                    })
            })
            .collect::<Option<Vec<_>>>()
        else {
            return ActionView::Split(SplitView::Opaque {
                split: self.to_owned(),
            });
        };

        match first_output.wrapped_memo_key.decrypt_outgoing(payload_key) {
            Ok(decrypted_memo_key) => ActionView::Split(SplitView::Visible {
                split: self.to_owned(),
                spent_notes: spent_notes
                    .into_iter()
                    .map(|note| txp.view_note(note))
                    .collect(),
                created_notes: created_notes
                    .into_iter()
                    .map(|note| txp.view_note(note))
                    .collect(),
                payload_key: decrypted_memo_key,
            }),
            Err(_) => ActionView::Split(SplitView::Opaque {
                split: self.to_owned(),
            }),
        }
    }
}

impl IsAction for IbcRelay {
    fn balance_commitment(&self) -> balance::Commitment {
        Default::default()
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::IbcRelay(self.clone())
    }
}

impl IsAction for ShieldedIcs20Withdrawal {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let Some(payload_key) = txp
            .payload_keys
            .get(&self.body.change_output.note_payload.note_commitment)
        else {
            return ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Opaque {
                withdrawal: self.to_owned(),
            });
        };

        let Some(spent_notes) = self
            .body
            .inputs
            .iter()
            .map(|input| txp.spend_nullifiers.get(&input.nullifier).cloned())
            .collect::<Option<Vec<_>>>()
        else {
            return ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Opaque {
                withdrawal: self.to_owned(),
            });
        };

        let Ok(change_note) = Note::decrypt_with_payload_key(
            &self.body.change_output.note_payload.encrypted_note,
            payload_key,
            &self.body.change_output.note_payload.ephemeral_key,
        ) else {
            return ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Opaque {
                withdrawal: self.to_owned(),
            });
        };

        match self
            .body
            .change_output
            .wrapped_memo_key
            .decrypt_outgoing(payload_key)
        {
            Ok(decrypted_memo_key) => {
                ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Visible {
                    withdrawal: self.to_owned(),
                    spent_notes: spent_notes
                        .into_iter()
                        .map(|note| txp.view_note(note))
                        .collect(),
                    change_note: txp.view_note(change_note),
                    payload_key: decrypted_memo_key,
                })
            }
            Err(_) => ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Opaque {
                withdrawal: self.to_owned(),
            }),
        }
    }
}

impl IsAction for penumbra_sdk_validator::validator::Definition {
    fn balance_commitment(&self) -> balance::Commitment {
        Default::default()
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ValidatorDefinition(self.clone())
    }
}

impl IsAction for MsgRegisterAsset {
    fn balance_commitment(&self) -> balance::Commitment {
        Default::default()
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ComplianceRegisterAsset(self.clone())
    }
}

impl IsAction for MsgRegisterUser {
    fn balance_commitment(&self) -> balance::Commitment {
        Default::default()
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ComplianceRegisterUser(self.clone())
    }
}

impl IsAction for AggregateBundle {
    fn balance_commitment(&self) -> balance::Commitment {
        Default::default()
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::AggregateBundle(self.clone())
    }
}
