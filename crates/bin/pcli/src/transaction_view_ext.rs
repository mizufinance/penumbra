use comfy_table::presets;
use comfy_table::Table;
use penumbra_sdk_asset::ValueView;
use penumbra_sdk_fee::Fee;
use penumbra_sdk_keys::AddressView;
use penumbra_sdk_transaction::TransactionView;

// Issues identified:
// TODO: FeeView
// TODO: TradingPairView
// Implemented some helper functions which may make more sense as methods on existing Structs

// a helper function to create pretty placeholders for encrypted information
fn format_opaque_bytes(bytes: &[u8]) -> String {
    if bytes.len() < 8 {
        return String::new();
    } else {
        /*
        // TODO: Hm, this can allow the same color for both, should rejig things to avoid this
        // Select foreground and background colors based on the first 8 bytes.
        let fg_color_index = bytes[0] % 8;
        let bg_color_index = bytes[4] % 8;

        // ANSI escape codes for foreground and background colors.
        let fg_color_code = 37; // 30 through 37 are foreground colors
        let bg_color_code = 40; // 40 through 47 are background colors
        */

        // to be more general, perhaps this should be configurable
        // an opaque address needs less space than an opaque memo, etc
        let max_bytes = 32;
        let rem = if bytes.len() > max_bytes {
            bytes[0..max_bytes].to_vec()
        } else {
            bytes.to_vec()
        };

        // Convert the rest of the bytes to hexadecimal.
        let hex_str = hex::encode_upper(rem);
        let opaque_chars: String = hex_str
            .chars()
            .map(|c| {
                match c {
                    '0' => "\u{2595}",
                    '1' => "\u{2581}",
                    '2' => "\u{2582}",
                    '3' => "\u{2583}",
                    '4' => "\u{2584}",
                    '5' => "\u{2585}",
                    '6' => "\u{2586}",
                    '7' => "\u{2587}",
                    '8' => "\u{2588}",
                    '9' => "\u{2589}",
                    'A' => "\u{259A}",
                    'B' => "\u{259B}",
                    'C' => "\u{259C}",
                    'D' => "\u{259D}",
                    'E' => "\u{259E}",
                    'F' => "\u{259F}",
                    _ => "",
                }
                .to_string()
            })
            .collect();

        //format!("\u{001b}[{};{}m{}", fg_color_code, bg_color_code, block_chars)
        format!("{}", opaque_chars)
    }
}

// feels like these functions should be extension traits of their respective structs
// propose moving this to core/keys/src/address/view.rs
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

// feels like these functions should be extension traits of their respective structs
// propose moving this to core/asset/src/value.rs
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

fn format_fee(fee: &Fee) -> String {
    // TODO: Implement FeeView to show decrypted fee.
    format!("{}", fee.amount())
}

pub trait TransactionViewExt {
    /// Render this transaction view on stdout.
    fn render_terminal(&self);
}

impl TransactionViewExt for TransactionView {
    fn render_terminal(&self) {
        let fee = &self.body_view.transaction_parameters.fee;
        // the denomination should be visible here... does a FeeView exist?
        println!("Fee: {}", format_fee(&fee));

        println!(
            "Expiration Height: {}",
            &self.body_view.transaction_parameters.expiry_height
        );

        if let Some(memo_view) = &self.body_view.memo_view {
            match memo_view {
                penumbra_sdk_transaction::MemoView::Visible {
                    plaintext,
                    ciphertext: _,
                } => {
                    println!("Memo Sender: {}", &plaintext.return_address.address());
                    println!("Memo Text: \n{}\n", &plaintext.text);
                }
                penumbra_sdk_transaction::MemoView::Opaque { ciphertext } => {
                    println!("Encrypted Memo: \n{}\n", format_opaque_bytes(&ciphertext.0));
                }
            }
        }

        let mut actions_table = Table::new();
        actions_table.load_preset(presets::NOTHING);
        actions_table.set_header(vec!["Tx Action", "Description"]);

        // Iterate over the ActionViews in the TxView & display as appropriate
        for action_view in &self.body_view.action_views {
            let action: String;

            let row = match action_view {
                penumbra_sdk_transaction::ActionView::Transfer(transfer) => match transfer {
                    penumbra_sdk_transaction::view::action_view::TransferView::Visible {
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
                    penumbra_sdk_transaction::view::action_view::TransferView::Opaque {
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
                penumbra_sdk_transaction::ActionView::Consolidate(consolidate) => match consolidate
                {
                    penumbra_sdk_transaction::view::action_view::ConsolidateView::Visible {
                        consolidate: _,
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
                            action = "<empty consolidate>".to_string();
                        }
                        ["Consolidate", &action]
                    }
                    penumbra_sdk_transaction::view::action_view::ConsolidateView::Opaque {
                        consolidate,
                    } => {
                        if let Some(first_output) = consolidate.body.outputs.first() {
                            let bytes = first_output.note_payload.encrypted_note.0;
                            action = format_opaque_bytes(&bytes);
                        } else {
                            action = "<empty consolidate>".to_string();
                        }
                        ["Consolidate", &action]
                    }
                },
                penumbra_sdk_transaction::ActionView::Split(split) => match split {
                    penumbra_sdk_transaction::view::action_view::SplitView::Visible {
                        split: _,
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
                            action = "<empty split>".to_string();
                        }
                        ["Split", &action]
                    }
                    penumbra_sdk_transaction::view::action_view::SplitView::Opaque { split } => {
                        if let Some(first_output) = split.body.outputs.first() {
                            let bytes = first_output.note_payload.encrypted_note.0;
                            action = format_opaque_bytes(&bytes);
                        } else {
                            action = "<empty split>".to_string();
                        }
                        ["Split", &action]
                    }
                },
                penumbra_sdk_transaction::ActionView::ShieldedIcs20Withdrawal(withdrawal) => {
                    let withdrawal = match withdrawal {
                        penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalView::Visible {
                            withdrawal,
                            ..
                        } => &withdrawal.body.withdrawal,
                        penumbra_sdk_shielded_pool::ShieldedIcs20WithdrawalView::Opaque {
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
                penumbra_sdk_transaction::ActionView::ProposalSubmit(proposal_submit) => {
                    action = format!(
                        "Submit Governance Proposal #{}",
                        proposal_submit.proposal().id
                    );
                    [&action, ""]
                }
                penumbra_sdk_transaction::ActionView::IbcRelay(_) => ["IBC Relay", ""],
                penumbra_sdk_transaction::ActionView::ValidatorDefinition(_) => {
                    ["Upload Validator Definition", ""]
                }
                penumbra_sdk_transaction::ActionView::ValidatorVote(_) => ["Validator Vote", ""],
                penumbra_sdk_transaction::ActionView::ComplianceRegisterAsset(x) => {
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
                penumbra_sdk_transaction::ActionView::ComplianceRegisterUser(x) => {
                    action = format!("Register user for asset {}", x.leaf.asset_id);
                    ["Compliance: Register User", &action]
                }
                penumbra_sdk_transaction::ActionView::AggregateBundle(_) => {
                    ["Aggregate Bundle", ""]
                }
            };

            actions_table.add_row(row);
        }

        // Print table of actions and their descriptions
        println!("{actions_table}");
    }
}
