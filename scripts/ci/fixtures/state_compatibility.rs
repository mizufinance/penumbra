use anyhow::{ensure, Context, Result};
use cnidarium::{StateDelta, Storage};
use decaf377::Fq;
use prost::Message;
use shieldd::ExecutionService;
use shieldd_sdk_app::{
    app::{StateReadExt as _, StateWriteExt as _},
    genesis::{AppState, Content},
    SUBSTORE_PREFIXES,
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

async fn seed(db: &Path) -> Result<ExecutionService> {
    let mut service = ExecutionService::open(db).await?;
    service
        .init_genesis(InitGenesisRequest {
            genesis: Some(
                AppState::Content(Content::default().with_chain_id("state-persistence".into()))
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
    let storage = Storage::load(db.to_path_buf(), SUBSTORE_PREFIXES.to_vec()).await?;
    let mut state = StateDelta::new(storage.latest_snapshot());
    // Seed a spent marker without requiring proof artifacts in this storage test.
    nullifier_tree::insert_batch(&mut state, [Nullifier(Fq::from(7u64))]).await?;
    state.put_block_transaction(1, Default::default()).await?;
    storage.commit(state).await?;
    storage.release().await;
    Ok(ExecutionService::open(db).await?)
}

async fn persisted_history(db: &Path) -> Result<Vec<u8>> {
    let storage = Storage::load(db.to_path_buf(), SUBSTORE_PREFIXES.to_vec()).await?;
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
    let encoded = history.encode_to_vec();
    storage.release().await;
    Ok(encoded)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    ensure!(args.len() == 3, "usage: state_compatibility WORKDIR OUTPUT");
    let work = Path::new(&args[1]);
    std::fs::create_dir_all(work)?;
    let mut control = None;
    let mut control_queries = None;
    let mut control_history = None;
    for mode in ["uninterrupted", "reopen", "checkpoint"] {
        let db = work.join(mode);
        let mut service = seed(&db).await?;
        let before = snapshot(&service).await?;
        if let Some(expected) = &control_queries {
            ensure!(&before == expected, "initial query mismatch");
        } else {
            control_queries = Some(before.clone());
        }
        if mode != "uninterrupted" {
            let checkpoint = service.export_genesis(ExportGenesisRequest {}).await?;
            service.close().await?;
            let history = persisted_history(&db).await?;
            if let Some(expected) = &control_history {
                ensure!(&history == expected, "historical bytes changed");
            } else {
                control_history = Some(history);
            }
            service = ExecutionService::open(&db).await?;
            ensure!(
                snapshot(&service).await? == before,
                "committed queries/proofs changed after reopen"
            );
            if mode == "checkpoint" {
                service
                    .init_genesis(InitGenesisRequest {
                        genesis: checkpoint.genesis,
                    })
                    .await?;
                ensure!(
                    snapshot(&service).await? == before,
                    "checkpoint changed committed queries/proofs"
                );
            }
        }
        begin(&mut service, 2).await?;
        ensure!(
            service.deposit(deposit()).await.is_err(),
            "historical host source accepted in a new block"
        );
        service.end_block(EndBlockRequest { height: 2 }).await?;
        service.commit(CommitRequest {}).await?;
        let after = snapshot(&service).await?;
        service.close().await?;
        let history = persisted_history(&db).await?;
        if let Some(expected) = &control_history {
            ensure!(&history == expected, "historical bytes changed");
        } else {
            control_history = Some(history);
        }
        if let Some(expected) = &control {
            ensure!(
                &after == expected,
                "{mode} differs from uninterrupted execution"
            );
        } else {
            control = Some(after);
        }
    }
    std::fs::write(&args[2], control.context("control result")?)?;
    Ok(())
}
