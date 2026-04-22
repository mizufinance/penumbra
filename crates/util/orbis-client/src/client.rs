use anyhow::{anyhow, bail, Context, Result};
use decaf377::Encoding;
use did_key::{generate, Ed25519KeyPair as DidEd25519KeyPair, Fingerprint};
use orbis_authn::{create_authenticated_request, JwtSigner};
use orbis_common::blockchain::{
    acp::{Actor, Object, Relationship, Subject, SubjectKind},
    ChainConfig, SourceHubClient, TxSigner, TEST_ACCOUNT_HEX_KEY,
};
use orbis_proto::{
    dkg_service::{dkg_service_client::DkgServiceClient, StartDkgRequest},
    info_service::{info_service_client::InfoServiceClient, GetNodeInfoRequest},
    pre_service::{pre_service_client::PreServiceClient, StartPreRequest},
    store_secret_service::{
        store_secret_service_client::StoreSecretServiceClient, StoreSecretRequest,
    },
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    auth::{default_reader_did_pk, deterministic_jwt_signer},
    pre::prepare_secret,
    types::{
        DkgResult, NodeInfo, PreResult, PreparedSecret, RingInfo, SecretEnvelope, StoreSecretResult,
    },
};

const TEST_POLICY_YAML: &str = r#"
name: test-policy
resources:
  - name: document
    relations:
      - name: creator
        types:
          - actor
      - name: reader
        types:
          - actor
    permissions:
      - name: read
        expr: creator + reader
      - name: write
        expr: creator
"#;

const ORBIS_NAMESPACE: &str = "orbis";
const ORBIS_RESOURCE: &str = "document";
const ORBIS_PERMISSION: &str = "read";

#[derive(Debug, Deserialize)]
struct RingPayload {
    ring_pk: String,
}

#[derive(Debug, Deserialize)]
struct PreResponse {
    xnc_cmt: String,
    secret: SecretEnvelope,
}

pub struct OrbisClient {
    endpoint: String,
    chain_config: ChainConfig,
}

