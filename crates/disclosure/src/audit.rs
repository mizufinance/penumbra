use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use shieldd_sdk_compliance::{TransferComplianceCiphertext, TransferComplianceMetadata};
use shieldd_sdk_transaction::Action;

use crate::{AcceptedBlock, ActionRef, OutputRef};
pub use shieldd_sdk_compliance::master_wrapping::MasterSelection;
pub use shieldd_sdk_compliance::transfer_audit::TransferTier;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuditAccess {
    General { value: MasterSelection },
    NamedPerson { tier: TransferTier, address: String },
}

impl AuditAccess {
    pub fn tier(&self) -> TransferTier {
        match self {
            Self::General { value } => value.tier(),
            Self::NamedPerson { tier, .. } => *tier,
        }
    }

    pub fn key_scope(&self) -> Result<AuditKeyScope> {
        match self {
            Self::General { .. } => Ok(AuditKeyScope::General),
            Self::NamedPerson { address, .. } => {
                let address: shieldd_sdk_keys::Address = address
                    .parse()
                    .context("invalid named-person audit address")?;
                Ok(AuditKeyScope::Person {
                    identity: address.to_vec(),
                })
            }
        }
    }

    pub fn key_field(&self) -> AuditKeyField {
        match self {
            Self::General {
                value: MasterSelection::Amount,
            } => AuditKeyField::Amount,
            Self::General {
                value: MasterSelection::Sender,
            } => AuditKeyField::Sender,
            Self::General {
                value: MasterSelection::Receiver,
            } => AuditKeyField::Receiver,
            Self::NamedPerson {
                tier: TransferTier::SenderCore | TransferTier::OutputCore,
                ..
            } => AuditKeyField::Amount,
            Self::NamedPerson {
                tier: TransferTier::SenderExt,
                ..
            } => AuditKeyField::Receiver,
            Self::NamedPerson {
                tier: TransferTier::OutputExt,
                ..
            } => AuditKeyField::Sender,
        }
    }
}

