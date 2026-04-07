use ark_ff::Zero;
use decaf377::Fr;
use penumbra_sdk_asset::{balance, Value};
use penumbra_sdk_auction::auction::dutch::actions::{
    view::{ActionDutchAuctionScheduleView, ActionDutchAuctionWithdrawView},
    ActionDutchAuctionEnd, ActionDutchAuctionSchedule, ActionDutchAuctionWithdraw,
};
use penumbra_sdk_community_pool::{CommunityPoolDeposit, CommunityPoolOutput, CommunityPoolSpend};
use penumbra_sdk_dex::{
    lp::{
        action::{PositionClose, PositionOpen, PositionWithdraw},
        position,
        view::PositionOpenView,
        LpNft, PositionMetadata,
    },
    swap::{Swap, SwapCiphertext, SwapView},
    swap_claim::{SwapClaim, SwapClaimView},
};
use penumbra_sdk_funding::liquidity_tournament::{
    ActionLiquidityTournamentVote, ActionLiquidityTournamentVoteView,
};
use penumbra_sdk_governance::{
    DelegatorVote, DelegatorVoteView, ProposalDepositClaim, ProposalSubmit, ProposalWithdraw,
    ValidatorVote, VotingReceiptToken,
};
use penumbra_sdk_ibc::IbcRelay;
use penumbra_sdk_shielded_pool::{
    Ics20Withdrawal, Note, Output, OutputView, Spend, SpendView, Transfer, TransferView,
};
use penumbra_sdk_stake::{Delegate, Undelegate, UndelegateClaim};

use crate::{Action, ActionView, TransactionPerspective};

// TODO: how do we have this be implemented in the component crates?
// currently can't because of txp

/// Common behavior between Penumbra actions.
pub trait IsAction {
    fn balance_commitment(&self) -> balance::Commitment;
    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView;
}

// foreign types

impl From<DelegatorVote> for Action {
    fn from(value: DelegatorVote) -> Self {
        Action::DelegatorVote(value)
    }
}

impl IsAction for DelegatorVote {
    fn balance_commitment(&self) -> balance::Commitment {
        Value {
            amount: self.body.unbonded_amount,
            asset_id: VotingReceiptToken::new(self.body.proposal).id(),
        }
        .commit(Fr::zero())
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let delegator_vote_view = match txp.spend_nullifiers.get(&self.body.nullifier) {
            Some(note) => DelegatorVoteView::Visible {
                delegator_vote: self.to_owned(),
                note: txp.view_note(note.to_owned()),
            },
            None => DelegatorVoteView::Opaque {
                delegator_vote: self.to_owned(),
            },
        };

        ActionView::DelegatorVote(delegator_vote_view)
    }
}

impl IsAction for ProposalDepositClaim {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ProposalDepositClaim(self.clone())
    }
}

impl IsAction for ProposalSubmit {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ProposalSubmit(self.to_owned())
    }
}

impl IsAction for ProposalWithdraw {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ProposalWithdraw(self.to_owned())
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

impl IsAction for Output {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let note_commitment = self.body.note_payload.note_commitment;
        let epk = self.body.note_payload.ephemeral_key;
        // Retrieve payload key for associated note commitment
        let output_view = if let Some(payload_key) = txp.payload_keys.get(&note_commitment) {
            let decrypted_note = Note::decrypt_with_payload_key(
                &self.body.note_payload.encrypted_note,
                payload_key,
                &epk,
            );

            let decrypted_memo_key = self.body.wrapped_memo_key.decrypt_outgoing(payload_key);

            if let (Ok(decrypted_note), Ok(decrypted_memo_key)) =
                (decrypted_note, decrypted_memo_key)
            {
                // Neither decryption failed, so return the visible ActionView
                OutputView::Visible {
                    output: self.to_owned(),
                    note: txp.view_note(decrypted_note),
                    payload_key: decrypted_memo_key,
                }
            } else {
                // One or both of the note or memo key is missing, so return the opaque ActionView
                OutputView::Opaque {
                    output: self.to_owned(),
                }
            }
        } else {
            // There was no payload key found, so return the opaque ActionView
            OutputView::Opaque {
                output: self.to_owned(),
            }
        };

        ActionView::Output(output_view)
    }
}

