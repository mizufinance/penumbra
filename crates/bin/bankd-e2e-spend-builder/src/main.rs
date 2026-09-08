#![deny(clippy::unwrap_used)]

use std::{env, fs, ops::Deref, path::PathBuf, str::FromStr};

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use cnidarium::Storage;
use decaf377::{Element, Encoding, Fq, Fr};
use decaf377_rdsa::{SigningKey, SpendAuth, VerificationKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use shieldd_sdk_app::SUBSTORE_PREFIXES;
use shieldd_sdk_asset::{asset, Value};
use shieldd_sdk_compliance::{
    structs::{
        AssetRegistrationGrant, AssetRegistrationGrantBody, MsgRegisterAsset, MsgRegisterUser,
        OrbisCapabilityCertificate, UserRegistrationGrant, UserRegistrationGrantBody,
    },
    ComplianceLeaf, DetectionKey, PocOrbisAuditBundle,
};
use shieldd_sdk_keys::{keys::SpendKey, test_keys, Address};
use shieldd_sdk_mock_client::MockClient;
use shieldd_sdk_num::Amount;
use shieldd_sdk_proto::DomainType;
use shieldd_sdk_shielded_pool::{ShieldedInputPlan, ShieldedOutputPlan};
use shieldd_sdk_transaction::{
    memo::MemoPlaintext, plan::MemoPlan, ActionPlan, Transaction, TransactionParameters,
    TransactionPlan,
};

struct Opt {
    db: PathBuf,
    chain_id: String,
    operation: Operation,
}

struct BuiltTx {
    tx: Transaction,
    audit_bundle: Option<PocOrbisAuditBundle>,
}

enum Operation {
    Spend {
        send_amount: Amount,
        recipient: Address,
        sender_spend_key: Option<SpendKey>,
        audit_bundle: Option<PathBuf>,
    },
    RegisterAsset {
        ring: PathBuf,
    },
    RegisterUsers {
        ring: PathBuf,
        registrations: PathBuf,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RingInput {
    ring_pk_hex: String,
    ring_id: String,
    policy_id: String,
    resource: String,
    permission: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserRegistrationInput {
    address: Address,
    rnk_dh_pk_hex: String,
    rnk_commitment_hex: String,
    capability_certificate_hex: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditBundleOutput {
    shieldd_tx_hash: String,
    action_index: u32,
    output_index: u32,
    bundle: PocOrbisAuditBundle,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opt = parse_args()?;
    let built = build_tx(&opt).await?;

    if let Operation::Spend {
        audit_bundle: Some(path),
        ..
    } = &opt.operation
    {
        write_audit_bundle(
            path,
            &built.tx,
            built
                .audit_bundle
                .context("regulated transfer did not produce an Orbis audit bundle")?,
        )?;
    }

    println!("{}", STANDARD.encode(built.tx.encode_to_vec()));
    Ok(())
}

fn parse_args() -> Result<Opt> {
    let mut args = env::args().skip(1);
    let db = PathBuf::from(
        args.next()
            .context("usage: bankd-e2e-spend-builder <db> <chain-id> [send-amount]")?,
    );
    let chain_id = args
        .next()
        .context("usage: bankd-e2e-spend-builder <db> <chain-id> [send-amount]")?;
    let operation = match args.next().as_deref() {
        Some("register-asset") => Operation::RegisterAsset {
            ring: PathBuf::from(args.next().context(
                "usage: bankd-e2e-spend-builder <db> <chain-id> register-asset <ring.json>",
            )?),
        },
        Some("register-users") => {
            let ring = PathBuf::from(args.next().context(
                "usage: bankd-e2e-spend-builder <db> <chain-id> register-users <ring.json> <registrations.json>",
            )?);
            let registrations = PathBuf::from(
                args.next()
                    .context("register-users requires wallet-provided registrations.json")?,
            );
            Operation::RegisterUsers {
                ring,
                registrations,
            }
        }
        Some("spend") => {
            let send_amount = parse_amount(args.next())?;
            let mut audit_bundle = None;
            let mut recipient = test_keys::ADDRESS_1.deref().clone();
            let mut sender_spend_key = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--audit-bundle" => {
                        audit_bundle = Some(PathBuf::from(
                            args.next().context("--audit-bundle requires a path")?,
                        ));
                    }
                    "--recipient" => recipient = parse_address(args.next())?,
                    "--sender-spend-key-file" => {
                        sender_spend_key = Some(read_spend_key_file(PathBuf::from(
                            args.next()
                                .context("--sender-spend-key-file requires a path")?,
                        ))?)
                    }
                    _ => bail!("unknown spend argument {arg}"),
                }
            }
            Operation::Spend {
                send_amount,
                recipient,
                sender_spend_key,
                audit_bundle,
            }
        }
        // Preserve the original e2e interface where the optional third argument
        // is the send amount.
        Some(raw) => Operation::Spend {
            send_amount: parse_amount(Some(raw.to_owned()))?,
            recipient: test_keys::ADDRESS_1.deref().clone(),
            sender_spend_key: None,
            audit_bundle: None,
        },
        None => Operation::Spend {
            send_amount: Amount::from(1u64),
            recipient: test_keys::ADDRESS_1.deref().clone(),
            sender_spend_key: None,
            audit_bundle: None,
        },
    };

    if args.next().is_some() {
        bail!("unexpected extra arguments");
    }

    Ok(Opt {
        db,
        chain_id,
        operation,
    })
}

fn parse_amount(raw: Option<String>) -> Result<Amount> {
    Ok(Amount::from(match raw {
        Some(raw) => raw
            .parse::<u64>()
            .with_context(|| format!("invalid send amount {raw}"))?,
        None => 1,
    }))
}

fn parse_address(raw: Option<String>) -> Result<Address> {
    let raw = raw.context("--recipient requires an address")?;
    Address::from_str(&raw).with_context(|| format!("invalid recipient address {raw}"))
}

fn read_spend_key_file(path: PathBuf) -> Result<SpendKey> {
    let config = fs::read_to_string(&path)
        .with_context(|| format!("failed to read sender wallet config {}", path.display()))?;
    let encoded = config
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("spend_key = "))
        .map(|value| value.trim_matches('"'))
        .context("sender wallet config is missing custody.spend_key")?;
    SpendKey::from_str(encoded).context("sender wallet config contains an invalid spend key")
}

async fn build_tx(opt: &Opt) -> Result<BuiltTx> {
    let storage = Storage::load(opt.db.clone(), SUBSTORE_PREFIXES.to_vec())
        .await
        .with_context(|| format!("failed to open Shieldd RocksDB at {}", opt.db.display()))?;

    let spend_key = match &opt.operation {
        Operation::Spend {
            sender_spend_key: Some(spend_key),
            ..
        } => spend_key.clone(),
        _ => test_keys::SPEND_KEY.clone(),
    };
    let mut client = MockClient::new(spend_key);
    client
        .sync_to_latest(storage.latest_snapshot())
        .await
        .context("failed to sync Shieldd test wallet to storage")?;

    match &opt.operation {
        Operation::Spend {
            send_amount,
            recipient,
            ..
        } => {
            build_spend_tx(
                &client,
                &storage,
                &opt.chain_id,
                *send_amount,
                recipient.clone(),
            )
            .await
        }
        Operation::RegisterAsset { ring } => Ok(BuiltTx {
            tx: build_registration_tx(
                &client,
                &opt.chain_id,
                register_asset_action(read_ring(ring)?)?,
            )
            .await?,
            audit_bundle: None,
        }),
        Operation::RegisterUsers {
            ring,
            registrations,
        } => {
            let ring = read_ring(ring)?;
            let registrations: Vec<UserRegistrationInput> =
                serde_json::from_slice(&fs::read(registrations).with_context(|| {
                    format!(
                        "failed to read registrations file {}",
                        registrations.display()
                    )
                })?)
                .context("failed to parse wallet registration bundles")?;
            let actions = registrations
                .into_iter()
                .enumerate()
                .map(|(index, registration)| {
                    register_user_action(&ring, &opt.chain_id, registration, index as u8)
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(BuiltTx {
                tx: build_registration_tx(&client, &opt.chain_id, actions).await?,
                audit_bundle: None,
            })
        }
    }
}

async fn build_spend_tx(
    client: &MockClient,
    storage: &Storage,
    chain_id: &str,
    send_amount: Amount,
    recipient: Address,
) -> Result<BuiltTx> {
    let input_note = client
        .notes
        .values()
        .filter(|note| !client.spent_note(&note.commit()))
        .filter(|note| note.amount() >= send_amount)
        .cloned()
        .next()
        .ok_or_else(|| {
            anyhow!(
                "no spendable note for the selected Shieldd sender covering {}",
                send_amount,
            )
        })?;

    let position = client
        .position(input_note.commit())
        .ok_or_else(|| anyhow!("input note commitment was unknown to mock client"))?;
    let spend = ShieldedInputPlan::new(&mut OsRng, input_note.clone(), position);
    let change_amount = input_note
        .amount()
        .checked_sub(&send_amount)
        .context("input note amount must cover send amount")?;

    let output = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: send_amount,
            asset_id: input_note.asset_id(),
        },
        recipient,
    );
    let change = ShieldedOutputPlan::new(
        &mut OsRng,
        Value {
            amount: change_amount,
            asset_id: input_note.asset_id(),
        },
        input_note.address(),
    );

    let transfer = shieldd_sdk_mock_client::TransferIntent {
        spends: vec![spend],
        outputs: vec![output, change],
        value_blinding: Fr::from(1u64),
    };
    let intent = shieldd_sdk_mock_client::TransactionIntent {
        actions: vec![transfer.into()],
        memo: Some(MemoPlan::new(
            &mut OsRng,
            MemoPlaintext::blank_memo(input_note.address()),
        )),
        fee_funding: None,
        transaction_parameters: TransactionParameters {
            chain_id: chain_id.to_owned(),
            ..Default::default()
        },
        nullifier_window: None,
    };

    let plan = client
        .complete_intent(intent, storage.latest_snapshot())
        .await?;
    let tx = client
        .witness_auth_build(&plan)
        .await
        .context("failed to build Shieldd spend transaction")?;
    let audit_bundle = match plan.actions.first() {
        Some(ActionPlan::Transfer(transfer)) => transfer.poc_orbis_audit_bundle()?,
        _ => None,
    };
    Ok(BuiltTx { tx, audit_bundle })
}

async fn build_registration_tx(
    client: &MockClient,
    chain_id: &str,
    actions: Vec<ActionPlan>,
) -> Result<Transaction> {
    let plan = TransactionPlan {
        actions,
        transaction_parameters: TransactionParameters {
            chain_id: chain_id.to_owned(),
            ..Default::default()
        },
        ..Default::default()
    };
    client
        .witness_auth_build(&plan)
        .await
        .context("failed to build Shieldd compliance registration transaction")
}

fn register_asset_action(ring: RingInput) -> Result<Vec<ActionPlan>> {
    let registrar_sk = demo_signing_key(1);
    let authority_vk = VerificationKey::from(&demo_signing_key(2));
    let seizure_authority_vk = VerificationKey::from(&demo_signing_key(3));
    let asset_id = asset::REGISTRY
        .parse_denom("ubrl")
        .expect("ubrl is a base denomination")
        .id();
    let body = AssetRegistrationGrantBody {
        asset_id,
        is_regulated: true,
        dk_pub: Some(DetectionKey::new(Fr::from(3u64)).public_key()),
        daily_volume_limit: Some(1),
        allowed_ibc_routes: vec![],
        ibc_origin: None,
        ring_pk: Some(parse_element(&ring.ring_pk_hex)?),
        ring_id: ring.ring_id,
        policy_id: ring.policy_id,
        permission: ring.permission,
        resource: ring.resource,
        registration_authority_vk: Some(authority_vk),
        seizure_authority_vk: Some(seizure_authority_vk),
        valid_until_unix: 4_102_444_800,
    };
    let grant = AssetRegistrationGrant {
        signature: registrar_sk.sign(OsRng, &body.signing_bytes()),
        registrar_vk: VerificationKey::from(&registrar_sk),
        body: body.clone(),
    };
    Ok(vec![ActionPlan::from(MsgRegisterAsset {
        asset_id: body.asset_id,
        is_regulated: body.is_regulated,
        dk_pub: body.dk_pub,
        daily_volume_limit: body.daily_volume_limit,
        allowed_ibc_routes: body.allowed_ibc_routes,
        ibc_origin: body.ibc_origin,
        ring_pk: body.ring_pk,
        ring_id: body.ring_id,
        policy_id: body.policy_id,
        permission: body.permission,
        resource: body.resource,
        registration_authority_vk: body.registration_authority_vk,
        seizure_authority_vk: body.seizure_authority_vk,
        asset_registration_grant: Some(grant),
    })])
}

fn register_user_action(
    ring: &RingInput,
    chain_id: &str,
    registration: UserRegistrationInput,
    nonce: u8,
) -> Result<ActionPlan> {
    let authority_sk = demo_signing_key(2);
    let asset_id = asset::REGISTRY
        .parse_denom("ubrl")
        .context("ubrl must be a base denomination")?
        .id();
    let ring_pk = parse_element(&ring.ring_pk_hex)?;
    let rnk_dh_pk = parse_element(&registration.rnk_dh_pk_hex)?;
    let rnk_commitment = parse_fq(&registration.rnk_commitment_hex)?;
    let leaf = ComplianceLeaf::registered(
        registration.address,
        asset_id,
        ring_pk,
        rnk_dh_pk,
        rnk_commitment,
    )?;
    let capability_certificate = OrbisCapabilityCertificate::decode(
        hex::decode(&registration.capability_certificate_hex)
            .context("capability certificate must be hex")?
            .as_slice(),
    )?;
    let policy = shieldd_sdk_compliance::AssetPolicy::new(
        Element::GENERATOR,
        u128::MAX,
        vec![],
        None,
        ring.ring_id.clone(),
        ring_pk,
        ring.policy_id.clone(),
        ring.permission.clone(),
        ring.resource.clone(),
    );
    capability_certificate.verify(&leaf, &policy, chain_id)?;
    let body = UserRegistrationGrantBody {
        leaf: leaf.clone(),
        policy_id: ring.policy_id.clone(),
        valid_until_unix: 4_102_444_800,
        nonce: vec![nonce; 16],
    };
    Ok(ActionPlan::from(MsgRegisterUser {
        leaf: leaf.clone(),
        capability_certificate: Some(capability_certificate),
        grant: Some(UserRegistrationGrant {
            signature: authority_sk.sign(OsRng, &body.signing_bytes()),
            body,
        }),
    }))
}

fn demo_signing_key(value: u8) -> SigningKey<SpendAuth> {
    let mut bytes = [0u8; 32];
    bytes[0] = value;
    SigningKey::try_from(bytes.as_slice()).expect("nonzero demo signing key must be valid")
}

fn parse_element(value: &str) -> Result<Element> {
    let bytes: [u8; 32] = hex::decode(value)
        .context("ringPkHex must be hex")?
        .try_into()
        .map_err(|_| anyhow!("ringPkHex must encode 32 bytes"))?;
    Encoding(bytes)
        .vartime_decompress()
        .map_err(|_| anyhow!("ringPkHex is not a valid decaf377 element"))
}

fn parse_fq(value: &str) -> Result<Fq> {
    let bytes: [u8; 32] = hex::decode(value)
        .context("rnkCommitmentHex must be hex")?
        .try_into()
        .map_err(|_| anyhow!("rnkCommitmentHex must encode 32 bytes"))?;
    Fq::from_bytes_checked(&bytes)
        .map_err(|_| anyhow!("rnkCommitmentHex is not a canonical field element"))
}

fn read_ring(path: &PathBuf) -> Result<RingInput> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", path.display()))
}

fn write_audit_bundle(path: &PathBuf, tx: &Transaction, bundle: PocOrbisAuditBundle) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let output = AuditBundleOutput {
        shieldd_tx_hash: tx.id().to_string(),
        action_index: 0,
        output_index: 0,
        bundle,
    };
    fs::write(path, serde_json::to_vec_pretty(&output)?)
        .with_context(|| format!("failed to write {}", path.display()))
}
