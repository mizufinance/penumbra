use crate::{AuditKeyField, AuditKeyIdentity, AuditKeyScope};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use shieldd_sdk_compliance::{
    registration::{policy_from_asset_grant, validate_user_grant},
    structs::OrbisCapabilityCertificate,
    AssetPolicy, AuditKeys, MsgRegisterAsset, MsgRegisterUser,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuditRegistration {
    Person { action: MsgRegisterUser },
    General { action: MsgRegisterAsset },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditRegistrationRequest {
    pub version: u32,
    pub chain_id: String,
    pub registration: AuditRegistration,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegisteredAuditKey {
    pub identity: AuditKeyIdentity,
    pub public_key: [u8; 32],
}

/// Exact certificate statement and expected MPC keys, reconstructed from validated registration data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditRegistrationStatement {
    pub version: u32,
    pub object_id: String,
    pub ring_id: String,
    pub ring_public_key: [u8; 32],
    pub keys: [RegisteredAuditKey; 3],
    pub message: Vec<u8>,
}

fn statement(
    chain: &str,
    asset_id: shieldd_sdk_asset::asset::Id,
    policy: &AssetPolicy,
    scope: AuditKeyScope,
    keys: &AuditKeys,
    message: Vec<u8>,
) -> AuditRegistrationStatement {
    let fields = [
        AuditKeyField::Amount,
        AuditKeyField::Sender,
        AuditKeyField::Receiver,
    ];
    let points = [keys.amount, keys.sender, keys.receiver];
    AuditRegistrationStatement {
        version: 1,
        object_id: format!(
            "shieldd:registration:{}:{}",
            hex::encode(chain.as_bytes()),
            asset_id
        ),
        ring_id: policy.ring.ring_id.clone(),
        ring_public_key: policy.ring.ring_pk.vartime_compress().0,
        keys: std::array::from_fn(|i| RegisteredAuditKey {
            identity: AuditKeyIdentity {
                chain: chain.to_owned(),
                ring: policy.ring.ring_id.clone(),
                epoch: keys.epoch,
                scope: scope.clone(),
                field: fields[i],
            },
            public_key: points[i].vartime_compress().0,
        }),
        message,
    }
}

/// The current asset policy and chain identity must come from the chosen node.
pub fn prepare_audit_registration(
    request: &AuditRegistrationRequest,
    node_chain_id: &str,
    current_unix: u64,
    current_asset_policy: Option<&AssetPolicy>,
) -> Result<AuditRegistrationStatement> {
    ensure!(
        request.version == 1,
        "unsupported audit registration version"
    );
    ensure!(
        !request.chain_id.is_empty() && request.chain_id == node_chain_id,
        "audit registration chain mismatch"
    );
    match &request.registration {
        AuditRegistration::Person { action } => {
            ensure!(
                action.capability_certificate.is_none(),
                "certificate issuance requires an unsigned registration"
            );
            let policy =
                current_asset_policy.context("current regulated asset policy unavailable")?;
            validate_user_grant(action, policy, current_unix)?;
            let message =
                OrbisCapabilityCertificate::signing_bytes(node_chain_id, &action.leaf, policy)?;
            Ok(statement(
                node_chain_id,
                action.leaf.asset_id,
                policy,
                AuditKeyScope::Person {
                    identity: action.leaf.address.to_vec(),
                },
                &action.leaf.audit_keys,
                message,
            ))
        }
        AuditRegistration::General { action } => {
            ensure!(
                action.audit_certificate.is_none(),
                "certificate issuance requires an unsigned registration"
            );
            ensure!(
                action.is_regulated,
                "unregulated assets have no general audit keys"
            );
            let policy = policy_from_asset_grant(action, current_unix)?;
            let message = OrbisCapabilityCertificate::general_signing_bytes(
                node_chain_id,
                action.asset_id,
                &policy,
            )?;
            Ok(statement(
                node_chain_id,
                action.asset_id,
                &policy,
                AuditKeyScope::General,
                &policy.ring.audit_keys,
                message,
            ))
        }
    }
}
