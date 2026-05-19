use std::{str::FromStr, time::Duration};

use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use ibc_types::{
    core::{
        channel::{
            msgs::MsgRecvPacket, packet::Sequence, ChannelId, Packet, PortId, TimeoutHeight,
        },
        client::Height,
        commitment::MerkleProof,
    },
    timestamp::Timestamp,
};
use penumbra_sdk_ibc::component::ChannelStateWriteExt as _;
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::penumbra::core::component::ibc::v1::FungibleTokenPacketData;
use penumbra_sdk_shielded_pool::{
    benchmark_helpers::{
        benchmark_parse_ics20_receive_context, benchmark_shielded_ics20_withdrawal_roundtrip_inputs,
    },
    Ics20Withdrawal, ShieldedIcs20WithdrawalFamilyId, ShieldedIcs20WithdrawalProof,
};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;

fn configured_sizes(var: &str, default: &[usize]) -> Vec<usize> {
    std::env::var(var)
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.parse().expect("invalid IBC bench size"))
                .collect()
        })
        .unwrap_or_else(|| default.to_vec())
}

fn channel(index: u64) -> ChannelId {
    ChannelId::from_str(&format!("channel-{index}")).expect("valid benchmark channel id")
}

fn transfer_packet(sequence: u64, data: Vec<u8>) -> Packet {
    Packet {
        sequence: Sequence::from(sequence),
        port_on_a: PortId::transfer(),
        chan_on_a: channel(0),
        port_on_b: PortId::transfer(),
        chan_on_b: channel(1),
        data,
        timeout_height_on_b: TimeoutHeight::At(Height::new(0, 1_000_000).expect("valid height")),
        timeout_timestamp_on_b: Timestamp::from_nanoseconds(1_000_000_000)
            .expect("valid timestamp"),
    }
}

fn packet_data(returned_to_source: bool, receiver: &Address) -> Vec<u8> {
    let denom = if returned_to_source {
        "transfer/channel-0/upenumbra"
    } else {
        "upenumbra"
    };
    let data = FungibleTokenPacketData {
        denom: denom.to_string(),
        amount: "12345".to_string(),
        sender: "bankd1benchmarksender".to_string(),
        receiver: receiver.to_string(),
        memo: String::new(),
    };
    serde_json::to_vec(&data).expect("encode benchmark packet data")
}

fn recv_packet(returned_to_source: bool) -> MsgRecvPacket {
    let mut rng = ChaCha20Rng::seed_from_u64(7);
    let receiver = Address::dummy(&mut rng);
    MsgRecvPacket {
        packet: transfer_packet(1, packet_data(returned_to_source, &receiver)),
        proof_commitment_on_a: MerkleProof {
            proofs: vec![ics23::CommitmentProof::default()],
        },
        proof_height_on_a: Height::new(0, 999_999).expect("valid proof height"),
        signer: receiver.to_string(),
    }
}

fn plain_withdrawal() -> Ics20Withdrawal {
    let mut rng = ChaCha20Rng::seed_from_u64(11);
    Ics20Withdrawal {
        amount: Amount::from(12345u64),
        denom: "upenumbra".try_into().expect("valid benchmark denom"),
        destination_chain_address: "bankd1benchmarkreceiver".to_string(),
        return_address: Address::dummy(&mut rng),
        timeout_height: Height::new(0, 1_000_000).expect("valid timeout height"),
        timeout_time: 60_000_000_000,
        source_channel: channel(0),
        use_compat_address: false,
        ics20_memo: String::new(),
        use_transparent_address: false,
    }
}

fn preseed_receipts(
    runtime: &tokio::runtime::Runtime,
    historical_receipts: usize,
) -> cnidarium::TempStorage {
    runtime.block_on(async {
        let storage = cnidarium::TempStorage::new()
            .await
            .expect("create benchmark storage");
        if historical_receipts > 0 {
            let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
            for sequence in 1..=historical_receipts as u64 {
                state.put_packet_receipt(&transfer_packet(sequence, Vec::new()));
            }
            storage
                .commit(state)
                .await
                .expect("commit preseeded packet receipts");
        }
        storage
    })
}

fn commit_receipt_block(
    runtime: &tokio::runtime::Runtime,
    storage: cnidarium::TempStorage,
    start_sequence: u64,
    packet_count: usize,
) -> Duration {
    runtime.block_on(async move {
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        for offset in 0..packet_count as u64 {
            state.put_packet_receipt(&transfer_packet(start_sequence + offset, Vec::new()));
        }
        let start = std::time::Instant::now();
        storage
            .commit(state)
            .await
            .expect("commit benchmark packet receipts");
        start.elapsed()
    })
}

fn bench_recv_context(c: &mut Criterion) {
    let mut group = c.benchmark_group("ibc_recv_only_preupdated_context");
    for returned_to_source in [false, true] {
        let branch = if returned_to_source {
            "return_source"
        } else {
            "sink_zone"
        };
        group.bench_function(branch, |b| {
            b.iter_batched(
                || recv_packet(returned_to_source),
                |packet| black_box(benchmark_parse_ics20_receive_context(&packet).unwrap()),
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn bench_outbound_plain(c: &mut Criterion) {
    let mut group = c.benchmark_group("ibc_outbound_plain_smoke");
    group.bench_function("ics20_withdrawal_packet_data", |b| {
        b.iter_batched(
            plain_withdrawal,
            |withdrawal| black_box(withdrawal.packet_data()),
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_receipt_commit_curve(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().expect("create benchmark runtime");
    let historical_counts = configured_sizes("PENUMBRA_IBC_BENCH_RECEIPT_HISTORY", &[0, 10_000]);
    let packet_counts = configured_sizes("PENUMBRA_IBC_BENCH_BLOCK_PACKETS", &[100]);
    let mut group = c.benchmark_group("ibc_jmt_receipt_commit_curve");

    for historical_receipts in historical_counts {
        for packet_count in &packet_counts {
            group.bench_with_input(
                BenchmarkId::new(
                    format!("historical_receipts_{historical_receipts}"),
                    packet_count,
                ),
                &(historical_receipts, *packet_count),
                |b, &(historical_receipts, packet_count)| {
                    b.iter_custom(|iters| {
                        let mut elapsed = Duration::ZERO;
                        for iter in 0..iters {
                            let storage = preseed_receipts(&runtime, historical_receipts);
                            let start_sequence =
                                historical_receipts as u64 + iter * packet_count as u64 + 1;
                            elapsed += commit_receipt_block(
                                &runtime,
                                storage,
                                start_sequence,
                                packet_count,
                            );
                        }
                        elapsed
                    })
                },
            );
        }
    }
    group.finish();
}

fn bench_shielded_withdrawal(c: &mut Criterion) {
    if std::env::var("PENUMBRA_IBC_BENCH_SHIELDED_PROOF").as_deref() != Ok("1") {
        return;
    }

    let (public, private) = benchmark_shielded_ics20_withdrawal_roundtrip_inputs(
        ShieldedIcs20WithdrawalFamilyId::Canonical,
        false,
    );
    let proof = ShieldedIcs20WithdrawalProof::prove(public.clone(), private)
        .expect("build benchmark shielded ICS-20 withdrawal proof");

    let mut group = c.benchmark_group("ibc_outbound_shielded_smoke");
    group.sample_size(10);
    group.bench_function("shielded_ics20_withdrawal_proof_verify", |b| {
        b.iter(|| black_box(proof.verify(&public).unwrap()))
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_recv_context,
    bench_outbound_plain,
    bench_receipt_commit_curve,
    bench_shielded_withdrawal,
);
criterion_main!(benches);
