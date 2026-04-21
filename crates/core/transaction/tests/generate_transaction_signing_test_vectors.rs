use decaf377::Fr;
use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey, VerificationKeyBytes};
use ed25519_consensus::SigningKey as Ed25519SigningKey;
use ibc_proto::ics23::CommitmentProof;
use ibc_types::core::{
    channel::{msgs::MsgRecvPacket, packet::Sequence, ChannelId, Packet, PortId},
    client::Height,
    commitment::MerkleProof,
};
use ibc_types::timestamp::Timestamp;
use penumbra_sdk_asset::{asset::Id, BASE_ASSET_DENOM};
use penumbra_sdk_fee::Fee;
use penumbra_sdk_governance::{
    Proposal, ProposalPayload, ProposalSubmit, ProposalSubmitBody, ValidatorVote,
    ValidatorVoteBody, ValidatorVoteReason, Vote,
};
use penumbra_sdk_ibc::IbcRelay;
use penumbra_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
use penumbra_sdk_keys::test_keys::SEED_PHRASE;
use penumbra_sdk_keys::{Address, FullViewingKey};
use penumbra_sdk_num::Amount;
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_shielded_pool::{
    ConsolidateFamilyId, ConsolidatePlan, Ics20Withdrawal, Note, ShieldedIcs20WithdrawalFamilyId,
    ShieldedIcs20WithdrawalPlan, ShieldedInputPlan, ShieldedOutputPlan, SplitFamilyId, SplitPlan,
    TransferPlan,
};
use penumbra_sdk_transaction::{
    check_transaction_plan_enabled, ActionPlan, TransactionParameters, TransactionPlan,
};
use penumbra_sdk_validator::{validator, validator::Definition, GovernanceKey, IdentityKey};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, TestRunner};
use rand_core::OsRng;
use std::io::Write;
use std::str::FromStr;
use std::{fs::File, io::Read};
use tendermint;

fn amount_strategy() -> impl Strategy<Value = Amount> {
    let inner_uint_range = 0u128..1_000_000_000_000_000_000u128;
    inner_uint_range.prop_map(|uint| Amount::from_le_bytes(uint.to_le_bytes()))
}

fn asset_id_strategy() -> impl Strategy<Value = Id> {
    Just(*penumbra_sdk_asset::BASE_ASSET_ID)
}

fn value_strategy() -> impl Strategy<Value = penumbra_sdk_asset::Value> {
    (asset_id_strategy(), amount_strategy())
        .prop_map(|(asset_id, amount)| penumbra_sdk_asset::Value { amount, asset_id })
}

fn address_strategy() -> impl Strategy<Value = Address> {
    // normally we would use address::dummy, but this seems to not work properly
    // for some reason (invalid key errors on computing effecthash.)
    prop::strategy::LazyJust::new(|| {
        let seed_phrase = SeedPhrase::generate(&mut OsRng);
        let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let addr = sk.full_viewing_key().payment_address(0u32.into()).0;

        addr
    })
}

fn note_strategy(addr: Address) -> impl Strategy<Value = Note> {
    value_strategy().prop_map(move |value| Note::generate(&mut OsRng, &addr, value))
}

fn spend_plan_strategy(fvk: &FullViewingKey) -> impl Strategy<Value = ShieldedInputPlan> {
    let tct_strategy = any::<penumbra_sdk_tct::Position>();
    let note_strategy = note_strategy(fvk.incoming().payment_address(0u32.into()).0);

    (tct_strategy, note_strategy)
        .prop_map(|(tct_pos, note)| ShieldedInputPlan::new(&mut OsRng, note, tct_pos))
}

fn output_plan_strategy() -> impl Strategy<Value = ShieldedOutputPlan> {
    (value_strategy(), address_strategy())
        .prop_map(|(value, address)| ShieldedOutputPlan::new(&mut OsRng, value, address))
}