impl OrbisClient {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            chain_config: ChainConfig::local(),
        }
    }

    pub async fn query_node_info(&self) -> Result<NodeInfo> {
        let mut client = InfoServiceClient::connect(self.endpoint.clone())
            .await
            .map_err(|e| anyhow!("failed to connect to {}: {}", self.endpoint, e))?;

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

    pub async fn start_dkg(&self, threshold: u32, peer_ids: &[String]) -> Result<DkgResult> {
        let total_nodes = peer_ids.len() as u32;
        if threshold > total_nodes {
            bail!("threshold ({threshold}) cannot be greater than total nodes ({total_nodes})");
        }

        let mut client = DkgServiceClient::connect(self.endpoint.clone())
            .await
            .map_err(|e| anyhow!("failed to connect to {}: {}", self.endpoint, e))?;

        let request = StartDkgRequest {
            threshold,
            peer_ids: peer_ids.to_vec(),
            pss_interval: None,
        };
        let jwt_signer = JwtSigner::new();
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

    pub async fn register_bulletin_namespace(&self, namespace: &str) -> Result<()> {
        let read_client = SourceHubClient::new(self.chain_config.clone())
            .await
            .map_err(|e| anyhow!("failed to create chain client: {}", e))?;

        if read_client.bulletin_get_namespace(namespace).await.is_ok() {
            return Ok(());
        }

        let signer = TxSigner::from_hex_key(TEST_ACCOUNT_HEX_KEY, self.chain_config.clone())
            .map_err(|e| anyhow!("failed to create signer: {}", e))?;
        let client = SourceHubClient::with_signer(self.chain_config.clone(), signer)
            .await
            .map_err(|e| anyhow!("failed to create signed SourceHub client: {}", e))?;

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
        &self,
        namespace: &str,
        collaborator_address: &str,
    ) -> Result<()> {
        let client = self.signing_client().await?;

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

    pub async fn get_latest_ring(&self) -> Result<RingInfo> {
        let client = SourceHubClient::new(self.chain_config.clone())
            .await
            .map_err(|e| anyhow!("failed to create SourceHub client: {}", e))?;

        let posts = client
            .bulletin_list_posts(ORBIS_NAMESPACE)
            .await
            .map_err(|e| anyhow!("failed to list Orbis ring posts: {}", e))?;

        let post = posts
            .last()
            .ok_or_else(|| anyhow!("no Orbis ring posts found; run DKG first"))?;
        let ring_payload: RingPayload = serde_json::from_slice(&post.payload)
            .map_err(|e| anyhow!("failed to parse Orbis ring payload: {}", e))?;

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

    pub async fn add_policy(&self) -> Result<String> {
        let client = self.signing_client().await?;

        let create_result = client
            .acp_create_policy(TEST_POLICY_YAML, 1)
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
        policy_ids
            .ids
            .last()
            .cloned()
            .ok_or_else(|| anyhow!("no policy IDs found after policy creation"))
    }

    pub async fn store_secret(
        &self,
        ring_pk_hex: &str,
        ring_id: &str,
        policy_id: &str,
        derivation_hex: &str,
    ) -> Result<StoreSecretResult> {
        let prepared = self.prepare_secret(ring_pk_hex, derivation_hex, policy_id)?;
        let mut client = StoreSecretServiceClient::connect(self.endpoint.clone())
            .await
            .map_err(|e| anyhow!("failed to connect to Orbis store-secret endpoint: {}", e))?;

        let request = StoreSecretRequest {
            encrypted_document: prepared.encrypted_document.clone(),
            enc_cmt: prepared.enc_cmt.clone(),
            ring_id: ring_id.to_string(),
            namespace: ORBIS_NAMESPACE.to_string(),
            policy_id: policy_id.to_string(),
            resource: ORBIS_RESOURCE.to_string(),
            permission: ORBIS_PERMISSION.to_string(),
            shared_point: prepared.shared_point.clone(),
            challenge: prepared.challenge.clone(),
            response: prepared.response.clone(),
            derived_pk: prepared.derived_pk.clone(),
            with_proof: false,
            tier: None,
            timestamp: None,
            metadata_hash: Some(prepared.metadata.clone()),
        };

        let token = deterministic_jwt_signer(default_reader_did_pk())
            .create_store_secret_jwt(
                prepared.encrypted_document,
                prepared.enc_cmt.clone(),
                ring_id,
                ORBIS_NAMESPACE,
                policy_id,
                ORBIS_RESOURCE,
                ORBIS_PERMISSION,
                prepared.shared_point,
                prepared.challenge,
                prepared.response,
                prepared.derived_pk,
                false,
                None,
                None,
                Some(prepared.metadata),
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
            enc_cmt_hex: hex::encode(prepared.enc_cmt),
        })
    }

    pub async fn register_object(&self, policy_id: &str, object_id: &str) -> Result<()> {
        let client = self.signing_client().await?;
        let document = Object {
            resource: ORBIS_RESOURCE.to_string(),
            id: object_id.to_string(),
        };

        let result = client
            .acp_register_object(policy_id, document)
            .await
            .map_err(|e| anyhow!("failed to register object in ACP: {}", e))?;
        if result.code != 0 {
            bail!(
                "register_object tx failed: code={} log={}",
                result.code,
                result.log
            );
        }
        Ok(())
    }

    pub async fn set_relationship(&self, policy_id: &str, object_id: &str) -> Result<()> {
        let client = self.signing_client().await?;
        let key_pair = generate::<DidEd25519KeyPair>(Some(&did_seed(default_reader_did_pk())));
        let did_uri = format!("did:key:{}", key_pair.fingerprint());

        let relationship = Relationship {
            object: Some(Object {
                resource: ORBIS_RESOURCE.to_string(),
                id: object_id.to_string(),
            }),
            relation: "reader".to_string(),
            subject: Some(Subject {
                kind: Some(SubjectKind::Actor(Actor { id: did_uri })),
            }),
        };

        let result = client
            .acp_set_relationship(policy_id, relationship)
            .await
            .map_err(|e| anyhow!("failed to set ACP relationship: {}", e))?;
        if result.code != 0 {
            bail!(
                "set_relationship tx failed: code={} log={}",
                result.code,
                result.log
            );
        }
        Ok(())
    }

    pub async fn pre_xnc_only(
        &self,
        reader_pk_hex: &str,
        object_id: &str,
        derivation_hex: &str,
    ) -> Result<PreResult> {
        let mut client = PreServiceClient::connect(self.endpoint.clone())
            .await
            .map_err(|e| anyhow!("failed to connect to Orbis PRE endpoint: {}", e))?;

        let reader_pk_bytes =
            hex::decode(reader_pk_hex).context("failed to decode adjusted reader key hex")?;
        let derivation_bytes =
            hex::decode(derivation_hex).context("failed to decode derivation hex")?;

        let request = StartPreRequest {
            rdr_pk: reader_pk_bytes.clone(),
            object_id: object_id.to_string(),
            namespace: ORBIS_NAMESPACE.to_string(),
            derivation: Some(derivation_bytes.clone()),
            salt: None,
            valid_window: None,
        };

        let token = deterministic_jwt_signer(default_reader_did_pk())
            .create_pre_jwt(
                reader_pk_bytes,
                ORBIS_NAMESPACE,
                object_id,
                Some(derivation_bytes),
                None,
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

        let _ = pre_response.secret;
        Ok(PreResult {
            xnc_cmt_hex: pre_response.xnc_cmt,
        })
    }

    fn prepare_secret(
        &self,
        ring_pk_hex: &str,
        derivation_hex: &str,
        policy_id: &str,
    ) -> Result<PreparedSecret> {
        prepare_secret(
            ring_pk_hex,
            derivation_hex,
            policy_id,
            ORBIS_RESOURCE,
            ORBIS_PERMISSION,
            None,
            None,
            None,
        )
    }

    async fn signing_client(&self) -> Result<SourceHubClient> {
        let signer = TxSigner::from_hex_key(TEST_ACCOUNT_HEX_KEY, self.chain_config.clone())
            .map_err(|e| anyhow!("failed to create SourceHub signer: {}", e))?;
        SourceHubClient::with_signer(self.chain_config.clone(), signer)
            .await
            .map_err(|e| anyhow!("failed to create signed SourceHub client: {}", e))
    }
}

fn did_seed(s: &str) -> [u8; 32] {
    Sha256::digest(s.as_bytes()).into()
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
}
