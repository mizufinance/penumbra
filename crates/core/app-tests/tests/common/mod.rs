//! Direct host fixtures for integration tests.
#[allow(unused_imports)]
pub use shieldd_sdk_test_subscriber::{
    set_tracing_subscriber, set_tracing_subscriber_with_env_filter,
};
#[allow(unused_imports)]
pub use temp_storage_ext::TempStorageExt;
mod temp_storage_ext;

#[allow(dead_code)]
pub async fn scan_latest(
    chain: &cnidarium::TempStorage,
    wallet: &shieldd_sdk_view::Storage,
) -> anyhow::Result<()> {
    use anyhow::Context;
    use shieldd_sdk_app::app::StateReadExt as _;
    use shieldd_sdk_compact_block::component::StateReadExt as _;
    use shieldd_sdk_sct::component::{clock::EpochRead as _, tree::SctRead as _};
    let snapshot = chain.latest_snapshot();
    let mut worker = shieldd_sdk_view::SyncWorker::new(wallet.clone()).await?;
    let first = wallet.last_sync_height().await?.map(|h| h + 1).unwrap_or(0);
    let last = snapshot.get_block_height().await?;
    for height in first..=last {
        let block = snapshot
            .compact_block(height)
            .await?
            .context("missing compact block")?;
        let updated_app_parameters = if block.app_parameters_updated {
            Some(snapshot.get_app_params().await?)
        } else {
            None
        };
        worker
            .scan(shieldd_sdk_view::WalletBlock {
                block: block.try_into()?,
                expected_sct_root: snapshot
                    .get_anchor_by_height(height)
                    .await?
                    .context("missing SCT anchor")?,
                timestamp: snapshot
                    .get_current_block_timestamp()
                    .await?
                    .unix_timestamp()
                    .try_into()?,
                transactions: snapshot
                    .transactions_by_height(height)
                    .await?
                    .transactions
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                assets: vec![],
                updated_app_parameters,
            })
            .await?;
    }
    Ok(())
}
