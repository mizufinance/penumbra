use shieldd_sdk_asset::balance;
use shieldd_sdk_compliance::structs::{MsgRegisterAsset, MsgRegisterUser};
use shieldd_sdk_ibc::IbcRelay;
use shieldd_sdk_shielded_pool::{
    Note, NoteReshape, NoteReshapeView, ShieldedHostWithdrawal, ShieldedHostWithdrawalView,
    ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalView, Transfer, TransferView,
};

use crate::{ActionView, TransactionPerspective};

/// Common behavior between Shieldd actions.
pub trait IsAction {
    fn balance_commitment(&self) -> balance::Commitment;
    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView;
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
            if note.amount() == shieldd_sdk_num::Amount::zero() && note.address() == sender_address
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

impl IsAction for NoteReshape {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let Some(first_output) = self.body.outputs.first() else {
            return ActionView::NoteReshape(NoteReshapeView::Opaque {
                note_reshape: self.to_owned(),
            });
        };
        let Some(payload_key) = txp
            .payload_keys
            .get(&first_output.note_payload.note_commitment)
        else {
            return ActionView::NoteReshape(NoteReshapeView::Opaque {
                note_reshape: self.to_owned(),
            });
        };

        let spent_notes = self
            .body
            .inputs
            .iter()
            .filter_map(|input| txp.spend_nullifiers.get(&input.nullifier).cloned())
            .collect::<Vec<_>>();
        if spent_notes.is_empty() {
            return ActionView::NoteReshape(NoteReshapeView::Opaque {
                note_reshape: self.to_owned(),
            });
        }
        let sender_address = spent_notes[0].address();

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
            return ActionView::NoteReshape(NoteReshapeView::Opaque {
                note_reshape: self.to_owned(),
            });
        };
        let created_notes = created_notes
            .into_iter()
            .filter(|note| {
                note.amount() != shieldd_sdk_num::Amount::zero() || note.address() != sender_address
            })
            .collect::<Vec<_>>();

        match first_output.wrapped_memo_key.decrypt_outgoing(payload_key) {
            Ok(decrypted_memo_key) => ActionView::NoteReshape(NoteReshapeView::Visible {
                note_reshape: self.to_owned(),
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
            Err(_) => ActionView::NoteReshape(NoteReshapeView::Opaque {
                note_reshape: self.to_owned(),
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

        let spent_notes = self
            .body
            .inputs
            .iter()
            .filter_map(|input| txp.spend_nullifiers.get(&input.nullifier).cloned())
            .collect::<Vec<_>>();
        if spent_notes.is_empty() {
            return ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Opaque {
                withdrawal: self.to_owned(),
            });
        }
        let sender_address = spent_notes[0].address();

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
                    change_note: (change_note.amount() != shieldd_sdk_num::Amount::zero()
                        || change_note.address() != sender_address)
                        .then(|| txp.view_note(change_note)),
                    payload_key: decrypted_memo_key,
                })
            }
            Err(_) => ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Opaque {
                withdrawal: self.to_owned(),
            }),
        }
    }
}

