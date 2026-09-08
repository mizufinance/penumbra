use comfy_table::presets;
use comfy_table::Table;
use shieldd_sdk_asset::ValueView;
use shieldd_sdk_keys::AddressView;
use shieldd_sdk_transaction::TransactionView;

fn format_opaque_bytes(bytes: &[u8]) -> String {
    if bytes.len() < 8 {
        return String::new();
    }
    hex::encode_upper(&bytes[..bytes.len().min(32)])
        .chars()
        .map(|c| match c {
            '0' => '\u{2595}',
            '1' => '\u{2581}',
            '2' => '\u{2582}',
            '3' => '\u{2583}',
            '4' => '\u{2584}',
            '5' => '\u{2585}',
            '6' => '\u{2586}',
            '7' => '\u{2587}',
            '8' => '\u{2588}',
            '9' => '\u{2589}',
            'A' => '\u{259A}',
            'B' => '\u{259B}',
            'C' => '\u{259C}',
            'D' => '\u{259D}',
            'E' => '\u{259E}',
            'F' => '\u{259F}',
            _ => unreachable!("hex encoding emits only hexadecimal characters"),
        })
        .collect()
}

fn format_address_view(address_view: &AddressView) -> String {
    match address_view {
        AddressView::Decoded {
            address: _,
            index,
            wallet_id: _,
        } => {
            if !index.is_ephemeral() {
                format!("[account {:?}]", index.account)
            } else {
                format!("[account {:?} (one-time address)]", index.account)
            }
        }
        AddressView::Opaque { address } => {
            // The address being opaque just means we can't see the internal structure,
            // we should render the content so it can be copy-pasted.
            format!("{}", address)
        }
    }
}

fn format_value_view(value_view: &ValueView) -> String {
    match value_view {
        ValueView::KnownAssetId {
            amount,
            metadata: denom,
            ..
        } => {
            let unit = denom.default_unit();
            format!("{}{}", unit.format_value(*amount), unit)
        }
        ValueView::UnknownAssetId { amount, asset_id } => {
            format!("{}{}", amount, asset_id)
        }
    }
}

pub trait TransactionViewExt {
    /// Render this transaction view on stdout.
    fn render_terminal(&self);
}

