use cnidarium::TempStorage;
use shieldd_sdk_app::{
    app::HostBlock,
    genesis::{AppState, Content},
    test_support::{TestHost, TEST_CHAIN_ID},
};
use shieldd_sdk_sct::component::clock::EpochRead as _;

#[tokio::test]
async fn host_commits_empty_blocks_with_exact_supplied_height_and_time() -> anyhow::Result<()> {
    let storage = TempStorage::new().await?;
    let start = tendermint::Time::parse_from_rfc3339("2026-01-01T00:00:00Z")?;
    let mut host = TestHost::new(
        storage.as_ref().clone(),
        AppState::Content(Content::default().with_chain_id(TEST_CHAIN_ID.into())),
        start,
    )
    .await?;
    assert_eq!(storage.latest_snapshot().get_block_height().await?, 0);
    for height in 1..=10 {
        let time = start
            .checked_add(std::time::Duration::from_secs(height * 13))
            .unwrap();
        host.execute_block(
            HostBlock {
                height: height as i64,
                time,
            },
            vec![],
        )
        .await?;
        let snapshot = storage.latest_snapshot();
        assert_eq!(snapshot.get_block_height().await?, height);
        assert_eq!(snapshot.get_current_block_timestamp().await?, time);
    }
    Ok(())
}
