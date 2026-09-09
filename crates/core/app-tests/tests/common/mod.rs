//! Direct host fixtures for integration tests.
#[allow(unused_imports)]
pub use shieldd_sdk_test_subscriber::set_tracing_subscriber;
#[allow(dead_code)]
pub async fn new_storage() -> anyhow::Result<cnidarium::TempStorage> {
    cnidarium::TempStorage::new_with_prefixes(shieldd_sdk_app::SUBSTORE_PREFIXES.to_vec()).await
}

#[allow(dead_code)]
pub async fn scan_latest(
    chain: &cnidarium::TempStorage,
    wallet: &shieldd_sdk_view::Storage,
) -> anyhow::Result<()> {
    use shieldd_sdk_sct::component::clock::EpochRead as _;
    let snapshot = chain.latest_snapshot();
    let mut worker = shieldd_sdk_view::SyncWorker::new(wallet.clone()).await?;
    let first = wallet.last_sync_height().await?.map(|h| h + 1).unwrap_or(0);
    let last = snapshot.get_block_height().await?;
    for height in first..=last {
        worker.scan(wallet_block(&snapshot, height).await?).await?;
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn wallet_block(
    snapshot: &cnidarium::Snapshot,
    height: u64,
) -> anyhow::Result<shieldd_sdk_view::WalletBlock> {
    use anyhow::Context;
    use shieldd_sdk_app::app::StateReadExt as _;
    use shieldd_sdk_compact_block::component::StateReadExt as _;
    use shieldd_sdk_sct::component::{clock::EpochRead as _, tree::SctRead as _};
    let block = snapshot
        .compact_block(height)
        .await?
        .context("missing compact block")?;
    let updated_app_parameters = if block.app_parameters_updated {
        Some(snapshot.get_app_params().await?)
    } else {
        None
    };
    Ok(shieldd_sdk_view::WalletBlock {
        block: block.try_into()?,
        expected_sct_root: snapshot
            .get_anchor_by_height(height)
            .await?
            .context("missing SCT anchor")?,
        // Genesis has no signed host block time.
        timestamp: if height == 0 {
            0
        } else {
            snapshot
                .get_block_timestamp(height)
                .await?
                .unix_timestamp()
                .try_into()?
        },
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
}
