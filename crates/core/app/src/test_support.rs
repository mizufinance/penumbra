//! Direct execution driver for integration tests and benchmark fixture generation.

use crate::app::{HostBlock, HostCommit, HostExecution, HostTxResponse};
use crate::genesis::AppState;
use anyhow::{ensure, Context, Result};
use cnidarium::Storage;

pub const TEST_CHAIN_ID: &str = "shieldd-test";

pub struct TestHost {
    pub execution: HostExecution,
    next_height: i64,
    next_time: tendermint::Time,
}

pub struct BlockResult {
    pub transactions: Vec<HostTxResponse>,
    pub commit: HostCommit,
}

impl TestHost {
    pub async fn new(storage: Storage, genesis: AppState, time: tendermint::Time) -> Result<Self> {
        let mut execution = HostExecution::new(storage);
        execution.init_genesis(genesis).await?;
        execution.commit().await?;
        Ok(Self {
            execution,
            next_height: 1,
            next_time: time,
        })
    }

    pub async fn execute(&mut self, txs: Vec<Vec<u8>>) -> Result<BlockResult> {
        let block = HostBlock {
            height: self.next_height,
            time: self.next_time,
        };
        self.execute_block(block, txs).await
    }

    pub async fn execute_block(
        &mut self,
        block: HostBlock,
        txs: Vec<Vec<u8>>,
    ) -> Result<BlockResult> {
        let height = block.height;
        let next_height = height.checked_add(1).context("test height overflow")?;
        let next_time = block
            .time
            .checked_add(std::time::Duration::from_secs(1))
            .context("test timestamp overflow")?;
        self.execution.begin_block(block).await?;
        let mut transactions = Vec::with_capacity(txs.len());
        for tx in txs {
            let response = self.execution.deliver_tx(&tx).await?;
            ensure!(
                response.code == 0,
                "test transaction rejected: {}",
                response.log
            );
            transactions.push(response);
        }
        self.execution.end_block(height).await?;
        let commit = self.execution.commit().await?;
        self.next_height = next_height;
        self.next_time = next_time;
        Ok(BlockResult {
            transactions,
            commit,
        })
    }
}