fn identity_key_strategy() -> impl Strategy<Value = IdentityKey> {
    let rand_bytes = prop::array::uniform32(any::<u8>());

    rand_bytes.prop_map(|vk_bytes| IdentityKey(VerificationKeyBytes::<SpendAuth>::from(vk_bytes)))
}

fn signing_key_strategy() -> impl Strategy<Value = SigningKey<SpendAuth>> {
    prop::strategy::LazyJust::new(|| SigningKey::<SpendAuth>::new(OsRng))
}

fn consensus_secret_key_strategy() -> impl Strategy<Value = Ed25519SigningKey> {
    prop::strategy::LazyJust::new(|| Ed25519SigningKey::new(OsRng))
}

fn validator_strategy() -> impl Strategy<Value = (validator::Validator, SigningKey<SpendAuth>)> {
    (signing_key_strategy(), consensus_secret_key_strategy()).prop_map(
        move |(new_validator_id_sk, new_validator_consensus_sk)| {
            let new_validator_id = IdentityKey(VerificationKey::from(&new_validator_id_sk).into());
            let new_validator_consensus = new_validator_consensus_sk.verification_key();
            (
                validator::Validator {
                    identity_key: new_validator_id.clone(),
                    consensus_key: tendermint::PublicKey::from_raw_ed25519(
                        &new_validator_consensus.to_bytes(),
                    )
                    .expect("consensus key is valid"),
                    governance_key: GovernanceKey(new_validator_id_sk.into()),
                    enabled: true,
                    sequence_number: 0,
                    name: "test validator".to_string(),
                    website: String::default(),
                    description: String::default(),
                },
                new_validator_id_sk,
            )
        },
    )
}

fn validator_definition_strategy() -> impl Strategy<Value = Definition> {
    (validator_strategy()).prop_map(|(new_validator, new_validator_id_sk)| {
        let bytes = new_validator.encode_to_vec();
        let auth_sig = new_validator_id_sk.sign(OsRng, &bytes);
        Definition {
            validator: new_validator,
            auth_sig,
        }
    })
}

fn sequence_strategy() -> impl Strategy<Value = Sequence> {
    (4001..2000000000u64).prop_map(Sequence)
}

fn ibc_action_strategy() -> impl Strategy<Value = IbcRelay> {
    (
        sequence_strategy(),
        0..1000000000u64,
        0..1000000000u64,
        address_strategy(),
    )
        .prop_map(|(sequence, revision_number, revision_height, src)| {
            IbcRelay::RecvPacket(MsgRecvPacket {
                packet: Packet {
                    sequence,
                    port_on_a: PortId::default(),
                    chan_on_a: ChannelId::default(),
                    port_on_b: PortId::default(),
                    chan_on_b: ChannelId::default(),
                    data: vec![0u8; 100],
                    timeout_height_on_b: ibc_types::core::channel::TimeoutHeight::At(
                        Height::new(revision_number, revision_height).expect("test value"),
                    ),
                    timeout_timestamp_on_b: Timestamp::now(),
                },
                // this can't be empty
                proof_commitment_on_a: MerkleProof {
                    proofs: vec![CommitmentProof::default()],
                },
                proof_height_on_a: Height::new(revision_number, revision_height)
                    .expect("test value"),
                signer: src.to_string(),
            })
        })
}

fn proposal_strategy() -> impl Strategy<Value = Proposal> {
    (
        prop::string::string_regex(r"[a-z]+-[0-9]+").unwrap(),
        prop::string::string_regex(r"[a-z]+-[0-9]+").unwrap(),
    )
        .prop_map(|(title, description)| Proposal {
            id: 0u64,
            title,
            description,
            payload: ProposalPayload::Signaling { commit: None },
        })
}

fn proposal_id_strategy() -> impl Strategy<Value = u64> {
    0u64..1000000000u64
}