impl TransactionViewExt for TransactionView {
    fn render_terminal(&self) {
        let fee = &self.body_view.transaction_parameters.fee;
        println!("Fee: {}", fee.amount());

        println!(
            "Expiration Height: {}",
            &self.body_view.transaction_parameters.expiry_height
        );

        if let Some(memo_view) = &self.body_view.memo_view {
            match memo_view {
                shieldd_sdk_transaction::MemoView::Visible {
                    plaintext,
                    ciphertext: _,
                } => {
                    println!("Memo Sender: {}", &plaintext.return_address.address());
                    println!("Memo Text: \n{}\n", &plaintext.text);
                }
                shieldd_sdk_transaction::MemoView::Opaque { ciphertext } => {
                    println!("Encrypted Memo: \n{}\n", format_opaque_bytes(&ciphertext.0));
                }
            }
        }

        let mut actions_table = Table::new();
        actions_table.load_preset(presets::NOTHING);
        actions_table.set_header(vec!["Tx Action", "Description"]);

        for action_view in &self.body_view.action_views {
            let action: String;

            let row = match action_view {
                shieldd_sdk_transaction::ActionView::Transfer(transfer) => match transfer {
                    shieldd_sdk_transaction::view::action_view::TransferView::Visible {
                        transfer: _,
                        spent_notes: _,
                        created_notes,
                        payload_key: _,
                    } => {
                        if let Some(created_note) = created_notes.first() {
                            action = format!(
                                "{} -> {}",
                                format_value_view(&created_note.value),
                                format_address_view(&created_note.address),
                            );
                        } else {
                            action = "<empty transfer>".to_string();
                        }
                        ["Transfer", &action]
                    }
                    shieldd_sdk_transaction::view::action_view::TransferView::Opaque {
                        transfer,
                    } => {
                        if let Some(first_output) = transfer.body.outputs.first() {
                            let bytes = first_output.note_payload.encrypted_note.0;
                            action = format_opaque_bytes(&bytes);
                        } else {
                            action = "<empty transfer>".to_string();
                        }
                        ["Transfer", &action]
                    }
                },
                shieldd_sdk_transaction::ActionView::NoteReshape(note_reshape) => {
                    match note_reshape {
                        shieldd_sdk_transaction::view::action_view::NoteReshapeView::Visible {
                            note_reshape: _,
                            spent_notes: _,
                            created_notes,
                            payload_key: _,
                        } => {
                            if let Some(created_note) = created_notes.first() {
                                action = format!(
                                    "{} -> {}",
                                    format_value_view(&created_note.value),
                                    format_address_view(&created_note.address),
                                );
                            } else {
                                action = "<empty note reshape>".to_string();
                            }
                            ["NoteReshape", &action]
                        }
                        shieldd_sdk_transaction::view::action_view::NoteReshapeView::Opaque {
                            note_reshape,
                        } => {
                            if let Some(first_output) = note_reshape.body.outputs.first() {
                                let bytes = first_output.note_payload.encrypted_note.0;
                                action = format_opaque_bytes(&bytes);
                            } else {
                                action = "<empty note reshape>".to_string();
                            }
                            ["NoteReshape", &action]
                        }
                    }
                }
                shieldd_sdk_transaction::ActionView::ShieldedIcs20Withdrawal(withdrawal) => {
                    let withdrawal = match withdrawal {
                        shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalView::Visible {
                            withdrawal,
                            ..
                        } => &withdrawal.body.withdrawal,
                        shieldd_sdk_shielded_pool::ShieldedIcs20WithdrawalView::Opaque {
                            withdrawal,
                        } => &withdrawal.body.withdrawal,
                    };
                    let unit = withdrawal.denom.best_unit_for(withdrawal.amount);
                    action = format!(
                        "{}{} via {} to {}",
                        unit.format_value(withdrawal.amount),
                        unit,
                        withdrawal.source_channel,
                        withdrawal.destination_chain_address,
                    );
                    ["Ics20 Withdrawal", &action]
                }
                shieldd_sdk_transaction::ActionView::ShieldedHostWithdrawal(withdrawal) => {
                    let withdrawal = match withdrawal {
                        shieldd_sdk_shielded_pool::ShieldedHostWithdrawalView::Visible {
                            withdrawal,
                            ..
                        } => &withdrawal.body.withdrawal,
                        shieldd_sdk_shielded_pool::ShieldedHostWithdrawalView::Opaque {
                            withdrawal,
                        } => &withdrawal.body.withdrawal,
                    };
                    action = match &withdrawal.destination {
                        shieldd_sdk_shielded_pool::HostWithdrawalDestination::Transfer(
                            transfer,
                        ) => {
                            format!(
                                "{} of {} to {}",
                                withdrawal.value.amount,
                                withdrawal.value.asset_id,
                                transfer.recipient,
                            )
                        }
                        shieldd_sdk_shielded_pool::HostWithdrawalDestination::Execution(
                            execution,
                        ) => format!(
                            "{} of {} via {} host calls (gas {}, refund {})",
                            withdrawal.value.amount,
                            withdrawal.value.asset_id,
                            execution.calls.len(),
                            execution.gas_limit,
                            execution.refund_address,
                        ),
                    };
                    ["Host Withdrawal", &action]
                }
                shieldd_sdk_transaction::ActionView::IbcRelay(_) => ["IBC Relay", ""],
                shieldd_sdk_transaction::ActionView::ComplianceRegisterAsset(x) => {
                    action = format!(
                        "Register asset {} as {}",
                        x.asset_id,
                        if x.is_regulated {
                            "regulated"
                        } else {
                            "unregulated"
                        }
                    );
                    ["Compliance: Register Asset", &action]
                }
                shieldd_sdk_transaction::ActionView::ComplianceRegisterUser(x) => {
                    action = format!("Register user for asset {}", x.leaf.asset_id);
                    ["Compliance: Register User", &action]
                }
                shieldd_sdk_transaction::ActionView::AggregateBundle(bundle) => {
                    action = format!(
                        "{} proof-family aggregates (protocol v{})",
                        bundle.families.len(),
                        bundle.version
                    );
                    ["Aggregate Bundle", &action]
                }
            };

            actions_table.add_row(row);
        }

        // Print table of actions and their descriptions
        println!("{actions_table}");
    }
}
