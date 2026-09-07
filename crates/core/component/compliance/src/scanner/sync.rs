use shieldd_sdk_proto::core::transaction::v1::Transaction as ProtoTransaction;
use shieldd_sdk_proto::Message;

use super::types::{
    ActionRef, ClearFlowEvent, ClearFlowKind, ComplianceCiphertextKind, ComplianceRecordRef,
    ExtractedComplianceCiphertext, OutputRef, PublicWithdrawalData, TxRef,
};

pub fn extract_compliance_ciphertexts(
    tx_ref: &TxRef,
    tx: &ProtoTransaction,
) -> Vec<ExtractedComplianceCiphertext> {
    use shieldd_sdk_proto::core::transaction::v1::action::Action;

    let Some(body) = &tx.body else {
        return vec![];
    };

    let mut results = Vec::new();
    for (action_index, action) in body.actions.iter().enumerate() {
        let action_ref = ActionRef {
            tx: tx_ref.clone(),
            action_index: action_index as u32,
        };
        match action.action.as_ref() {
            Some(Action::Transfer(transfer)) => {
                let Some(body) = transfer.body.as_ref() else {
                    continue;
                };
                let Some(routing_tags) = body.routing.as_ref().and_then(|routing| {
                    let tags: Vec<u32> = routing.tags.iter().map(|tag| tag.value).collect();
                    tags.try_into().ok()
                }) else {
                    continue;
                };
                for (output_index, output) in body.outputs.iter().enumerate() {
                    if output.compliance_ciphertext.is_empty() {
                        continue;
                    }
                    results.push(ExtractedComplianceCiphertext {
                        record_ref: ComplianceRecordRef::TransferOutput(OutputRef {
                            action: action_ref.clone(),
                            output_index: output_index as u32,
                        }),
                        kind: ComplianceCiphertextKind::Transfer,
                        routing_tags,
                        raw_bytes: output.compliance_ciphertext.clone(),
                        metadata_bytes: (!output.compliance_metadata.is_empty())
                            .then(|| output.compliance_metadata.clone()),
                        public_withdrawal: None,
                    });
                }
            }
            Some(Action::ShieldedHostWithdrawal(withdrawal)) => {
                let Some(body) = withdrawal.body.as_ref() else {
                    continue;
                };
                let Some(public_withdrawal) = host_withdrawal_data(body) else {
                    continue;
                };
                results.push(ExtractedComplianceCiphertext {
                    record_ref: ComplianceRecordRef::HostWithdrawal(action_ref),
                    kind: ComplianceCiphertextKind::Withdrawal,
                    routing_tags: [body.routing_tag.as_ref().map_or(0, |tag| tag.value), 0],
                    raw_bytes: body.withdrawal_compliance_ciphertext.clone(),
                    metadata_bytes: None,
                    public_withdrawal: Some(public_withdrawal),
                });
            }
            Some(Action::ShieldedIcs20Withdrawal(withdrawal)) => {
                let Some(body) = withdrawal.body.as_ref() else {
                    continue;
                };
                let Some(public_withdrawal) = ics20_withdrawal_data(body) else {
                    continue;
                };
                results.push(ExtractedComplianceCiphertext {
                    record_ref: ComplianceRecordRef::Ics20Withdrawal(action_ref),
                    kind: ComplianceCiphertextKind::Withdrawal,
                    routing_tags: [body.routing_tag.as_ref().map_or(0, |tag| tag.value), 0],
                    raw_bytes: body.withdrawal_compliance_ciphertext.clone(),
                    metadata_bytes: None,
                    public_withdrawal: Some(public_withdrawal),
                });
            }
            _ => {}
        }
    }

    results
}

fn host_withdrawal_data(
    body: &shieldd_sdk_proto::core::component::shielded_pool::v1::ShieldedHostWithdrawalBody,
) -> Option<PublicWithdrawalData> {
    use shieldd_sdk_proto::core::component::shielded_pool::v1::host_withdrawal::Destination;

    let withdrawal = body.withdrawal.as_ref()?;
    let value: shieldd_sdk_asset::Value = withdrawal.value.clone()?.try_into().ok()?;
    let (self_address, destination) = match withdrawal.destination.as_ref()? {
        Destination::Transfer(transfer) => (None, transfer.recipient.clone()),
        Destination::Execution(execution) => {
            let contracts = execution
                .calls
                .iter()
                .map(|call| hex::encode(&call.contract))
                .collect::<Vec<_>>()
                .join(",");
            (
                Some(execution.refund_address.clone()),
                format!("host-execution:{contracts}"),
            )
        }
    };
    Some(PublicWithdrawalData {
        asset_id: value.asset_id,
        amount: value.amount,
        self_address,
        destination,
    })
}

