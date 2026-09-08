use crate::service::*;
use anyhow::Result;
use cnidarium::TempStorage;
use shieldd_sdk_app::{
    genesis::{AppState, Content},
    SUBSTORE_PREFIXES,
};
use shieldd_sdk_asset::asset;
use shieldd_sdk_keys::test_keys::ADDRESS_0;
use shieldd_sdk_proto::cnidarium::v1::KeyValueRequest as ComponentKeyValueRequest;
use shieldd_sdk_proto::core::app::v1::AppParametersRequest as ComponentAppParametersRequest;
use shieldd_sdk_proto::core::component::{
    compact_block::v1::CompactBlockRangeRequest as ComponentCompactBlockRangeRequest,
    compliance::v1::{
        ComplianceAssetStatusRequest as ComponentComplianceAssetStatusRequest,
        ComplianceBatchMerkleProofsRequest as ComponentComplianceBatchMerkleProofsRequest,
        ComplianceBatchQuery, ComplianceUserLeafRequest as ComponentComplianceUserLeafRequest,
    },
    sct::v1::NullifierWindowRequest as ComponentNullifierWindowRequest,
    shielded_pool::v1::AssetMetadataByIdRequest as ComponentAssetMetadataByIdRequest,
};
use shieldd_sdk_proto::execution_client::v1::{
    BeginBlockRequest, CheckTxRequest, CommitRequest, DeliverTxRequest, EndBlockRequest,
    InitGenesisRequest,
};
use std::ops::Deref as _;

async fn initialized_client() -> Result<(TempStorage, ExecutionService)> {
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let mut client = ExecutionService::new(storage.deref().clone());
    client
        .init_genesis(InitGenesisRequest {
            genesis: Some(
                AppState::Content(
                    Content::default().with_chain_id("shieldd-service-test".to_owned()),
                )
                .into(),
            ),
        })
        .await?;
    client.commit(CommitRequest {}).await?;
    Ok((storage, client))
}

#[tokio::test]
async fn execution_check_tx_rejects_invalid_transaction() -> Result<()> {
    let (_storage, client) = initialized_client().await?;

    let response = client
        .check_tx(CheckTxRequest {
            tx: b"not a shieldd transaction".to_vec(),
        })
        .await?;

    assert_eq!(response.code, 1);
    assert!(response.log.contains("decoding transaction"));
    Ok(())
}

#[tokio::test]
async fn execution_deliver_tx_rejects_invalid_transaction() -> Result<()> {
    let (_storage, mut client) = initialized_client().await?;
    let mut begin_block = BeginBlockRequest {
        height: 1,
        time: Some(Default::default()),
    };
    begin_block
        .time
        .as_mut()
        .expect("test begin-block time")
        .seconds = 1_700_000_000;
    client.begin_block(begin_block).await?;

    let response = client
        .deliver_tx(DeliverTxRequest {
            tx: b"not a shieldd transaction".to_vec(),
        })
        .await?;

    assert_eq!(response.code, 1);
    assert!(response.log.contains("decoding transaction"));
    Ok(())
}

#[tokio::test]
async fn execution_exposes_embedded_frontend_queries() -> Result<()> {
    let (_storage, client) = initialized_client().await?;
    let asset_id = asset::REGISTRY.parse_unit("shieldd").id();
    let asset_id_proto: shieldd_sdk_proto::core::asset::v1::AssetId = asset_id.into();
    let address: shieldd_sdk_proto::core::keys::v1::Address = ADDRESS_0.clone().into();

    let parameters = client
        .app_parameters(ComponentAppParametersRequest {})
        .await?
        .app_parameters
        .expect("app parameters response contains parameters");
    assert_eq!(parameters.chain_id, "shieldd-service-test");

    let nullifier_window = client
        .nullifier_window(ComponentNullifierWindowRequest {})
        .await?
        .window
        .expect("initialized state contains a nullifier window");
    assert_eq!(
        nullifier_window.protocol_version,
        shieldd_sdk_sct::nullifier_generation::PROTOCOL_VERSION
    );
    assert_eq!(nullifier_window.current_generation, 0);

    let metadata = client
        .asset_metadata_by_id(ComponentAssetMetadataByIdRequest {
            asset_id: Some(asset_id_proto.clone()),
        })
        .await?
        .denom_metadata
        .expect("known genesis asset metadata");
    assert_eq!(metadata.base, "ushieldd");

    let status = client
        .compliance_asset_status(ComponentComplianceAssetStatusRequest {
            asset_id: Some(asset_id_proto.clone()),
        })
        .await?;
    assert!(status.is_registered);
    assert!(!status.is_regulated);

    let user = client
        .compliance_user_leaf(ComponentComplianceUserLeafRequest {
            address: Some(address.clone()),
            asset_id: Some(asset_id_proto.clone()),
        })
        .await?;
    assert!(!user.is_registered);
    assert!(user.leaf.is_none());

    let batch = client
        .compliance_batch_merkle_proofs(ComponentComplianceBatchMerkleProofsRequest {
            queries: vec![ComplianceBatchQuery {
                address: Some(address),
                asset_id: Some(asset_id_proto),
            }],
        })
        .await?;
    assert_eq!(batch.results.len(), 1);
    assert!(batch.results[0].asset_registered);
    assert!(!batch.results[0].user_registered);

    let key = client
        .key_value(ComponentKeyValueRequest {
            key: shieldd_sdk_sct::state_key::tree::anchor_by_height(0),
            proof: false,
        })
        .await?;
    assert!(key.value.is_some());
    assert!(key.proof.is_none());
    Ok(())
}