impl IsAction for Spend {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let spend_view = match txp.spend_nullifiers.get(&self.body.nullifier) {
            Some(note) => SpendView::Visible {
                spend: self.to_owned(),
                note: txp.view_note(note.to_owned()),
            },
            None => SpendView::Opaque {
                spend: self.to_owned(),
            },
        };

        ActionView::Spend(spend_view)
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
        let note_commitment = first_output.note_payload.note_commitment;
        let payload_key = match txp.payload_keys.get(&note_commitment) {
            Some(payload_key) => payload_key,
            None => {
                return ActionView::Transfer(TransferView::Opaque {
                    transfer: self.to_owned(),
                })
            }
        };
        let spent_notes = match self
            .body
            .inputs
            .iter()
            .map(|input| txp.spend_nullifiers.get(&input.nullifier).cloned())
            .collect::<Option<Vec<_>>>()
        {
            Some(notes) => notes,
            None => {
                return ActionView::Transfer(TransferView::Opaque {
                    transfer: self.to_owned(),
                })
            }
        };
        let created_notes = match self
            .body
            .outputs
            .iter()
            .map(|output| {
                Note::decrypt_with_payload_key(
                    &output.note_payload.encrypted_note,
                    payload_key,
                    &output.note_payload.ephemeral_key,
                )
            })
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(notes) => notes,
            Err(_) => {
                return ActionView::Transfer(TransferView::Opaque {
                    transfer: self.to_owned(),
                })
            }
        };

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

impl IsAction for Delegate {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::Delegate(self.to_owned())
    }
}

impl IsAction for Undelegate {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::Undelegate(self.to_owned())
    }
}

impl IsAction for UndelegateClaim {
    fn balance_commitment(&self) -> balance::Commitment {
        self.body.balance_commitment
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::UndelegateClaim(self.to_owned())
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

impl IsAction for Ics20Withdrawal {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::Ics20Withdrawal(self.to_owned())
    }
}

impl IsAction for CommunityPoolDeposit {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::CommunityPoolDeposit(self.clone())
    }
}

impl IsAction for CommunityPoolOutput {
    fn balance_commitment(&self) -> balance::Commitment {
        // Outputs from the Community Pool require value
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::CommunityPoolOutput(self.clone())
    }
}

impl IsAction for CommunityPoolSpend {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::CommunityPoolSpend(self.clone())
    }
}

impl IsAction for PositionOpen {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let view = match txp.position_metadata_key.and_then(|key| {
            PositionMetadata::decrypt(&key, self.encrypted_metadata.as_ref().map(|x| x.as_slice()))
                .ok()
                .flatten()
        }) {
            None => PositionOpenView::Opaque {
                action: self.clone(),
            },
            Some(metadata) => PositionOpenView::Visible {
                action: self.clone(),
                metadata,
            },
        };
        ActionView::PositionOpen(view)
    }
}

impl IsAction for PositionClose {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::PositionClose(self.to_owned())
    }
}

impl IsAction for PositionWithdraw {
    fn balance_commitment(&self) -> balance::Commitment {
        let prev_state_nft = if self.sequence == 0 {
            Value {
                amount: 1u64.into(),
                asset_id: LpNft::new(self.position_id, position::State::Closed).asset_id(),
            }
        } else {
            Value {
                amount: 1u64.into(),
                asset_id: LpNft::new(
                    self.position_id,
                    position::State::Withdrawn {
                        sequence: self.sequence - 1,
                    },
                )
                .asset_id(),
            }
        }
        .commit(Fr::zero());

        let next_state_nft = Value {
            amount: 1u64.into(),
            asset_id: LpNft::new(
                self.position_id,
                position::State::Withdrawn {
                    sequence: self.sequence,
                },
            )
            .asset_id(),
        }
        .commit(Fr::zero());

        // The action consumes a closed position and produces the position's reserves and a withdrawn position NFT.
        self.reserves_commitment - prev_state_nft + next_state_nft
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::PositionWithdraw(self.to_owned())
    }
}

impl IsAction for Swap {
    /// Compute a commitment to the value contributed to a transaction by this swap.
    /// Will subtract (v1,t1), (v2,t2), and (f,fee_token)
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance_commitment_inner()
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let commitment = self.body.payload.commitment;