fn ics20_withdrawal_data(
    body: &shieldd_sdk_proto::core::component::shielded_pool::v1::ShieldedIcs20WithdrawalBody,
) -> Option<PublicWithdrawalData> {
    let withdrawal = body.withdrawal.as_ref()?;
    let amount = withdrawal.amount?.try_into().ok()?;
    let denom: shieldd_sdk_asset::asset::Metadata =
        withdrawal.denom.as_ref()?.denom.as_str().try_into().ok()?;
    let self_address = withdrawal
        .return_address
        .clone()
        .and_then(|address| shieldd_sdk_keys::Address::try_from(address).ok())
        .map(|address| address.to_string());
    Some(PublicWithdrawalData {
        asset_id: denom.id(),
        amount,
        self_address,
        destination: withdrawal.destination_chain_address.clone(),
    })
}

pub fn extract_clear_flows(tx_ref: &TxRef, tx: &ProtoTransaction) -> Vec<ClearFlowEvent> {
    use shieldd_sdk_proto::core::transaction::v1::action::Action;

    let Some(body) = &tx.body else {
        return vec![];
    };

    let mut results = Vec::new();
    for (action_index, action) in body.actions.iter().enumerate() {
        let output_ref = OutputRef {
            action: ActionRef {
                tx: tx_ref.clone(),
                action_index: action_index as u32,
            },
            output_index: 0,
        };

        match action.action.as_ref() {
            Some(Action::IbcRelayAction(relay)) => {
                if let Some(event) = extract_ibc_shield(&output_ref, relay) {
                    results.push(event);
                }
            }
            Some(Action::ShieldedIcs20Withdrawal(withdrawal)) => {
                if let Some(event) = extract_ics20_withdrawal(&output_ref, withdrawal) {
                    results.push(event);
                }
            }
            _ => {}
        }
    }

    results
}

fn extract_ibc_shield(
    output_ref: &OutputRef,
    relay: &shieldd_sdk_proto::core::component::ibc::v1::IbcRelay,
) -> Option<ClearFlowEvent> {
    let raw_action = relay.raw_action.as_ref()?;
    if !raw_action
        .type_url
        .ends_with("ibc.core.channel.v1.MsgRecvPacket")
    {
        return None;
    }
    let msg =
        ibc_proto::ibc::core::channel::v1::MsgRecvPacket::decode(raw_action.value.as_ref()).ok()?;
    let packet = msg.packet?;
    let packet_data: shieldd_sdk_proto::core::component::ibc::v1::FungibleTokenPacketData =
        serde_json::from_slice(packet.data.as_slice()).ok()?;
    let metadata: shieldd_sdk_asset::asset::Metadata =
        packet_data.denom.as_str().try_into().ok()?;
    let amount: shieldd_sdk_num::Amount = packet_data.amount.try_into().ok()?;

    Some(ClearFlowEvent {
        output_ref: output_ref.clone(),
        kind: ClearFlowKind::Shield,
        asset_id: metadata.id(),
        amount,
        self_address: Some(packet_data.receiver),
        counterparty: Some(packet_data.sender.clone()),
        public_address: Some(packet_data.sender),
    })
}

