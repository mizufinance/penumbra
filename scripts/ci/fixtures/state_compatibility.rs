use anyhow::{ensure, Context, Result};
use cnidarium::{StateDelta, Storage};
use decaf377::Fq;
use prost::Message;
use shieldd::ExecutionService;
use shieldd_sdk_app::{
    app::StateReadExt as _,
    genesis::{AppState, Content},
    StateWriteExt as _, SUBSTORE_PREFIXES,
};
use shieldd_sdk_keys::test_keys;
use shieldd_sdk_proto::{
    cnidarium::v1::KeyValueRequest,
    core::{
        app::v1::AppParametersRequest,
        component::{compact_block::v1::CompactBlockRangeRequest, sct::v1::NullifierWindowRequest},
    },
    execution_client::v1::*,
};
use shieldd_sdk_sct::{component::tree::VerificationExt, nullifier_tree, Nullifier};
use std::path::Path;

fn deposit() -> DepositRequest {
    DepositRequest {
        denom: shieldd_sdk_asset::BASE_ASSET_DENOM.to_string(),
        amount: "100".into(),
        recipient: test_keys::ADDRESS_0.to_string(),
        source: Some(HostSource {
            height: 1,
            tx_hash: vec![7; 32],
            tx_index: 0,
            msg_index: 0,
        }),
    }
}

async fn begin(service: &mut ExecutionService, height: i64) -> Result<()> {
    let mut request = BeginBlockRequest {
        height,
        time: Some(Default::default()),
    };
    request.time.as_mut().context("time")?.seconds = 1_700_000_000 + height;
    service.begin_block(request).await?;
    Ok(())
}

async fn snapshot(service: &ExecutionService) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    service
        .export_genesis(ExportGenesisRequest {})
        .await?
        .encode_length_delimited(&mut bytes)?;
    service
        .get_committed_state(GetCommittedStateRequest {})
        .await?
        .encode_length_delimited(&mut bytes)?;
    service
        .app_parameters(AppParametersRequest {})
        .await?
        .encode_length_delimited(&mut bytes)?;
    service
        .nullifier_window(NullifierWindowRequest {})
        .await?
        .encode_length_delimited(&mut bytes)?;
    for key in [
        "application/data/chain_id",
        "application/data/absent-compatibility-key",
    ] {
        service
            .key_value(KeyValueRequest {
                key: key.into(),
                proof: true,
            })
            .await?
            .encode_length_delimited(&mut bytes)?;
    }
    for block in service
        .compact_block_range(CompactBlockRangeRequest {
            start_height: 0,
            end_height: 1,
            keep_alive: false,
        })
        .await?
    {
        block.encode_length_delimited(&mut bytes)?;
    }
    Ok(bytes)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    ensure!(
        args.len() == 4,
        "usage: state_compatibility create|continue DB OUTPUT"
    );
    let db = Path::new(&args[2]);
    let mut service = ExecutionService::open(db).await?;
    if args[1] == "create" {
        service
            .init_genesis(InitGenesisRequest {
                genesis: Some(
                    AppState::Content(
                        Content::default().with_chain_id("state-compatibility".into()),
                    )
                    .into(),
                ),
            })
            .await?;
        service.commit(CommitRequest {}).await?;
        begin(&mut service, 1).await?;
        service.deposit(deposit()).await?;
        service.end_block(EndBlockRequest { height: 1 }).await?;
        service.commit(CommitRequest {}).await?;
        service.close().await?;
        let storage = Storage::load(db, SUBSTORE_PREFIXES.to_vec()).await?;
        let mut state = StateDelta::new(storage.latest_snapshot());
        // A spent-marker fixture exercises the persisted nullifier index without proving a spend.
        nullifier_tree::insert_batch(&mut state, [Nullifier(Fq::from(7u64))]).await?;
        // A typed history fixture checks the nonverifiable log separately from consensus state.
        state.put_block_transaction(1, Default::default()).await?;
        std::fs::write(
            db.with_extension("history"),
            state.transactions_by_height(1).await?.encode_to_vec(),
        )?;
        storage.commit(state).await?;
        storage.release().await;
        service = ExecutionService::open(db).await?;
        std::fs::write(db.with_extension("snapshot"), snapshot(&service).await?)?;
    } else {
        ensure!(args[1] == "continue", "unknown mode");
        ensure!(
            snapshot(&service).await? == std::fs::read(db.with_extension("snapshot"))?,
            "committed queries changed across versions"
        );
        service.close().await?;
        let storage = Storage::load(db, SUBSTORE_PREFIXES.to_vec()).await?;
        ensure!(
            storage
                .latest_snapshot()
                .check_nullifier_unspent(Nullifier(Fq::from(7u64)))
                .await
                .is_err(),
            "spent marker lost after reopening"
        );
        let history = storage.latest_snapshot().transactions_by_height(1).await?;
        ensure!(
            history.transactions.len() == 1,
            "historical transaction disappeared"
        );
        ensure!(
            history.encode_to_vec() == std::fs::read(db.with_extension("history"))?,
            "historical transaction bytes changed"
        );
        storage.release().await;
        service = ExecutionService::open(db).await?;
        let checkpoint = service.export_genesis(ExportGenesisRequest {}).await?;
        service
            .init_genesis(InitGenesisRequest {
                genesis: checkpoint.genesis,
            })
            .await?;
        begin(&mut service, 2).await?;
        ensure!(
            service.deposit(deposit()).await.is_err(),
            "historical host source was accepted in a new block"
        );
        service.end_block(EndBlockRequest { height: 2 }).await?;
        service.commit(CommitRequest {}).await?;
    }
    std::fs::write(&args[3], snapshot(&service).await?)?;
    service.close().await?;
    Ok(())
}