        let plaintext = txp.payload_keys.get(&commitment).and_then(|payload_key| {
            // Decrypt swap ciphertext
            SwapCiphertext::decrypt_with_payload_key(&self.body.payload.encrypted_swap, payload_key)
                .ok()
        });

        ActionView::Swap(match plaintext {
            Some(swap_plaintext) => {
                // If we can find a matching BSOD in the TxP, use it to compute the output notes
                // for the swap.
                let bsod = txp
                    .batch_swap_output_data
                    .iter()
                    // This finds the first matching one; there should only be one
                    // per trading pair per block and we trust the TxP provider not to lie about it.
                    .find(|bsod| bsod.trading_pair == swap_plaintext.trading_pair);

                let (output_1, output_2) = match bsod.map(|bsod| swap_plaintext.output_notes(bsod))
                {
                    Some((output_1, output_2)) => {
                        (Some(txp.view_note(output_1)), Some(txp.view_note(output_2)))
                    }
                    None => (None, None),
                };

                SwapView::Visible {
                    swap: self.to_owned(),
                    swap_plaintext: swap_plaintext.clone(),
                    output_1,
                    output_2,
                    claim_tx: txp
                        .nullification_transaction_ids_by_commitment
                        .get(&commitment)
                        .cloned(),
                    batch_swap_output_data: bsod.cloned(),
                    asset_1_metadata: txp
                        .denoms
                        .get(&swap_plaintext.trading_pair.asset_1())
                        .cloned(),
                    asset_2_metadata: txp
                        .denoms
                        .get(&swap_plaintext.trading_pair.asset_2())
                        .cloned(),
                }
            }
            None => {
                // If we can find a matching BSOD in the TxP, we can use it to compute the output notes
                // for the swap.
                let bsod = txp
                    .batch_swap_output_data
                    .iter()
                    // This finds the first matching one; there should only be one
                    // per trading pair per block and we trust the TxP provider not to lie about it.
                    .find(|bsod| bsod.trading_pair == self.body.trading_pair);

                // We can get the denom metadata whether we get a BSOD or not
                let denom_1 = txp.denoms.get(&self.body.trading_pair.asset_1()).cloned();
                let denom_2 = txp.denoms.get(&self.body.trading_pair.asset_2()).cloned();

                match bsod {
                    None => {
                        // If we can't find a matching BSOD, we can't compute the output notes
                        // for the swap.
                        SwapView::Opaque {
                            swap: self.to_owned(),
                            batch_swap_output_data: None,
                            output_1: None,
                            output_2: None,
                            asset_1_metadata: denom_1.clone(),
                            asset_2_metadata: denom_2.clone(),
                        }
                    }
                    Some(bsod) => {
                        // If we can find a matching BSOD, use it to compute the output notes
                        // for the swap.
                        let (lambda_1_i, lambda_2_i) =
                            bsod.pro_rata_outputs((self.body.delta_1_i, self.body.delta_2_i));
                        SwapView::Opaque {
                            swap: self.to_owned(),
                            batch_swap_output_data: Some(bsod.clone()),
                            asset_1_metadata: denom_1.clone(),
                            asset_2_metadata: denom_2.clone(),
                            output_1: Some(
                                Value {
                                    amount: lambda_1_i,
                                    asset_id: self.body.trading_pair.asset_1(),
                                }
                                .view_with_cache(&txp.denoms),
                            ),
                            output_2: Some(
                                Value {
                                    amount: lambda_2_i,
                                    asset_id: self.body.trading_pair.asset_2(),
                                }
                                .view_with_cache(&txp.denoms),
                            ),
                        }
                    }
                }
            }
        })
    }
}

impl IsAction for SwapClaim {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        // Get the advice notes for each output from the swap claim
        let output_1 = txp.advice_notes.get(&self.body.output_1_commitment);
        let output_2 = txp.advice_notes.get(&self.body.output_2_commitment);

        match (output_1, output_2) {
            (Some(output_1), Some(output_2)) => {
                let swap_claim_view = SwapClaimView::Visible {
                    swap_claim: self.to_owned(),
                    output_1: txp.view_note(output_1.to_owned()),
                    output_2: txp.view_note(output_2.to_owned()),
                    swap_tx: txp
                        .creation_transaction_ids_by_nullifier
                        .get(&self.body.nullifier)
                        .cloned(),
                };
                ActionView::SwapClaim(swap_claim_view)
            }
            _ => {
                let swap_claim_view = SwapClaimView::Opaque {
                    swap_claim: self.to_owned(),
                };
                ActionView::SwapClaim(swap_claim_view)
            }
        }
    }
}

