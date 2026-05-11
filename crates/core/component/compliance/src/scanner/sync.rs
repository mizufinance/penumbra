use penumbra_sdk_proto::core::transaction::v1::Transaction as ProtoTransaction;
use penumbra_sdk_proto::Message;

use super::types::{
    ActionRef, ClearFlowEvent, ClearFlowKind, ExtractedComplianceCiphertext, OutputRef, TxRef,
};

pub fn extract_compliance_ciphertexts(
    tx_ref: &TxRef,
    tx: &ProtoTransaction,
) -> Vec<ExtractedComplianceCiphertext> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action;

    let Some(body) = &tx.body else {
        return vec![];
    };

    let mut results = Vec::new();
    for (action_index, action) in body.actions.iter().enumerate() {
        let Some(Action::Transfer(transfer)) = action.action.as_ref() else {
            continue;
        };
        let Some(body) = transfer.body.as_ref() else {
            continue;
        };

        for (output_index, output) in body.outputs.iter().enumerate() {
            if output.compliance_ciphertext.is_empty() {
                continue;
            }
            results.push(ExtractedComplianceCiphertext {
                output_ref: OutputRef {
                    action: ActionRef {
                        tx: tx_ref.clone(),
                        action_index: action_index as u32,
                    },
                    output_index: output_index as u32,
                },
                raw_bytes: output.compliance_ciphertext.clone(),
            });
        }
    }

    results
}

pub fn extract_clear_flows(tx_ref: &TxRef, tx: &ProtoTransaction) -> Vec<ClearFlowEvent> {
    use penumbra_sdk_proto::core::transaction::v1::action::Action;

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
    relay: &penumbra_sdk_proto::core::component::ibc::v1::IbcRelay,
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
    let packet_data: penumbra_sdk_proto::core::component::ibc::v1::FungibleTokenPacketData =
        serde_json::from_slice(packet.data.as_slice()).ok()?;
    let metadata: penumbra_sdk_asset::asset::Metadata =
        packet_data.denom.as_str().try_into().ok()?;
    let amount: penumbra_sdk_num::Amount = packet_data.amount.try_into().ok()?;

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
    withdrawal: &penumbra_sdk_proto::core::component::shielded_pool::v1::ShieldedIcs20Withdrawal,
) -> Option<ClearFlowEvent> {
    let body = withdrawal.body.as_ref()?;
    let payload = body.withdrawal.as_ref()?;
    let denom = payload.denom.clone()?;
    let metadata: penumbra_sdk_asset::asset::Metadata = denom.denom.as_str().try_into().ok()?;
    let amount: penumbra_sdk_num::Amount = payload.amount.clone()?.try_into().ok()?;
    let return_address = payload
        .return_address
        .clone()
        .and_then(|address| penumbra_sdk_keys::Address::try_from(address).ok())
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
    use penumbra_sdk_proto::core::component::shielded_pool::v1::{
        Consolidate, ConsolidateBody, Split, SplitBody, Transfer, TransferBody, TransferOutputBody,
    };
    use penumbra_sdk_proto::core::transaction::v1::{
        action::Action, Action as ActionProto, TransactionBody,
    };
    use penumbra_sdk_txhash::TransactionId;

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
                        action: Some(Action::Split(Split {
                            body: Some(SplitBody::default()),
                            ..Default::default()
                        })),
                    },
                    ActionProto {
                        action: Some(Action::Transfer(Transfer {
                            body: Some(TransferBody {
                                outputs: vec![
                                    TransferOutputBody::default(),
                                    TransferOutputBody {
                                        compliance_ciphertext: vec![1, 2, 3, 4],
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
                    ActionProto {
                        action: Some(Action::Consolidate(Consolidate {
                            body: Some(ConsolidateBody::default()),
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
        assert_eq!(extracted[0].output_ref.action.action_index, 1);
        assert_eq!(extracted[0].output_ref.output_index, 1);
        assert_eq!(extracted[0].raw_bytes, vec![1, 2, 3, 4]);
        assert_eq!(extracted[1].output_ref.action.action_index, 1);
        assert_eq!(extracted[1].output_ref.output_index, 2);
        assert_eq!(extracted[1].raw_bytes, vec![5, 6]);
    }
}