fn proposal_submit_strategy() -> impl Strategy<Value = ProposalSubmit> {
    (
        proposal_strategy(),
        identity_key_strategy(),
        signing_key_strategy(),
    )
        .prop_map(|(proposal, proposer, signing_key)| {
            let governance_key = GovernanceKey(signing_key.into());
            let body = ProposalSubmitBody {
                proposal,
                proposer,
                governance_key,
            };
            let auth_sig = signing_key.sign(OsRng, &body.encode_to_vec());
            ProposalSubmit { body, auth_sig }
        })
}

fn vote_strategy() -> impl Strategy<Value = Vote> {
    prop_oneof![Just(Vote::Yes), Just(Vote::No), Just(Vote::Abstain),]
}

fn validator_vote_strategy() -> impl Strategy<Value = ValidatorVote> {
    (
        proposal_id_strategy(),
        vote_strategy(),
        identity_key_strategy(),
        signing_key_strategy(),
        prop::string::string_regex(r"[a-zA-Z0-9]+").unwrap(),
    )
        .prop_map(|(proposal, vote, identity_key, signing_key, reason)| {
            let governance_key = GovernanceKey(signing_key.into());
            let body = ValidatorVoteBody {
                proposal,
                vote,
                identity_key,
                governance_key,
                reason: ValidatorVoteReason(reason),
            };

            let bytes = body.encode_to_vec();
            let auth_sig = signing_key.sign(OsRng, &bytes);
            ValidatorVote { body, auth_sig }
        })
}

fn shielded_ics20_withdrawal_plan_strategy(
    fvk: &FullViewingKey,
) -> impl Strategy<Value = ShieldedIcs20WithdrawalPlan> {
    let note_strategy = note_strategy(fvk.incoming().payment_address(0u32.into()).0);
    let position_strategy = any::<penumbra_sdk_tct::Position>();

    (
        note_strategy,
        position_strategy,
        address_strategy(),
        0..1000000000u64,
        0..1000000000u64,
    )
        .prop_map(
            |(note, position, return_address, revision_number, revision_height)| {
                let withdrawal = Ics20Withdrawal {
                    amount: note.amount(),
                    denom: BASE_ASSET_DENOM.clone(),
                    destination_chain_address: return_address.to_string(),
                    return_address: return_address.clone(),
                    timeout_height: Height::new(revision_number, revision_height)
                        .expect("test value"),
                    timeout_time: 0u64,
                    source_channel: ChannelId::default(),
                    use_compat_address: false,
                    use_transparent_address: false,
                    ics20_memo: String::default(),
                };
                ShieldedIcs20WithdrawalPlan::new(
                    ShieldedIcs20WithdrawalFamilyId::Canonical,
                    vec![ShieldedInputPlan::new(&mut OsRng, note, position)],
                    None,
                    withdrawal,
                    Fr::rand(&mut OsRng),
                )
                .expect("valid shielded ICS-20 withdrawal plan")
            },
        )
}

fn transfer_plan_strategy(fvk: &FullViewingKey) -> impl Strategy<Value = TransferPlan> {
    (spend_plan_strategy(fvk), output_plan_strategy()).prop_map(|(spend, output)| {
        TransferPlan::from_spend_output(spend.into(), output.into(), Fr::rand(&mut OsRng))
            .expect("valid transfer plan")
    })
}

fn consolidate_plan_strategy(fvk: &FullViewingKey) -> impl Strategy<Value = ConsolidatePlan> {
    let addr = fvk.incoming().payment_address(0u32.into()).0;
    (
        note_strategy(addr.clone()),
        any::<penumbra_sdk_tct::Position>(),
        note_strategy(addr.clone()),
        any::<penumbra_sdk_tct::Position>(),
    )
        .prop_map(move |(note_1, pos_1, note_2, pos_2)| {
            let total_amount = note_1.amount() + note_2.amount();
            let output = ShieldedOutputPlan::new(
                &mut OsRng,
                penumbra_sdk_asset::Value {
                    amount: total_amount,
                    asset_id: note_1.asset_id(),
                },
                addr.clone(),
            );
            ConsolidatePlan::new(
                ConsolidateFamilyId::TwoByOne,
                vec![
                    ShieldedInputPlan::new(&mut OsRng, note_1, pos_1).into(),
                    ShieldedInputPlan::new(&mut OsRng, note_2, pos_2).into(),
                ],
                vec![output.into()],
                Fr::rand(&mut OsRng),
            )
            .expect("valid consolidate plan")
        })
}