impl IsAction for ActionDutchAuctionSchedule {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let view = ActionDutchAuctionScheduleView {
            action: self.to_owned(),
            auction_id: self.description.id(),
            input_metadata: txp.denoms.get_by_id(self.description.input.asset_id),
            output_metadata: txp.denoms.get_by_id(self.description.output_id),
        };
        ActionView::ActionDutchAuctionSchedule(view)
    }
}

impl IsAction for ActionDutchAuctionEnd {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance().commit(Fr::zero())
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        ActionView::ActionDutchAuctionEnd(self.to_owned())
    }
}

impl IsAction for ActionDutchAuctionWithdraw {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance_commitment()
    }

    fn view_from_perspective(&self, _txp: &TransactionPerspective) -> ActionView {
        let view = ActionDutchAuctionWithdrawView {
            action: self.to_owned(),
            reserves: vec![],
        };
        ActionView::ActionDutchAuctionWithdraw(view)
    }
}

impl IsAction for ActionLiquidityTournamentVote {
    fn balance_commitment(&self) -> balance::Commitment {
        self.balance_commitment()
    }

    fn view_from_perspective(&self, txp: &TransactionPerspective) -> ActionView {
        let lqt_vote_view = match txp.spend_nullifiers.get(&self.body.nullifier) {
            Some(note) => ActionLiquidityTournamentVoteView::Visible {
                vote: self.to_owned(),
                note: txp.view_note(note.to_owned()),
            },
            None => ActionLiquidityTournamentVoteView::Opaque {
                vote: self.to_owned(),
            },
        };

        ActionView::ActionLiquidityTournamentVote(lqt_vote_view)
    }
}

#[cfg(test)]
mod tests {
    use decaf377::Fq;
    use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
    use penumbra_sdk_asset::{asset, Balance, Value};
    use penumbra_sdk_keys::{
        keys::{Bip44Path, SeedPhrase, SpendKey},
        symmetric::{OvkWrappedKey, PayloadKey, WrappedMemoKey},
    };
    use penumbra_sdk_sct::Nullifier;
    use penumbra_sdk_shielded_pool::{
        EncryptedBackref, Note, Rseed, Transfer, TransferBody, TransferFamilyId, TransferInputBody,
        TransferOutputBody, TransferProof, TransferView,
    };

    use crate::{is_action::IsAction, ActionView, TransactionPerspective};

