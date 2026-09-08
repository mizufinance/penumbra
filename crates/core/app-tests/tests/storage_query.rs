use anyhow::Context;
use cnidarium::proto::v1::{query_service_server::QueryService, KeyValueRequest};
use cnidarium::{StateRead as _, TempStorage};
use ibc_types::core::commitment::{MerklePath, MerkleProof, MerkleRoot};
use shieldd_sdk_app::{
    genesis::{AppState, Content},
    test_support::{TestHost, TEST_CHAIN_ID},
};

#[tokio::test]
async fn host_storage_query_proves_committed_value_at_exact_root() -> anyhow::Result<()> {
    let storage = TempStorage::new().await?;
    let mut host = TestHost::new(
        storage.as_ref().clone(),
        AppState::Content(Content::default().with_chain_id(TEST_CHAIN_ID.into())),
        tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?,
    )
    .await?;
    let committed = host.execute(vec![]).await?.commit;
    let snapshot = storage.latest_snapshot();
    assert_eq!(snapshot.version(), 1);
    let key = "application/data/chain_id";
    let value = snapshot
        .get_raw(key)
        .await?
        .context("chain ID must be stored")?;
    let (direct_value, direct_proof) = snapshot.get_with_proof(key.as_bytes().to_vec()).await?;
    assert_eq!(direct_value.as_ref(), Some(&value));
    let response = cnidarium::rpc::Server::new(storage.as_ref().clone())
        .key_value(tonic::Request::new(KeyValueRequest {
            key: key.into(),
            proof: true,
        }))
        .await?
        .into_inner();
    assert_eq!(response.value.context("query value")?.value, value);
    let proof: MerkleProof = response.proof.context("query proof")?.try_into()?;
    assert_eq!(proof, direct_proof);
    assert_eq!(committed.root_hash, snapshot.root_hash().await?.0.to_vec());
    proof.verify_membership(
        &[cnidarium::ics23_spec()],
        MerkleRoot {
            hash: committed.root_hash,
        },
        MerklePath {
            key_path: vec![key.into()],
        },
        value,
        0,
    )?;
    Ok(())
}
