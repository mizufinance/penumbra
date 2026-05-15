use std::collections::HashSet;

use anyhow::{anyhow, bail, Context, Result};
use decaf377::Encoding;
use orbis_authn::{create_authenticated_request, JwtSigner};
use orbis_common::blockchain::{
    acp::{Actor, Object, Relationship, Subject, SubjectKind},
    SourceHubClient,
};
use orbis_proto::{
    dkg_service::{dkg_service_client::DkgServiceClient, StartDkgRequest},
    info_service::{info_service_client::InfoServiceClient, GetNodeInfoRequest},
    pre_service::{pre_service_client::PreServiceClient, StartPreRequest, TimestampRange},
    store_secret_service::{
        store_secret_service_client::StoreSecretServiceClient, StoreSecretRequest,
    },
};
use orbis_tonic::transport::Endpoint;
use serde::Deserialize;

use crate::types::{DkgResult, NodeInfo, PreResult, RingInfo, StoreSecretResult};
use penumbra_sdk_compliance::{OrbisEncryptedSeedUploadPackage, OrbisSecretEnvelope};

#[derive(Debug, Deserialize)]
struct RingPayload {
    ring_pk: String,
}

#[derive(Debug, Deserialize)]
struct PreResponse {
    xnc_cmt: String,
    secret: OrbisSecretEnvelope,
}

pub struct OrbisClient {
    endpoint: Endpoint,
}

impl OrbisClient {
    pub fn new(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        let endpoint = Endpoint::from_shared(endpoint.clone())
            .with_context(|| format!("invalid Orbis endpoint {endpoint:?}"))?;
        Ok(Self { endpoint })
    }

    pub async fn query_node_info(&self) -> Result<NodeInfo> {
        let channel = self
            .endpoint
            .clone()
            .connect()
            .await
            .map_err(|e| anyhow!("failed to connect to Orbis info endpoint: {}", e))?;
        let mut client = InfoServiceClient::new(channel);

        let response = client
            .get_node_info(GetNodeInfoRequest {})
            .await
            .map_err(|e| anyhow!("failed to query node info: {}", e))?;

        let node_info = response.into_inner();
        Ok(NodeInfo {
            public_address: node_info.public_address,
            peer_id: node_info.peer_id,
            p2p_address: node_info.p2p_address,
        })
    }

    pub async fn start_dkg(
        &self,
        threshold: u32,
        peer_ids: &[String],
        jwt_signer: &JwtSigner,
    ) -> Result<DkgResult> {
        let total_nodes = peer_ids.len() as u32;
        if threshold > total_nodes {
            bail!("threshold ({threshold}) cannot be greater than total nodes ({total_nodes})");
        }

        let channel = self
            .endpoint
            .clone()
            .connect()
            .await
            .map_err(|e| anyhow!("failed to connect to Orbis DKG endpoint: {}", e))?;
        let mut client = DkgServiceClient::new(channel);

        let request = StartDkgRequest {
            threshold,
            peer_ids: peer_ids.to_vec(),
            pss_interval: None,
        };
        let token = jwt_signer
            .create_dkg_jwt(threshold, peer_ids, None)
            .map_err(|e| anyhow!("failed to create DKG JWT: {}", e))?;
        let request = create_authenticated_request(request, &token)
            .map_err(|e| anyhow!("failed to create authenticated DKG request: {}", e))?;

        let response = client
            .start_dkg(request)
            .await
            .map_err(|e| anyhow!("DKG request failed: {}", e))?
            .into_inner();

        Ok(DkgResult {
            session_id: response.session_id,
            status: response.status,
            message: response.message,
        })
    }

    pub async fn register_bulletin_namespace(
        client: &SourceHubClient,
        namespace: &str,
    ) -> Result<()> {
        if client.bulletin_get_namespace(namespace).await.is_ok() {
            return Ok(());
        }

        match client.bulletin_register_namespace(namespace).await {
            Ok(result) if result.code == 0 => Ok(()),
            Ok(result) => {
                let log = result.log;
                if log.contains("already exists") || log.contains("namespace already exists") {
                    Ok(())
                } else {
                    bail!(
                        "register namespace tx failed: code={} log={log}",
                        result.code
                    )
                }
            }
            Err(error) => {
                let msg = error.to_string();
                if msg.contains("already exists") || msg.contains("namespace already exists") {
                    Ok(())
                } else {
                    Err(anyhow!("failed to register bulletin namespace: {}", error))
                }
            }
        }
    }

    pub async fn add_bulletin_collaborator(
        client: &SourceHubClient,
        namespace: &str,
        collaborator_address: &str,
    ) -> Result<()> {
        match client
            .bulletin_add_collaborator(namespace, collaborator_address)
            .await
        {
            Ok(result) if result.code == 0 => Ok(()),
            Ok(result) => {
                let log = result.log;
                if log.contains("already exists") || log.contains("collaborator already exists") {
                    Ok(())
                } else {
                    bail!("add collaborator tx failed: code={} log={log}", result.code)
                }
            }
            Err(error) => {
                let msg = error.to_string();
                if msg.contains("already exists") || msg.contains("collaborator already exists") {
                    Ok(())
                } else {
                    Err(anyhow!("failed to add collaborator: {}", error))
                }
            }
        }
    }