#[tokio::test]
async fn execution_reads_bounded_compact_block_ranges() -> Result<()> {
    let (_storage, mut client) = initialized_client().await?;
    for height in 1..=2 {
        let mut begin = BeginBlockRequest {
            height,
            time: Some(Default::default()),
        };
        begin.time.as_mut().expect("test begin-block time").seconds = 1_700_000_000 + height;
        client.begin_block(begin).await?;
        client.end_block(EndBlockRequest { height }).await?;
        client.commit(CommitRequest {}).await?;
    }

    let responses = client
        .compact_block_range(ComponentCompactBlockRangeRequest {
            start_height: 0,
            end_height: 2,
            keep_alive: false,
        })
        .await?;
    let heights = responses
        .into_iter()
        .map(|response| {
            response
                .compact_block
                .expect("range response contains a compact block")
                .height
        })
        .collect::<Vec<_>>();
    assert_eq!(heights, vec![0, 1, 2]);
    Ok(())
}
#[tokio::test]
async fn key_value_proves_membership_and_absence_at_committed_root() -> Result<()> {
    use anyhow::Context as _;
    use cnidarium::{StateDelta, StateWrite as _};
    use ibc_types::core::commitment::{MerklePath, MerkleProof, MerkleRoot};
    // ICS23 requires nonempty leaf values, including absence-proof neighbors.
    let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec()).await?;
    let client = ExecutionService::new(storage.deref().clone());
    let mut delta = StateDelta::new(storage.latest_snapshot());
    delta.put_raw("query-proof-present".into(), b"main-store value".to_vec());
    delta.put_raw(
        "ibc-data/query-proof-present".into(),
        b"committed value".to_vec(),
    );
    storage.commit(delta).await?;
    let snapshot = storage.latest_snapshot();
    let root = MerkleRoot {
        hash: snapshot.root_hash().await?.0.to_vec(),
    };
    for (key, path, present) in [
        ("query-proof-present", vec!["query-proof-present"], true),
        ("query-proof-absent", vec!["query-proof-absent"], false),
        (
            "ibc-data/query-proof-present",
            vec!["ibc-data", "query-proof-present"],
            true,
        ),
        (
            "ibc-data/query-proof-absent",
            vec!["ibc-data", "query-proof-absent"],
            false,
        ),
    ] {
        let response = client
            .key_value(ComponentKeyValueRequest {
                key: key.into(),
                proof: true,
            })
            .await?;
        let proof: MerkleProof = response.proof.expect("requested proof").try_into()?;
        let (value, direct_proof) = snapshot.get_with_proof(key.as_bytes().to_vec()).await?;
        assert_eq!(proof, direct_proof);
        assert_eq!(
            response.value.as_ref().map(|value| &value.value),
            value.as_ref()
        );
        assert_eq!(value.is_some(), present);
        assert_eq!(proof.proofs.len(), path.len(), "proof depth for {key}");
        let specs = vec![cnidarium::ics23_spec(); path.len()];
        let path = MerklePath {
            key_path: path.into_iter().map(str::to_owned).collect(),
        };
        if let Some(value) = value {
            proof
                .verify_membership(&specs, root.clone(), path, value, 0)
                .with_context(|| format!("membership proof for {key}"))?;
        } else {
            proof
                .verify_non_membership(&specs, root.clone(), path)
                .with_context(|| format!("absence proof for {key}"))?;
        }
    }
    Ok(())
}