    fn make_note_and_keys() -> (Note, penumbra_sdk_keys::keys::FullViewingKey) {
        let mut rng = rand::thread_rng();
        let sk =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(&mut rng), &Bip44Path::new(0));
        let fvk = sk.full_viewing_key().clone();
        let (addr, _) = fvk.incoming().payment_address(0u32.into());
        let value = Value {
            amount: 10u64.into(),
            asset_id: asset::Id(Fq::from(1u64)),
        };
        let rseed = Rseed::generate(&mut rng);
        let note = Note::from_parts(addr, value, rseed).unwrap();
        (note, fvk)
    }

    fn make_transfer(
        note: &Note,
        payload_key: &PayloadKey,
        nullifier: Nullifier,
        valid_memo: bool,
    ) -> Transfer {
        let note_payload = note.payload();
        let wrapped_memo_key = if valid_memo {
            WrappedMemoKey::encrypt(
                payload_key,
                note.ephemeral_secret_key(),
                note.transmission_key(),
                &note.diversified_generator(),
            )
        } else {
            WrappedMemoKey([0u8; 48])
        };
        Transfer {
            body: TransferBody {
                family_id: TransferFamilyId::OneByOne,
                anchor: penumbra_sdk_tct::Tree::default().root(),
                balance_commitment: Balance::from(Value {
                    amount: 1u64.into(),
                    asset_id: asset::Id(Fq::from(1u64)),
                })
                .commit(decaf377::Fr::from(0u64)),
                inputs: vec![TransferInputBody {
                    nullifier,
                    rk: VerificationKey::from(SigningKey::<SpendAuth>::from(decaf377::Fr::from(
                        1u64,
                    ))),
                    encrypted_backref: EncryptedBackref::dummy(),
                    compliance_ciphertext: vec![],
                    dleq_proof: vec![],
                }],
                outputs: vec![TransferOutputBody {
                    note_payload,
                    wrapped_memo_key,
                    ovk_wrapped_key: OvkWrappedKey([0u8; 48]),
                    compliance_ciphertext: vec![],
                    dleq_proofs: vec![],
                }],
                target_timestamp: 0,
                compliance_anchor: penumbra_sdk_tct::StateCommitment(Fq::from(0u64)),
                asset_anchor: penumbra_sdk_tct::StateCommitment(Fq::from(0u64)),
            },
            auth_sigs: vec![[0u8; 64].into()],
            proof: TransferProof::default(),
        }
    }

    fn derive_payload_key(
        note: &Note,
        fvk: &penumbra_sdk_keys::keys::FullViewingKey,
    ) -> (PayloadKey, penumbra_sdk_tct::StateCommitment) {
        let note_payload = note.payload();
        let ivk = fvk.incoming();
        let shared_secret = ivk
            .key_agreement_with(&note_payload.ephemeral_key)
            .expect("key agreement succeeded");
        let payload_key = PayloadKey::derive(&shared_secret, &note_payload.ephemeral_key);
        (payload_key, note_payload.note_commitment)
    }

    #[test]
    fn transfer_view_visible_when_both_maps_present_and_memo_valid() {
        let (note, fvk) = make_note_and_keys();
        let (payload_key, note_commitment) = derive_payload_key(&note, &fvk);
        let nullifier = Nullifier(Fq::from(42u64));
        let transfer = make_transfer(&note, &payload_key, nullifier, true);

        let mut txp = TransactionPerspective::default();
        txp.payload_keys.insert(note_commitment, payload_key);
        txp.spend_nullifiers.insert(nullifier, note.clone());

        let view = transfer.view_from_perspective(&txp);
        assert!(matches!(
            view,
            ActionView::Transfer(TransferView::Visible { .. })
        ));
    }

    #[test]
    fn transfer_view_opaque_when_payload_key_missing() {
        let (note, fvk) = make_note_and_keys();
        let (payload_key, _) = derive_payload_key(&note, &fvk);
        let nullifier = Nullifier(Fq::from(43u64));
        let transfer = make_transfer(&note, &payload_key, nullifier, true);

        let mut txp = TransactionPerspective::default();
        // payload_keys intentionally absent
        txp.spend_nullifiers.insert(nullifier, note.clone());

        let view = transfer.view_from_perspective(&txp);
        assert!(matches!(
            view,
            ActionView::Transfer(TransferView::Opaque { .. })
        ));
    }

    #[test]
    fn transfer_view_opaque_when_nullifier_missing() {
        let (note, fvk) = make_note_and_keys();
        let (payload_key, note_commitment) = derive_payload_key(&note, &fvk);
        let nullifier = Nullifier(Fq::from(44u64));
        let transfer = make_transfer(&note, &payload_key, nullifier, true);

        let mut txp = TransactionPerspective::default();
        txp.payload_keys.insert(note_commitment, payload_key);
        // spend_nullifiers intentionally absent

        let view = transfer.view_from_perspective(&txp);
        assert!(matches!(
            view,
            ActionView::Transfer(TransferView::Opaque { .. })
        ));
    }

    #[test]
    fn transfer_view_opaque_when_memo_key_invalid() {
        let (note, fvk) = make_note_and_keys();
        let (payload_key, note_commitment) = derive_payload_key(&note, &fvk);
        let nullifier = Nullifier(Fq::from(45u64));
        // valid_memo = false → WrappedMemoKey([0u8; 48]) → decrypt_outgoing fails
        let transfer = make_transfer(&note, &payload_key, nullifier, false);

        let mut txp = TransactionPerspective::default();
        txp.payload_keys.insert(note_commitment, payload_key);
        txp.spend_nullifiers.insert(nullifier, note.clone());

        let view = transfer.view_from_perspective(&txp);
        assert!(matches!(
            view,
            ActionView::Transfer(TransferView::Opaque { .. })
        ));
    }
}
