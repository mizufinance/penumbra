#![deny(clippy::unwrap_used)]

use std::{env, ops::Deref, path::PathBuf, str::FromStr};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use cnidarium::Storage;
use decaf377::Fr;
use rand_core::OsRng;
use shieldd_sdk_app::SUBSTORE_PREFIXES;
use shieldd_sdk_asset::{asset, Value};
use shieldd_sdk_keys::{test_keys, Address};
use shieldd_sdk_mock_client::MockClient;
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::DomainType;
use shieldd_sdk_shielded_pool::{
    EvmCall, HostExecution, HostTransfer, HostWithdrawal, HostWithdrawalDestination,
    ShieldedInputPlan, ShieldedOutputPlan,
};
use shieldd_sdk_transaction::{memo::MemoPlaintext, plan::MemoPlan, TransactionParameters};

struct Opt {
    db: PathBuf,
    chain_id: String,
    destination: HostWithdrawalDestination,
    denom: String,
    amount: Amount,
}

#[tokio::main]
async fn main() -> Result<()> {
    let tx_bytes = build_host_withdrawal_tx(parse_args()?).await?;
    println!("{}", STANDARD.encode(tx_bytes));
    Ok(())
}

fn parse_args() -> Result<Opt> {
    let mut args = env::args().skip(1);
    let usage = "usage: bankd-e2e-host-withdrawal-builder \
        <db> <chain-id> <recipient-or-refund-address> <denom> <amount> \
        [<gas-limit> <contract> <calldata> ...]";
    let db = PathBuf::from(args.next().context(usage)?);
    let chain_id = args.next().context(usage)?;
    let recipient_or_refund_address = args.next().context(usage)?;
    let denom = args.next().context(usage)?;
    let raw_amount = args.next().context(usage)?;
    let amount = Amount::from(
        raw_amount
            .parse::<u64>()
            .with_context(|| format!("invalid withdrawal amount {raw_amount}"))?,
    );

    let destination = match args.next() {
        None => HostWithdrawalDestination::Transfer(HostTransfer {
            recipient: recipient_or_refund_address,
        }),
        Some(raw_gas_limit) => {
            let gas_limit = raw_gas_limit
                .parse::<u64>()
                .with_context(|| format!("invalid execution gas limit {raw_gas_limit}"))?;
            let refund_address = Address::from_str(&recipient_or_refund_address)
                .with_context(|| format!("invalid refund address {recipient_or_refund_address}"))?;

            let mut calls = Vec::new();
            loop {
                let Some(raw_contract) = args.next() else {
                    break;
                };
                let raw_calldata = args.next().context(usage)?;
                calls.push(EvmCall {
                    contract: decode_contract(&raw_contract)?,
                    calldata: decode_hex(&raw_calldata)
                        .with_context(|| format!("invalid calldata {raw_calldata}"))?,
                });
            }
            if calls.is_empty() {
                bail!("host execution must contain at least one call");
            }

            HostWithdrawalDestination::Execution(HostExecution {
                refund_address,
                gas_limit,
                calls,
            })
        }
    };

    Ok(Opt {
        db,
        chain_id,
        destination,
        denom,
        amount,
    })
}

fn decode_contract(raw: &str) -> Result<[u8; 20]> {
    let bytes = decode_hex(raw).with_context(|| format!("invalid contract address {raw}"))?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("contract address must contain exactly 20 bytes: {raw}"))
}

fn decode_hex(raw: &str) -> Result<Vec<u8>> {
    let raw = raw.strip_prefix("0x").unwrap_or(raw);
    if raw.len() % 2 != 0 {
        bail!("hex input must contain an even number of digits");
    }

    (0..raw.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&raw[index..index + 2], 16)
                .with_context(|| format!("invalid hex byte at offset {index}"))
        })
        .collect()
}

async fn build_host_withdrawal_tx(opt: Opt) -> Result<Vec<u8>> {
    let storage = Storage::load(opt.db.clone(), SUBSTORE_PREFIXES.to_vec())
        .await
        .with_context(|| format!("failed to open Shieldd RocksDB at {}", opt.db.display()))?;

    let mut client = MockClient::new(test_keys::SPEND_KEY.clone());
    client
        .sync_to_latest(storage.latest_snapshot())
        .await
        .context("failed to sync Shieldd test wallet to storage")?;

    let denom: asset::Metadata = opt
        .denom
        .as_str()
        .try_into()
        .with_context(|| format!("invalid withdrawal denom {}", opt.denom))?;
    let input_note = client
        .notes
        .values()
        .filter(|note| note.address() == test_keys::ADDRESS_0.deref().clone())
        .filter(|note| note.asset_id() == denom.id())
        .filter(|note| !client.spent_note(&note.commit()))
        .filter(|note| note.amount() >= opt.amount)
        .cloned()
        .next()
        .ok_or_else(|| {
            anyhow!(
                "no spendable note for Shieldd test address {} covering {}{}",
                test_keys::ADDRESS_0_STR,
                opt.amount,
                opt.denom,
            )
        })?;

    let position = client
        .position(input_note.commit())
        .ok_or_else(|| anyhow!("input note commitment was unknown to mock client"))?;
    let spend = ShieldedInputPlan::new(&mut OsRng, input_note.clone(), position);
    let change_amount = input_note
        .amount()
        .checked_sub(&opt.amount)
        .context("input note amount must cover withdrawal amount")?;
    let change = if change_amount == Amount::zero() {
        None
    } else {
        Some(ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: change_amount,
                asset_id: input_note.asset_id(),
            },
            input_note.address(),
        ))
    };

    let withdrawal = HostWithdrawal {
        value: Value {
            amount: opt.amount,
            asset_id: input_note.asset_id(),
        },
        destination: opt.destination,
    };
    let withdrawal_plan = shieldd_sdk_mock_client::WithdrawalIntent {
        spends: vec![spend],
        change_output: change,
        withdrawal: withdrawal,
        value_blinding: Fr::from(1u64),
    };
    let intent = shieldd_sdk_mock_client::TransactionIntent {
        actions: vec![withdrawal_plan.into()],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(test_keys::ADDRESS_0.deref().clone()),
        )),
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: opt.chain_id,
            ..Default::default()
        },
        nullifier_window: None,
    };

    let snapshot = storage.latest_snapshot();
    let plan = client.complete_intent(intent, snapshot).await?;
    let tx = client
        .witness_auth_build(&plan)
        .await
        .context("failed to build Shieldd host withdrawal transaction")?;
    Ok(tx.encode_to_vec())
}