    pub async fn get_latest_ring(client: &SourceHubClient, namespace: &str) -> Result<RingInfo> {
        let posts = client
            .bulletin_list_posts(namespace)
            .await
            .map_err(|e| anyhow!("failed to list Orbis ring posts: {}", e))?;

        let (post, ring_payload) = posts
            .iter()
            .rev()
            .find_map(|post| {
                serde_json::from_slice::<RingPayload>(&post.payload)
                    .ok()
                    .map(|payload| (post, payload))
            })
            .ok_or_else(|| anyhow!("no Orbis ring posts found; run DKG first"))?;

        let ring_pk_hex = ring_payload.ring_pk;
        let bytes = hex::decode(&ring_pk_hex).context("invalid Orbis ring_pk hex")?;
        let bytes_arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow!("ring_pk should be 32 bytes"))?;
        let ring_pk = Encoding(bytes_arr)
            .vartime_decompress()
            .map_err(|_| anyhow!("invalid ring_pk encoding"))?;

        Ok(RingInfo {
            ring_id: post.id.clone(),
            ring_pk,
            ring_pk_hex,
        })
    }

    pub async fn add_policy(
        client: &SourceHubClient,
        policy_yaml: &str,
        marshal_type: i32,
        resource: &str,
        permission: &str,
    ) -> Result<String> {
        let existing_ids = client
            .acp_list_policy_ids()
            .await
            .map(|ids| ids.ids.into_iter().collect::<HashSet<_>>())
            .unwrap_or_default();

        let create_result = client
            .acp_create_policy(policy_yaml, marshal_type)
            .await
            .map_err(|e| anyhow!("failed to create policy: {}", e))?;
        if create_result.code != 0 {
            bail!(
                "create policy tx failed: code={} log={}",
                create_result.code,
                create_result.log
            );
        }

        let policy_ids = client
            .acp_list_policy_ids()
            .await
            .map_err(|e| anyhow!("failed to list policy IDs: {}", e))?;
        let mut candidate_ids = policy_ids
            .ids
            .iter()
            .filter(|id| !existing_ids.contains(*id))
            .cloned()
            .collect::<Vec<_>>();
        if candidate_ids.is_empty() {
            candidate_ids = policy_ids.ids.clone();
        }

        for policy_id in candidate_ids {
            if Self::policy_defines_resource(client, &policy_id, resource, permission).await? {
                return Ok(policy_id);
            }
        }

        bail!("created ACP policy, but could not find a policy defining {resource}/{permission}")
    }

    pub async fn store_encrypted_seed_package(
        &self,
        namespace: &str,
        ring_id: &str,
        package: &OrbisEncryptedSeedUploadPackage,
        jwt_signer: &JwtSigner,
    ) -> Result<StoreSecretResult> {
        let channel = self
            .endpoint
            .clone()
            .connect()
            .await
            .map_err(|e| anyhow!("failed to connect to Orbis store-secret endpoint: {}", e))?;
        let mut client = StoreSecretServiceClient::new(channel);

        let request = StoreSecretRequest {
            encrypted_document: package.encrypted_document.clone(),
            enc_cmt: package.enc_cmt.clone(),
            ring_id: ring_id.to_string(),
            namespace: namespace.to_string(),
            policy_id: package.policy_id.clone(),
            resource: package.resource.clone(),
            permission: package.permission.clone(),
            shared_point: package.shared_point.clone(),
            challenge: package.orbis_challenge.clone(),
            response: package.orbis_response.clone(),
            derived_pk: Some(package.derived_pk.clone()),
            with_proof: false,
            tier: Some(package.tier_label.clone()),
            timestamp: Some(package.timestamp),
            metadata_hash: Some(package.metadata_hash.clone()),
        };

        let token = jwt_signer
            .create_store_secret_jwt(
                package.encrypted_document.clone(),
                package.enc_cmt.clone(),
                ring_id,
                namespace,
                &package.policy_id,
                &package.resource,
                &package.permission,
                package.shared_point.clone(),
                package.orbis_challenge.clone(),
                package.orbis_response.clone(),
                Some(package.derived_pk.clone()),
                false,
                Some(package.tier_label.clone()),
                Some(package.timestamp),
                Some(package.metadata_hash.clone()),
            )
            .map_err(|e| anyhow!("failed to create Orbis store-secret JWT: {}", e))?;

        let response = client
            .store_secret(create_authenticated_request(request, &token)?)
            .await
            .map_err(|e| anyhow!("Orbis store-secret request failed: {}", e))?
            .into_inner();

        Ok(StoreSecretResult {
            status: response.status,
            message: response.message,
            created_at: response.created_at,
            object_id: response.object_id,
            ring_id: response.ring_id,
            signature: response.signature,
        })
    }

    pub async fn register_object(
        client: &SourceHubClient,
        policy_id: &str,
        resource: &str,
        object_id: &str,
    ) -> Result<()> {
        let document = Object {
            resource: resource.to_string(),
            id: object_id.to_string(),
        };

        match client.acp_register_object(policy_id, document).await {
            Ok(result) if result.code == 0 => Ok(()),
            Ok(result) => {
                let log = result.log;
                if log.contains("object already registered") || log.contains("already exists") {
                    Ok(())
                } else {
                    bail!(
                        "register_object tx failed: code={} log={}",
                        result.code,
                        log
                    );
                }
            }
            Err(error) => {
                let msg = error.to_string();
                if msg.contains("object already registered") || msg.contains("already exists") {
                    Ok(())
                } else {
                    Err(anyhow!("failed to register object in ACP: {}", error))
                }
            }
        }
    }

    pub async fn set_relationship(
        client: &SourceHubClient,
        policy_id: &str,
        resource: &str,
        object_id: &str,
        relation: &str,
        reader_did_uri: &str,
    ) -> Result<()> {
        let relationship = Relationship {
            object: Some(Object {
                resource: resource.to_string(),
                id: object_id.to_string(),
            }),
            relation: relation.to_string(),
            subject: Some(Subject {
                kind: Some(SubjectKind::Actor(Actor {
                    id: reader_did_uri.to_string(),
                })),
            }),
        };

        match client.acp_set_relationship(policy_id, relationship).await {
            Ok(result) if result.code == 0 => Ok(()),
            Ok(result) => {
                let log = result.log;
                if log.contains("relationship already exists") || log.contains("already exists") {
                    Ok(())
                } else {
                    bail!(
                        "set_relationship tx failed: code={} log={}",
                        result.code,
                        log
                    );
                }
            }
            Err(error) => {
                let msg = error.to_string();
                if msg.contains("relationship already exists") || msg.contains("already exists") {
                    Ok(())
                } else {
                    Err(anyhow!("failed to set ACP relationship: {}", error))
                }
            }
        }
    }

    pub async fn start_pre(
        &self,
        namespace: &str,
        reader_pk_hex: &str,
        object_id: &str,
        derivation_hex: &str,
        salt: Option<&str>,
        timestamp: Option<u64>,
        jwt_signer: &JwtSigner,
    ) -> Result<PreResult> {
        let channel = self
            .endpoint
            .clone()
            .connect()
            .await
            .map_err(|e| anyhow!("failed to connect to Orbis PRE endpoint: {}", e))?;
        let mut client = PreServiceClient::new(channel);

        let reader_pk_bytes =
            hex::decode(reader_pk_hex).context("failed to decode reader key hex")?;
        let derivation_bytes =
            hex::decode(derivation_hex).context("failed to decode derivation hex")?;

        let request = StartPreRequest {
            rdr_pk: reader_pk_bytes.clone(),
            object_id: object_id.to_string(),
            namespace: namespace.to_string(),
            derivation: Some(derivation_bytes.clone()),
            salt: salt.map(str::to_owned),
            valid_window: timestamp.map(|ts| TimestampRange { start: ts, end: ts }),
        };

        let token = jwt_signer
            .create_pre_jwt(
                reader_pk_bytes,
                namespace,
                object_id,
                Some(derivation_bytes),
                salt.map(str::to_owned),
            )
            .map_err(|e| anyhow!("failed to create Orbis PRE JWT: {}", e))?;

        let response = client
            .start_pre(create_authenticated_request(request, &token)?)
            .await
            .map_err(|e| anyhow!("Orbis PRE request failed: {}", e))?
            .into_inner();

        if response.encrypted_secret.is_empty() {
            bail!("Orbis PRE response did not include encrypted_secret");
        }

        let pre_response: PreResponse = serde_json::from_slice(&response.encrypted_secret)
            .map_err(|e| anyhow!("failed to parse PRE response JSON: {}", e))?;

        Ok(PreResult {
            xnc_cmt_hex: pre_response.xnc_cmt,
            secret: pre_response.secret,
        })
    }

    async fn policy_defines_resource(
        client: &SourceHubClient,
        policy_id: &str,
        resource_name: &str,
        permission_name: &str,
    ) -> Result<bool> {
        let policy = client
            .acp_query_policy(policy_id)
            .await
            .map_err(|e| anyhow!("failed to query created policy {policy_id}: {}", e))?;
        Ok(policy
            .record
            .and_then(|record| record.policy)
            .map(|policy| {
                policy.resources.iter().any(|resource| {
                    resource.name == resource_name
                        && resource
                            .permissions
                            .iter()
                            .any(|permission| permission.name == permission_name)
                })
            })
            .unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn containerized_p2p_address_can_be_derived_without_cli_parsing() {
        let info = NodeInfo {
            public_address: "sourcehub1deadbeef".to_string(),
            peer_id: "12D3KooWExample".to_string(),
            p2p_address: "/ip4/127.0.0.1/tcp/4001".to_string(),
        };
        assert_eq!(info.peer_id, "12D3KooWExample");
        assert!(info.p2p_address.contains("4001"));
    }

    #[test]
    fn invalid_endpoint_is_rejected() {
        assert!(OrbisClient::new("not a valid endpoint").is_err());
    }
}