fn split_plan_strategy(fvk: &FullViewingKey) -> impl Strategy<Value = SplitPlan> {
    let addr = fvk.incoming().payment_address(0u32.into()).0;
    (
        note_strategy(addr.clone()),
        any::<penumbra_sdk_tct::Position>(),
    )
        .prop_map(move |(note, position)| {
            let quarter = note.amount() / Amount::from(4u64);
            let outputs = vec![
                ShieldedOutputPlan::new(
                    &mut OsRng,
                    penumbra_sdk_asset::Value {
                        amount: quarter,
                        asset_id: note.asset_id(),
                    },
                    addr.clone(),
                )
                .into(),
                ShieldedOutputPlan::new(
                    &mut OsRng,
                    penumbra_sdk_asset::Value {
                        amount: quarter,
                        asset_id: note.asset_id(),
                    },
                    addr.clone(),
                )
                .into(),
                ShieldedOutputPlan::new(
                    &mut OsRng,
                    penumbra_sdk_asset::Value {
                        amount: quarter,
                        asset_id: note.asset_id(),
                    },
                    addr.clone(),
                )
                .into(),
                ShieldedOutputPlan::new(
                    &mut OsRng,
                    penumbra_sdk_asset::Value {
                        amount: note.amount() - quarter - quarter - quarter,
                        asset_id: note.asset_id(),
                    },
                    addr.clone(),
                )
                .into(),
            ];

            SplitPlan::new(
                SplitFamilyId::OneByFour,
                vec![ShieldedInputPlan::new(&mut OsRng, note, position).into()],
                outputs,
                Fr::rand(&mut OsRng),
            )
            .expect("valid split plan")
        })
}

fn action_plan_strategy(fvk: &FullViewingKey) -> impl Strategy<Value = ActionPlan> {
    prop_oneof![
        transfer_plan_strategy(fvk).prop_map(ActionPlan::Transfer),
        consolidate_plan_strategy(fvk).prop_map(ActionPlan::Consolidate),
        split_plan_strategy(fvk).prop_map(ActionPlan::Split),
        validator_definition_strategy().prop_map(ActionPlan::ValidatorDefinition),
        proposal_submit_strategy().prop_map(ActionPlan::ProposalSubmit),
        ibc_action_strategy().prop_map(ActionPlan::IbcAction),
        validator_vote_strategy().prop_map(ActionPlan::ValidatorVote),
        shielded_ics20_withdrawal_plan_strategy(fvk).prop_map(ActionPlan::ShieldedIcs20Withdrawal),
    ]
}

fn actions_vec_strategy(fvk: &FullViewingKey) -> impl Strategy<Value = Vec<ActionPlan>> {
    prop::collection::vec(action_plan_strategy(fvk), 2..5)
}

fn transaction_parameters_strategy() -> impl Strategy<Value = TransactionParameters> {
    let expiry_height = 0u64..10000000000u64;
    let chain_id = prop::string::string_regex(r"[a-z]+-[0-9]+").unwrap();
    let fee = value_strategy().prop_map(|fee_value| Fee(fee_value));

    (expiry_height, chain_id, fee).prop_map(|(expiry_height, chain_id, fee)| {
        TransactionParameters {
            expiry_height,
            chain_id,
            fee,
        }
    })
}

fn transaction_plan_strategy(fvk: &FullViewingKey) -> impl Strategy<Value = TransactionPlan> {
    (actions_vec_strategy(fvk), transaction_parameters_strategy()).prop_map(|(actions, params)| {
        TransactionPlan {
            actions,
            transaction_parameters: params,
            fee_funding: None,
            detection_data: None,
            memo: None,
        }
    })
}