impl IsAction for ShieldedHostWithdrawal {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let Some(payload_key) = txp
            .payload_keys
            .get(&self.body.change_output.note_payload.note_commitment)
        else {
            return ActionView::ShieldedHostWithdrawal(ShieldedHostWithdrawalView::Opaque {
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
            return ActionView::ShieldedHostWithdrawal(ShieldedHostWithdrawalView::Opaque {
                withdrawal: self.to_owned(),
            });
        };

        let Ok(change_note) = Note::decrypt_with_payload_key(
            &self.body.change_output.note_payload.encrypted_note,
            payload_key,
            &self.body.change_output.note_payload.ephemeral_key,
        ) else {
            return ActionView::ShieldedHostWithdrawal(ShieldedHostWithdrawalView::Opaque {
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
                ActionView::ShieldedHostWithdrawal(ShieldedHostWithdrawalView::Visible {
                    withdrawal: self.to_owned(),
                    spent_notes: spent_notes
                        .into_iter()
                        .map(|note| txp.view_note(note))
                        .collect(),
                    change_note: txp.view_note(change_note),
                    payload_key: decrypted_memo_key,
                })
            }
            Err(_) => ActionView::ShieldedHostWithdrawal(ShieldedHostWithdrawalView::Opaque {
                withdrawal: self.to_owned(),
            }),
        }
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

#[cfg(test)]
mod tests {
    use std::{ops::Deref, str::FromStr};

    use decaf377::Fr;
    use ibc_types::core::{channel::ChannelId, client::Height as IbcHeight};
    use rand_core::OsRng;
    use shieldd_sdk_asset::{Value, BASE_ASSET_DENOM};
    use shieldd_sdk_keys::{test_keys, PayloadKey};
    use shieldd_sdk_shielded_pool::{
        Ics20Withdrawal, Note, ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalProof,
        ShieldedIcs20WithdrawalView, ShieldedInputPlan,
    };
    use shieldd_sdk_tct::Tree;

    use super::{IsAction, TransactionPerspective};
    use crate::ActionView;

    #[test]
    fn withdrawal_view_ignores_private_padding_and_hides_synthetic_change() {
        let spent_note = Note::generate(
            &mut OsRng,
            &test_keys::ADDRESS_0,
            Value {
                amount: 40_000u64.into(),
                asset_id: BASE_ASSET_DENOM.id(),
            },
        );
        let spend = ShieldedInputPlan::new(&mut OsRng, spent_note.clone(), 0u64.into());
        let plan = shieldd_sdk_shielded_pool::test_plan_helpers::ics20_withdrawal(
            vec![spend],
            None,
            Ics20Withdrawal {
                amount: 40_000u64.into(),
                denom: BASE_ASSET_DENOM.clone(),
                destination_chain_address: "cosmos1destination".to_string(),
                return_address: test_keys::ADDRESS_0.deref().clone(),
                timeout_height: IbcHeight::new(1, 10).expect("valid timeout height"),
                timeout_time: 60_000_000_000,
                source_channel: ChannelId::from_str("channel-0").expect("valid channel id"),
                ics20_memo: String::new(),
                use_transparent_address: false,
            },
            Fr::from(7u64),
        )
        .expect("valid withdrawal plan");
        let memo_key: PayloadKey = [7u8; 32].into();
        let body = plan
            .action_body(
                &test_keys::FULL_VIEWING_KEY,
                &memo_key,
                Tree::default().root(),
                0,
            )
            .expect("build withdrawal body");
        let action = ShieldedIcs20Withdrawal {
            auth_sigs: vec![[0u8; 64].into(); body.family_id.auth_sig_count()],
            body,
            proof: ShieldedIcs20WithdrawalProof::default(),
        };

        let output = &action.body.change_output;
        let shared_secret = Note::decrypt_key(
            output.ovk_wrapped_key.clone(),
            output.note_payload.note_commitment,
            action.body.balance_commitment,
            test_keys::FULL_VIEWING_KEY.outgoing(),
            &output.note_payload.ephemeral_key,
        )
        .expect("unwrap synthetic change payload key");
        let payload_key = PayloadKey::derive(&shared_secret, &output.note_payload.ephemeral_key);

        let mut perspective = TransactionPerspective::default();
        perspective
            .spend_nullifiers
            .insert(action.body.inputs[0].nullifier, spent_note);
        perspective
            .payload_keys
            .insert(output.note_payload.note_commitment, payload_key);
        assert!(
            !perspective
                .spend_nullifiers
                .contains_key(&action.body.inputs[1].nullifier),
            "regression requires the synthetic nullifier to be absent from wallet storage"
        );

        match action.view_from_perspective(&perspective) {
            ActionView::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalView::Visible {
                spent_notes,
                change_note,
                ..
            }) => {
                assert_eq!(spent_notes.len(), 1);
                assert!(
                    change_note.is_none(),
                    "zero sender-owned synthetic change must not appear in the view"
                );
            }
            other => panic!("expected visible withdrawal view, got {other:?}"),
        }
    }
}