fn extract_ics20_withdrawal(
    output_ref: &OutputRef,
    withdrawal: &shieldd_sdk_proto::core::component::shielded_pool::v1::ShieldedIcs20Withdrawal,
) -> Option<ClearFlowEvent> {
    let body = withdrawal.body.as_ref()?;
    let payload = body.withdrawal.as_ref()?;
    let denom = payload.denom.clone()?;
    let metadata: shieldd_sdk_asset::asset::Metadata = denom.denom.as_str().try_into().ok()?;
    let amount: shieldd_sdk_num::Amount = payload.amount?.try_into().ok()?;
    let return_address = payload
        .return_address
        .clone()
        .and_then(|address| shieldd_sdk_keys::Address::try_from(address).ok())
        .map(|address| address.to_string());

    Some(ClearFlowEvent {
        output_ref: output_ref.clone(),
        kind: ClearFlowKind::Withdraw,
        asset_id: metadata.id(),
        amount,
        self_address: return_address,
        counterparty: Some(payload.destination_chain_address.clone()),
        public_address: Some(payload.destination_chain_address.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shieldd_sdk_proto::core::component::shielded_pool::v1::{
        NoteReshape, NoteReshapeBody, RoutingTag, Transfer, TransferBody, TransferOutputBody,
        TransferRouting,
    };
    use shieldd_sdk_proto::core::transaction::v1::{
        action::Action, Action as ActionProto, TransactionBody,
    };
    use shieldd_sdk_txhash::TransactionId;

    fn tx_ref() -> TxRef {
        TxRef {
            block: crate::scanner::BlockRef {
                height: 10,
                block_hash: [1u8; 32],
                parent_hash: [2u8; 32],
                block_time_unix: None,
            },
            tx_index: 3,
            tx_hash: TransactionId([4u8; 32]),
        }
    }

    #[test]
    fn extract_empty_transaction() {
        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(extract_compliance_ciphertexts(&tx_ref(), &tx).is_empty());
    }

    #[test]
    fn extract_ciphertexts_includes_action_and_output_indices() {
        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![
                    ActionProto {
                        action: Some(Action::NoteReshape(NoteReshape {
                            body: Some(NoteReshapeBody::default()),
                            ..Default::default()
                        })),
                    },
                    ActionProto {
                        action: Some(Action::Transfer(Transfer {
                            body: Some(TransferBody {
                                routing: Some(TransferRouting {
                                    tags: vec![RoutingTag { value: 11 }, RoutingTag { value: 22 }],
                                }),
                                outputs: vec![
                                    TransferOutputBody::default(),
                                    TransferOutputBody {
                                        compliance_ciphertext: vec![1, 2, 3, 4],
                                        compliance_metadata: vec![9, 9],
                                        ..Default::default()
                                    },
                                    TransferOutputBody {
                                        compliance_ciphertext: vec![5, 6],
                                        ..Default::default()
                                    },
                                ],
                                ..Default::default()
                            }),
                            ..Default::default()
                        })),
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };

        let extracted = extract_compliance_ciphertexts(&tx_ref(), &tx);
        assert_eq!(extracted.len(), 2);
        assert_eq!(extracted[0].record_ref.action().action_index, 1);
        assert_eq!(extracted[0].record_ref.output_index(), 1);
        assert_eq!(extracted[0].metadata_bytes, Some(vec![9, 9]));
        assert_eq!(extracted[0].raw_bytes, vec![1, 2, 3, 4]);
        assert_eq!(extracted[0].routing_tags, [11, 22]);
        assert_eq!(extracted[1].record_ref.action().action_index, 1);
        assert_eq!(extracted[1].record_ref.output_index(), 2);
        assert_eq!(extracted[1].raw_bytes, vec![5, 6]);
        assert_eq!(extracted[1].routing_tags, [11, 22]);
    }

    #[test]
    fn extract_ciphertexts_uses_typed_withdrawal_refs_and_public_facts() {
        use shieldd_sdk_proto::core::component::{
            ibc::v1::Ics20Withdrawal,
            shielded_pool::v1::{
                host_withdrawal::Destination, HostTransfer, HostWithdrawal, ShieldedHostWithdrawal,
                ShieldedHostWithdrawalBody, ShieldedIcs20Withdrawal, ShieldedIcs20WithdrawalBody,
            },
        };
        let asset_id = shieldd_sdk_asset::asset::Id(decaf377::Fq::from(77u64));
        let tx = ProtoTransaction {
            body: Some(TransactionBody {
                actions: vec![
                    ActionProto {
                        action: Some(Action::ShieldedHostWithdrawal(ShieldedHostWithdrawal {
                            body: Some(ShieldedHostWithdrawalBody {
                                withdrawal: Some(HostWithdrawal {
                                    value: Some(
                                        shieldd_sdk_asset::Value {
                                            amount: 12u64.into(),
                                            asset_id,
                                        }
                                        .into(),
                                    ),
                                    destination: Some(Destination::Transfer(HostTransfer {
                                        recipient: "0xabc".to_owned(),
                                    })),
                                }),
                                routing_tag: Some(RoutingTag { value: 31 }),
                                withdrawal_compliance_ciphertext: vec![1; 192],

                                ..Default::default()
                            }),
                            ..Default::default()
                        })),
                    },
                    ActionProto {
                        action: Some(Action::ShieldedIcs20Withdrawal(ShieldedIcs20Withdrawal {
                            body: Some(ShieldedIcs20WithdrawalBody {
                                withdrawal: Some(Ics20Withdrawal {
                                    amount: Some(shieldd_sdk_num::Amount::from(34u64).into()),
                                    denom: Some(shieldd_sdk_proto::core::asset::v1::Denom {
                                        denom: "utest".to_owned(),
                                    }),
                                    destination_chain_address: "cosmos1dest".to_owned(),
                                    ..Default::default()
                                }),
                                routing_tag: Some(RoutingTag { value: 41 }),
                                withdrawal_compliance_ciphertext: vec![3; 192],

                                ..Default::default()
                            }),
                            ..Default::default()
                        })),
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };

        let extracted = extract_compliance_ciphertexts(&tx_ref(), &tx);
        assert_eq!(extracted.len(), 2);
        assert!(matches!(
            extracted[0].record_ref,
            ComplianceRecordRef::HostWithdrawal(_)
        ));
        assert_eq!(extracted[0].routing_tags, [31, 0]);
        assert_eq!(
            extracted[0].public_withdrawal.as_ref().unwrap().amount,
            12u64.into()
        );
        assert_eq!(
            extracted[0].public_withdrawal.as_ref().unwrap().destination,
            "0xabc"
        );
        assert!(matches!(
            extracted[1].record_ref,
            ComplianceRecordRef::Ics20Withdrawal(_)
        ));
        assert_eq!(extracted[1].routing_tags, [41, 0]);
        assert_eq!(
            extracted[1].public_withdrawal.as_ref().unwrap().amount,
            34u64.into()
        );
        assert_eq!(
            extracted[1].public_withdrawal.as_ref().unwrap().destination,
            "cosmos1dest"
        );
    }
}