#[test]
#[ignore]
fn generate_transaction_signing_test_vectors() {
    // Run this to regenerate the `EffectHash` test vectors. Ignored by default.
    let mut runner = TestRunner::new(Config::default());
    let test_vectors_dir = "tests/signing_test_vectors";
    std::fs::create_dir_all(test_vectors_dir).expect("failed to create test vectors dir");

    let mut i = 0;
    while i < 100 {
        let seed_phrase = SeedPhrase::from_str(SEED_PHRASE).expect("test seed phrase is valid");
        let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
        let fvk = sk.full_viewing_key();
        let value_tree = transaction_plan_strategy(fvk)
            .new_tree(&mut runner)
            .expect("Failed to create new tree");
        let transaction_plan = value_tree.current();

        if check_transaction_plan_enabled(&transaction_plan).is_err() {
            continue;
        }

        let json_plan = serde_json::to_string_pretty(&transaction_plan)
            .expect("should be able to json tx plan");

        let transaction_plan_encoded = transaction_plan.encode_to_vec();
        let effect_hash_hex = hex::encode(
            transaction_plan
                .effect_hash(fvk)
                .expect("should be able to compute effect hash")
                .0,
        );

        let json_file_path = format!("{}/transaction_plan_{}.json", test_vectors_dir, i);
        let proto_file_path = format!("{}/transaction_plan_{}.proto", test_vectors_dir, i);
        let hash_file_path = format!("{}/effect_hash_{}.txt", test_vectors_dir, i);

        let mut json_file = File::create(&json_file_path).expect("Failed to create JSON file");
        json_file
            .write_all(json_plan.as_bytes())
            .expect("Failed to write JSON file");
        let mut proto_file =
            File::create(&proto_file_path).expect("Failed to create Protobuf file");
        proto_file
            .write_all(&transaction_plan_encoded)
            .expect("Failed to write Protobuf file");

        // Write effect hash
        let mut hash_file = File::create(&hash_file_path).expect("Failed to create hash file");
        hash_file
            .write_all(effect_hash_hex.as_bytes())
            .expect("Failed to write hash file");

        i += 1;
    }
}

#[test]
fn effect_hash_test_vectors() {
    // This parses the transaction plan, computes the effect hash, and verifies that it
    // matches the expected effect hash.
    let test_vectors_dir = "tests/signing_test_vectors";
    let seed_phrase = SeedPhrase::from_str(SEED_PHRASE).expect("test seed phrase is valid");
    let sk = SpendKey::from_seed_phrase_bip44(seed_phrase, &Bip44Path::new(0));
    let fvk = sk.full_viewing_key();

    let mut supported_vectors = 0;
    for i in 0..100 {
        let proto_file_path = format!("{}/transaction_plan_{}.proto", test_vectors_dir, i);
        let mut proto_file = File::open(&proto_file_path).expect("Failed to open Protobuf file");
        let mut transaction_plan_encoded = Vec::<u8>::new();
        proto_file
            .read_to_end(&mut transaction_plan_encoded)
            .expect("Failed to read Protobuf file");
        let Ok(transaction_plan) = TransactionPlan::decode(&transaction_plan_encoded[..]) else {
            continue;
        };

        if check_transaction_plan_enabled(&transaction_plan).is_err() {
            continue;
        }

        let effect_hash_hex = hex::encode(
            transaction_plan
                .effect_hash(fvk)
                .expect("should be able to compute effect hash")
                .0,
        );

        let hash_file_path = format!("{}/effect_hash_{}.txt", test_vectors_dir, i);
        let expected_effect_hash = std::fs::read_to_string(&hash_file_path)
            .expect("should be able to read expected effect hash");
        assert_eq!(effect_hash_hex, expected_effect_hash);
        supported_vectors += 1;
    }

    assert!(
        supported_vectors > 0,
        "expected at least one enabled signing test vector"
    );
}