/// Public LaKey identity passed to Orbis; the PRF is evaluated only by its MPC nodes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditKeyIdentity {
    pub chain: String,
    pub ring: String,
    pub epoch: u64,
    pub scope: AuditKeyScope,
    pub field: AuditKeyField,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuditKeyScope {
    General,
    Person { identity: Vec<u8> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditKeyField {
    Amount,
    Sender,
    Receiver,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditSelection {
    pub version: u32,
    pub chain_id: String,
    pub reference: OutputRef,
    pub access: AuditAccess,
    pub policy: AuditPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditPolicy {
    pub ring_id: String,
    pub policy_id: String,
    pub resource: String,
    pub permission: String,
}

/// Public transaction bytes selected from independently fetched committed node data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AcceptedAuditCiphertext {
    pub selection: AuditSelection,
    pub ciphertext: Vec<u8>,
    pub metadata: TransferComplianceMetadata,
    pub epk: [u8; 32],
    pub wrapping: [u8; 32],
    pub identity: AuditKeyIdentity,
    pub object_id: String,
}

/// Indexed bytes must exactly match a canonical transaction accepted by the chosen node.
pub fn verify_audit_candidate(
    selection: &AuditSelection,
    block: &AcceptedBlock,
    raw: &[u8],
) -> Result<()> {
    use shieldd_sdk_proto::DomainType;
    let candidate = shieldd_sdk_transaction::Transaction::decode_canonical(raw)?;
    ensure!(
        candidate.id().to_string() == selection.reference.transaction_id,
        "indexed transaction ID mismatch"
    );
    ensure!(
        block.height == selection.reference.height,
        "indexed transaction height mismatch"
    );
    let accepted = block
        .transactions
        .iter()
        .find(|tx| tx.id() == candidate.id())
        .context("indexed transaction not accepted")?;
    ensure!(
        accepted.encode_to_vec() == raw,
        "indexed transaction bytes differ from accepted transaction"
    );
    Ok(())
}

/// Node access remains outside this function; caller supplies its chosen node's data.
pub fn accepted_audit_ciphertext(
    selection: AuditSelection,
    node_chain_id: &str,
    block: &AcceptedBlock,
) -> Result<AcceptedAuditCiphertext> {
    ensure!(
        selection.version == 2,
        "unsupported audit selection version"
    );
    ensure!(
        !selection.chain_id.is_empty() && selection.chain_id == node_chain_id,
        "audit chain mismatch"
    );
    ensure!(
        selection.reference.height > 0 && selection.reference.height == block.height,
        "audit height mismatch"
    );
    let tx = block
        .transactions
        .iter()
        .find(|tx| tx.id().to_string() == selection.reference.transaction_id)
        .context("audit transaction not accepted at supplied height")?;
    ensure!(
        tx.transaction_parameters().chain_id == node_chain_id,
        "transaction chain mismatch"
    );
    let ActionRef::Body(action) = selection.reference.action else {
        anyhow::bail!("compliance audit requires an ordinary Transfer");
    };
    let Action::Transfer(transfer) = tx
        .actions()
        .nth(action as usize)
        .context("audit action unavailable")?
    else {
        anyhow::bail!("compliance audit requires a Transfer");
    };
    transfer.body.validate_shape()?;
    ensure!(
        transfer.body.proof_context == shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
        "audit requires ordinary Transfer context"
    );
    ensure!(
        selection.reference.output == 0,
        "only the receiver output carries Transfer compliance data"
    );
    let output = transfer
        .body
        .outputs
        .first()
        .context("audit output unavailable")?;
    let ct = TransferComplianceCiphertext::from_bytes(&output.compliance_ciphertext)?;
    let metadata = TransferComplianceMetadata::from_bytes(&output.compliance_metadata)?;
    let policy = &selection.policy;
    for (identifier, expected) in [
        (&policy.ring_id, metadata.ring_id_hash()?),
        (&policy.policy_id, metadata.policy_id_hash()?),
        (&policy.resource, metadata.resource_hash()?),
        (&policy.permission, metadata.permission_hash()?),
    ] {
        ensure!(
            !identifier.is_empty() && identifier.len() <= 1024,
            "invalid audit policy identifier"
        );
        ensure!(
            shieldd_sdk_compliance::indexed_tree::string_to_fq(identifier) == expected,
            "audit policy does not match accepted transaction"
        );
    }
    ensure!(
        metadata.audit_epoch != 0,
        "unregulated transaction has no Orbis audit keys"
    );
    let identity = AuditKeyIdentity {
        chain: selection.chain_id.clone(),
        ring: policy.ring_id.clone(),
        epoch: metadata.audit_epoch,
        scope: selection.access.key_scope()?,
        field: selection.access.key_field(),
    };
    let tier = selection.access.tier().select(&ct, &metadata)?;
    let epk = tier.epk.vartime_compress().0;
    let wrapping = match &selection.access {
        AuditAccess::General { value } => ct.master_wrappings[*value as usize],
        AuditAccess::NamedPerson { .. } => tier.c2,
    }
    .to_bytes();
    let scope = match &selection.access {
        AuditAccess::General { value } => format!("general:{}", *value as u8),
        AuditAccess::NamedPerson { tier, .. } => {
            let tier = match tier {
                TransferTier::SenderCore => "sender_core",
                TransferTier::SenderExt => "sender_ext",
                TransferTier::OutputCore => "output_core",
                TransferTier::OutputExt => "output_ext",
            };
            format!(
                "named:{tier}:{}",
                match &identity.scope {
                    AuditKeyScope::Person { identity } => hex::encode(identity),
                    AuditKeyScope::General => unreachable!("named access has person scope"),
                }
            )
        }
    };
    let object_id = format!(
        "shieldd:{}:{}:{action}:0:{scope}",
        hex::encode(selection.chain_id.as_bytes()),
        tx.id()
    );
    Ok(AcceptedAuditCiphertext {
        selection,
        ciphertext: output.compliance_ciphertext.clone(),
        metadata,
        epk,
        wrapping,
        identity,
        object_id,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecodedAuditValue {
    Amount {
        base_units: String,
    },
    AddressComponents {
        diversified_generator: [u8; 32],
        transmission_key: [u8; 32],
    },
}

/// Decodes using an already verified PRE shared point; decoding does not verify PRE evidence.
pub fn decode_audit_ciphertext(
    accepted: &AcceptedAuditCiphertext,
    shared_point: [u8; 32],
) -> Result<DecodedAuditValue> {
    use shieldd_sdk_compliance::transfer_audit::TransferAuditData;
    let shared = decaf377::Encoding(shared_point)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid audit shared point"))?;
    let ct = TransferComplianceCiphertext::from_bytes(&accepted.ciphertext)?;
    let value = match &accepted.selection.access {
        AuditAccess::General { value } => value.decrypt(&ct, &accepted.metadata, &shared)?,
        AuditAccess::NamedPerson { tier, .. } => {
            tier.select(&ct, &accepted.metadata)?.decrypt(&shared)?
        }
    };
    Ok(match value {
        TransferAuditData::Amount(amount) => DecodedAuditValue::Amount {
            base_units: amount.to_string(),
        },
        TransferAuditData::Counterparty(address) => DecodedAuditValue::AddressComponents {
            diversified_generator: address.diversified_generator.vartime_compress().0,
            transmission_key: address.transmission_key,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_chain_height_version_and_missing_acceptance() {
        let selection = AuditSelection {
            version: 2,
            chain_id: "chain".to_owned(),
            reference: OutputRef {
                transaction_id: "00".repeat(32),
                height: 1,
                action: ActionRef::Body(0),
                output: 0,
            },
            access: AuditAccess::General {
                value: MasterSelection::Amount,
            },
            policy: AuditPolicy {
                ring_id: "ring".into(),
                policy_id: "policy".into(),
                resource: "transaction".into(),
                permission: "read".into(),
            },
        };
        let block = AcceptedBlock {
            height: 1,
            transactions: Vec::new(),
        };
        assert!(
            accepted_audit_ciphertext(selection.clone(), "other-chain", &block)
                .unwrap_err()
                .to_string()
                .contains("chain mismatch")
        );
        let mut changed = selection.clone();
        changed.version = 1;
        assert!(accepted_audit_ciphertext(changed, "chain", &block)
            .unwrap_err()
            .to_string()
            .contains("unsupported"));
        let mut changed = selection.clone();
        changed.reference.height = 2;
        assert!(accepted_audit_ciphertext(changed, "chain", &block)
            .unwrap_err()
            .to_string()
            .contains("height mismatch"));
        assert!(accepted_audit_ciphertext(selection, "chain", &block)
            .unwrap_err()
            .to_string()
            .contains("not accepted"));
    }

    #[test]
    fn indexed_bytes_require_exact_canonical_accepted_transaction() {
        let tx = shieldd_sdk_transaction::Transaction::default();
        let raw: Vec<u8> = (&tx).into();
        let mut selection = AuditSelection {
            version: 2,
            chain_id: "chain".into(),
            reference: OutputRef {
                transaction_id: tx.id().to_string(),
                height: 7,
                action: ActionRef::Body(0),
                output: 0,
            },
            access: AuditAccess::General {
                value: MasterSelection::Amount,
            },
            policy: AuditPolicy {
                ring_id: "ring".into(),
                policy_id: "policy".into(),
                resource: "transaction".into(),
                permission: "read".into(),
            },
        };
        let mut block = AcceptedBlock {
            height: 7,
            transactions: vec![tx],
        };
        verify_audit_candidate(&selection, &block, &raw).unwrap();
        let mut altered = raw.clone();
        altered.extend_from_slice(&[0xf8, 0x07, 0x00]);
        assert!(verify_audit_candidate(&selection, &block, &altered).is_err());
        selection.reference.transaction_id = "00".repeat(32);
        assert!(verify_audit_candidate(&selection, &block, &raw).is_err());
        selection.reference.transaction_id = block.transactions[0].id().to_string();
        block.height += 1;
        assert!(verify_audit_candidate(&selection, &block, &raw).is_err());
        block.height -= 1;
        block.transactions.clear();
        assert!(verify_audit_candidate(&selection, &block, &raw).is_err());
    }
}
