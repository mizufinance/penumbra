//! Compliance decoding and completion of wallet intent.
use anyhow::Result;
use decaf377::Fr;
use shieldd_sdk_asset::asset;
use shieldd_sdk_compliance::ComplianceQuery;
use shieldd_sdk_compliance::{
    AssetPolicy, AssetProofData, BatchComplianceData, ComplianceLeaf, MerklePath, UserProofData,
};
use shieldd_sdk_keys::Address;
use shieldd_sdk_proto::view::v1 as view_pb;
use shieldd_sdk_tct::StateCommitment;
use shieldd_sdk_transaction::plan::{ActionPlan, TransactionPlan};
use std::collections::{BTreeMap, BTreeSet};

/// Convert a proto MerklePath to native MerklePath.
fn parse_proto_merkle_path(
    path: Option<shieldd_sdk_proto::core::component::compliance::v1::MerklePath>,
    label: &str,
) -> Result<shieldd_sdk_compliance::structs::MerklePath> {
    path.ok_or_else(|| anyhow::anyhow!("missing {label}"))?
        .try_into()
        .map_err(|error| anyhow::anyhow!("invalid {label}: {error}"))
}

pub(crate) fn parse_batch_compliance(
    queries: &[ComplianceQuery],
    batch_response: view_pb::ComplianceBatchMerkleProofsResponse,
    asset_policies: BTreeMap<asset::Id, AssetPolicy>,
) -> Result<BatchComplianceData> {
    anyhow::ensure!(
        batch_response.results.len() == queries.len(),
        "batch compliance response count {} does not match query count {}",
        batch_response.results.len(),
        queries.len()
    );

    // Parse anchors
    let compliance_anchor_bytes: [u8; 32] =
        batch_response
            .compliance_anchor
            .try_into()
            .map_err(|v: Vec<u8>| {
                anyhow::anyhow!(
                    "batch response: compliance_anchor must be 32 bytes, got {}",
                    v.len()
                )
            })?;
    let compliance_anchor = StateCommitment(
        decaf377::Fq::from_bytes_checked(&compliance_anchor_bytes)
            .map_err(|e| anyhow::anyhow!("batch response: invalid compliance_anchor: {}", e))?,
    );

    let asset_anchor_bytes: [u8; 32] =
        batch_response
            .asset_anchor
            .try_into()
            .map_err(|v: Vec<u8>| {
                anyhow::anyhow!(
                    "batch response: asset_anchor must be 32 bytes, got {}",
                    v.len()
                )
            })?;
    let asset_anchor = StateCommitment(
        decaf377::Fq::from_bytes_checked(&asset_anchor_bytes)
            .map_err(|e| anyhow::anyhow!("batch response: invalid asset_anchor: {}", e))?,
    );

    let mut asset_proofs: BTreeMap<asset::Id, AssetProofData> = BTreeMap::new();
    let mut user_proofs: BTreeMap<(Address, asset::Id), UserProofData> = BTreeMap::new();

    // Match results with queries - parse directly since individual results don't have anchors
    for (i, result) in batch_response.results.into_iter().enumerate() {
        let ComplianceQuery { address, asset_id } = &queries[i];

        let compliance_path =
            parse_proto_merkle_path(result.compliance_path, "batch compliance_path")?;
        let asset_path = parse_proto_merkle_path(result.asset_path, "batch asset_path")?;

        // Cache asset proof
        if !asset_proofs.contains_key(asset_id) {
            // Parse indexed_leaf from proto response using TryFrom
            let indexed_leaf = if let Some(leaf_data) = result.asset_indexed_leaf {
                shieldd_sdk_compliance::IndexedLeaf::try_from(leaf_data).map_err(|e| {
                    anyhow::anyhow!("invalid indexed_leaf for asset {}: {}", asset_id, e)
                })?
            } else {
                anyhow::bail!(
                    "asset_indexed_leaf missing in batch response for asset {} \
                         (server returned incomplete data)",
                    asset_id
                );
            };

            asset_proofs.insert(
                *asset_id,
                AssetProofData {
                    auth_path: asset_path.clone(),
                    position: result.asset_position,
                    indexed_leaf,
                    is_regulated: result.is_regulated,
                },
            );
        }

        // Build user proof with leaf
        let key = (address.clone(), *asset_id);
        if !user_proofs.contains_key(&key) {
            if result.user_registered {
                let leaf = ComplianceLeaf::try_from(result.compliance_leaf.ok_or_else(|| {
                    anyhow::anyhow!("registered user is missing a compliance leaf")
                })?)?;
                user_proofs.insert(
                    key,
                    UserProofData {
                        auth_path: compliance_path,
                        position: result.compliance_position,
                        leaf,
                    },
                );
            } else if !result.is_regulated {
                let synthetic_leaf =
                    ComplianceLeaf::synthetic_unregulated(address.clone(), *asset_id);
                user_proofs.insert(
                    key,
                    UserProofData {
                        auth_path: MerklePath::default(),
                        position: 0,
                        leaf: synthetic_leaf,
                    },
                );
            } else {
                anyhow::bail!(
                    "user not registered in compliance tree for address {:?} and asset {:?}",
                    address,
                    asset_id
                );
            }
        }
    }

    Ok(shieldd_sdk_compliance::BatchComplianceData {
        compliance_anchor,
        asset_anchor,
        asset_proofs,
        asset_policies,
        user_proofs,
    })
}

/// Completes wallet intent with one batch of authenticated compliance witnesses.
pub async fn complete_plan_with_compliance<P, F>(
    intent: crate::planning_intent::TransactionIntent,
    fetch: P,
    rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
    routing: shieldd_sdk_shielded_pool::discovery::Parameters,
    timestamp_override: Option<u64>,
) -> Result<TransactionPlan>
where
    P: FnOnce(Vec<ComplianceQuery>) -> F,
    F: std::future::Future<Output = Result<BatchComplianceData>>,
{
    use crate::planning_intent::ActionIntent;
    use shieldd_sdk_shielded_pool::{
        NoteReshapeContext, NoteReshapePlan, ShieldedHostWithdrawalPlan,
        ShieldedIcs20WithdrawalPlan, WithdrawalContext,
    };
    let timestamp = match timestamp_override {
        Some(timestamp) => timestamp,
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    };
    let mut queries = BTreeSet::new();
    for action in &intent.actions {
        if matches!(action, ActionIntent::Complete(_)) {
            continue;
        }
        for spend in action.spends() {
            queries.insert((spend.note.address(), spend.note.asset_id()));
        }
        for output in action.outputs() {
            queries.insert((output.dest_address.clone(), output.value.asset_id));
        }
    }
    if let Some(fee) = &intent.fee_funding {
        for spend in &fee.spends {
            queries.insert((spend.note.address(), spend.note.asset_id()));
        }
        for output in &fee.outputs {
            queries.insert((output.dest_address.clone(), output.value.asset_id));
        }
    }
    let batch = if queries.is_empty() {
        None
    } else {
        Some(
            fetch(
                queries
                    .into_iter()
                    .map(|(address, asset_id)| ComplianceQuery { address, asset_id })
                    .collect(),
            )
            .await?,
        )
    };
    let batch_ref = || {
        batch
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing compliance batch"))
    };
    let mut used_nonces = BTreeSet::new();
    for action in &intent.actions {
        if let ActionIntent::Complete(action) = action {
            let nonce = match action {
                ActionPlan::Transfer(plan) => Some(plan.compliance.nonce),
                ActionPlan::NoteReshape(plan) => Some(plan.compliance.nonce),
                ActionPlan::ShieldedIcs20Withdrawal(plan) => Some(plan.compliance.nonce),
                ActionPlan::ShieldedHostWithdrawal(plan) => Some(plan.compliance.nonce),
                _ => None,
            };
            if let Some(nonce) = nonce {
                anyhow::ensure!(
                    used_nonces.insert(nonce.to_bytes()),
                    "duplicate shielded action nonce"
                );
            }
        }
    }
    let mut actions = Vec::with_capacity(intent.actions.len());
    for action in intent.actions {
        let action = match action {
            ActionIntent::Complete(action) => action,
            ActionIntent::Transfer(transfer) => ActionPlan::Transfer(complete_transfer(
                transfer,
                batch_ref()?,
                timestamp,
                fresh_action_nonce(rng, &mut used_nonces)?,
                routing.clone(),
            )?),
            ActionIntent::NoteReshape(reshape) => {
                let witness = action_witness(batch_ref()?, &reshape.spends)?;
                let context = NoteReshapeContext {
                    witness,
                    nonce: fresh_action_nonce(rng, &mut used_nonces)?,
                };
                ActionPlan::NoteReshape(NoteReshapePlan::new(
                    reshape.family_id,
                    reshape.spends,
                    reshape.outputs,
                    reshape.value_blinding,
                    context,
                    routing.clone(),
                )?)
            }
            ActionIntent::Ics20Withdrawal(withdrawal) => {
                let context = WithdrawalContext {
                    witness: action_witness(batch_ref()?, &withdrawal.spends)?,
                    timestamp,
                    nonce: fresh_action_nonce(rng, &mut used_nonces)?,
                };
                ActionPlan::ShieldedIcs20Withdrawal(ShieldedIcs20WithdrawalPlan::new(
                    withdrawal.spends,
                    withdrawal.change_output,
                    withdrawal.withdrawal,
                    withdrawal.value_blinding,
                    context,
                    routing.clone(),
                )?)
            }
            ActionIntent::HostWithdrawal(withdrawal) => {
                let context = WithdrawalContext {
                    witness: action_witness(batch_ref()?, &withdrawal.spends)?,
                    timestamp,
                    nonce: fresh_action_nonce(rng, &mut used_nonces)?,
                };
                ActionPlan::ShieldedHostWithdrawal(ShieldedHostWithdrawalPlan::new(
                    withdrawal.spends,
                    withdrawal.change_output,
                    withdrawal.withdrawal,
                    withdrawal.value_blinding,
                    context,
                    routing.clone(),
                )?)
            }
        };
        actions.push(action);
    }
    let fee_funding = intent
        .fee_funding
        .map(|fee| {
            Ok::<_, anyhow::Error>(shieldd_sdk_transaction::FeeFundingPlan {
                transfer: complete_transfer(
                    fee,
                    batch_ref()?,
                    timestamp,
                    fresh_action_nonce(rng, &mut used_nonces)?,
                    routing,
                )?,
            })
        })
        .transpose()?;
    let mut plan = TransactionPlan {
        actions,
        fee_funding,
        transaction_parameters: intent.transaction_parameters,
        memo: intent.memo,
        nullifier_window: intent.nullifier_window,
    };
    plan.sort_actions();
    Ok(plan)
}

fn complete_transfer(
    intent: crate::planning_intent::TransferIntent,
    batch: &shieldd_sdk_compliance::BatchComplianceData,
    timestamp: u64,
    nonce: Fr,
    routing: shieldd_sdk_shielded_pool::discovery::Parameters,
) -> Result<shieldd_sdk_shielded_pool::TransferPlan> {
    use shieldd_sdk_shielded_pool::{TransferContext, TransferPlan};
    let witness = action_witness(batch, &intent.spends)?;
    let recipient = intent
        .outputs
        .first()
        .ok_or_else(|| anyhow::anyhow!("transfer requires a recipient"))?;
    let recipient = user_witness(batch, &recipient.dest_address, witness.asset.asset_id)?;
    let policy = if witness.asset.is_regulated {
        Some(
            batch
                .asset_policies
                .get(&witness.asset.asset_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("missing regulated asset policy"))?,
        )
    } else {
        None
    };
    TransferPlan::new(
        intent.spends,
        intent.outputs,
        intent.value_blinding,
        TransferContext {
            witness,
            recipient,
            policy,
            timestamp,
            nonce,
        },
        routing,
    )
}

fn action_witness(
    batch: &shieldd_sdk_compliance::BatchComplianceData,
    spends: &[shieldd_sdk_shielded_pool::ShieldedInputPlan],
) -> Result<shieldd_sdk_shielded_pool::ActionWitness> {
    use shieldd_sdk_shielded_pool::{ActionWitness, AssetWitness};
    let spend = spends
        .first()
        .ok_or_else(|| anyhow::anyhow!("shielded action requires a spend"))?;
    let asset_id = spend.note.asset_id();
    let asset = batch
        .asset_proofs
        .get(&asset_id)
        .ok_or_else(|| anyhow::anyhow!("missing asset proof for {asset_id}"))?;
    Ok(ActionWitness {
        asset: AssetWitness {
            asset_id,
            root: batch.asset_anchor,
            leaf: asset.indexed_leaf.clone(),
            position: asset.position,
            path: asset.auth_path.clone(),
            is_regulated: asset.is_regulated,
        },
        user_root: batch.compliance_anchor,
        sender: user_witness(batch, &spend.note.address(), asset_id)?,
    })
}

fn user_witness(
    batch: &shieldd_sdk_compliance::BatchComplianceData,
    address: &Address,
    asset_id: asset::Id,
) -> Result<shieldd_sdk_shielded_pool::UserWitness> {
    let user = batch
        .user_proofs
        .get(&(address.clone(), asset_id))
        .ok_or_else(|| {
            anyhow::anyhow!("missing user witness for {address} and asset {asset_id}")
        })?;
    Ok(shieldd_sdk_shielded_pool::UserWitness {
        leaf: user.leaf.clone(),
        position: user.position,
        path: user.auth_path.clone(),
    })
}

fn fresh_action_nonce(
    rng: &mut (impl rand_core::RngCore + rand_core::CryptoRng),
    used: &mut BTreeSet<[u8; 32]>,
) -> Result<Fr> {
    let nonce = Fr::rand(rng);
    anyhow::ensure!(
        used.insert(nonce.to_bytes()),
        "compliance RNG generated a duplicate shielded action nonce"
    );
    Ok(nonce)
}

#[cfg(test)]
mod tests {
    use super::{complete_plan_with_compliance, fresh_action_nonce, parse_proto_merkle_path};
    use crate::planning_intent::{
        ActionIntent, NoteReshapeIntent, TransactionIntent, TransferIntent,
    };
    use async_trait::async_trait;
    use decaf377::Fr;
    use rand::{rngs::StdRng, SeedableRng};
    use rand_core::{CryptoRng, Error as RandError, RngCore};
    use shieldd_sdk_asset::{asset, Value, BASE_ASSET_ID};
    use shieldd_sdk_compliance::{
        AssetPolicy, AssetProofData, ComplianceLeaf, ComplianceProofProvider, MerklePath,
        UserProofData,
    };
    use shieldd_sdk_keys::Address;
    use shieldd_sdk_proto::core::component::compliance::v1 as compliance_pb;
    use shieldd_sdk_shielded_pool::{
        Note, NoteReshapeFamilyId, ShieldedInputPlan, ShieldedOutputPlan,
    };
    use shieldd_sdk_tct::StateCommitment;
    use shieldd_sdk_transaction::plan::ActionPlan;
    use std::collections::BTreeSet;

    struct RepeatingRng;

    impl RngCore for RepeatingRng {
        fn next_u32(&mut self) -> u32 {
            0
        }

        fn next_u64(&mut self) -> u64 {
            0
        }

        fn fill_bytes(&mut self, destination: &mut [u8]) {
            destination.fill(0);
        }

        fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), RandError> {
            self.fill_bytes(destination);
            Ok(())
        }
    }

    impl CryptoRng for RepeatingRng {}

    struct UnregulatedProofProvider;

    #[async_trait]
    impl ComplianceProofProvider for UnregulatedProofProvider {
        async fn get_compliance_anchor(&self) -> anyhow::Result<StateCommitment> {
            Ok(StateCommitment(decaf377::Fq::from(0u64)))
        }

        async fn get_asset_anchor(&self) -> anyhow::Result<StateCommitment> {
            let (root, _, _, _) = shieldd_sdk_compliance::create_default_imt_proof(BASE_ASSET_ID.0);
            Ok(root)
        }

        async fn get_asset_proof(&self, asset_id: asset::Id) -> anyhow::Result<AssetProofData> {
            let (_, indexed_leaf, auth_path, position) =
                shieldd_sdk_compliance::create_default_imt_proof(asset_id.0);
            Ok(AssetProofData {
                auth_path,
                position,
                indexed_leaf,
                is_regulated: false,
            })
        }

        async fn get_asset_policy(
            &self,
            _asset_id: asset::Id,
        ) -> anyhow::Result<Option<AssetPolicy>> {
            Ok(None)
        }

        async fn get_user_proof(
            &self,
            address: &Address,
            asset_id: asset::Id,
        ) -> anyhow::Result<UserProofData> {
            Ok(UserProofData {
                auth_path: MerklePath::default(),
                position: 0,
                leaf: ComplianceLeaf::synthetic_unregulated(address.clone(), asset_id),
            })
        }
    }

    fn self_transfer_intent(rng: &mut StdRng) -> TransferIntent {
        let sender = Address::dummy(rng);
        let value = Value {
            amount: 100u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let note = Note::generate(rng, &sender, value);
        let spend = ShieldedInputPlan::new(rng, note, 0u64.into());
        let output = ShieldedOutputPlan::new(rng, value, sender);
        TransferIntent {
            spends: vec![spend],
            outputs: vec![output],
            value_blinding: Fr::rand(rng),
        }
    }

    fn self_note_reshape_intent(rng: &mut StdRng) -> NoteReshapeIntent {
        let address = Address::dummy(rng);
        let input_value = Value {
            amount: 100u64.into(),
            asset_id: *BASE_ASSET_ID,
        };
        let spends = (0..8)
            .map(|position| {
                let note = Note::generate(rng, &address, input_value);
                ShieldedInputPlan::new(rng, note, position.into())
            })
            .collect();
        let output = ShieldedOutputPlan::new(
            rng,
            Value {
                amount: 800u64.into(),
                asset_id: *BASE_ASSET_ID,
            },
            address,
        );
        NoteReshapeIntent {
            family_id: NoteReshapeFamilyId::EightByOne,
            spends,
            outputs: vec![output],
            value_blinding: Fr::rand(rng),
        }
    }

    #[test]
    fn rpc_merkle_path_parser_requires_canonical_fixed_shape() {
        parse_proto_merkle_path(Some(MerklePath::default().into()), "test_path")
            .expect("canonical fixed-width path");

        parse_proto_merkle_path(None, "test_path").expect_err("missing path must fail");

        let mut short: compliance_pb::MerklePath = MerklePath::default().into();
        short.layers.pop();
        parse_proto_merkle_path(Some(short), "test_path").expect_err("short path must fail");

        let mut noncanonical: compliance_pb::MerklePath = MerklePath::default().into();
        noncanonical.layers[0].siblings[0] = vec![0xff; 32];
        parse_proto_merkle_path(Some(noncanonical), "test_path")
            .expect_err("noncanonical field must fail");
    }

    #[test]
    fn transfer_compliance_nonce_allocator_rejects_cross_action_reuse() {
        let mut used = BTreeSet::new();
        let mut repeating = RepeatingRng;
        fresh_action_nonce(&mut repeating, &mut used).expect("first nonce is unused");
        fresh_action_nonce(&mut repeating, &mut used)
            .expect_err("a repeated action nonce must fail closed");

        let mut seeded = StdRng::seed_from_u64(0x7368_6965_6c64_645f);
        let mut used = BTreeSet::new();
        for _ in 0..8 {
            fresh_action_nonce(&mut seeded, &mut used)
                .expect("independent CSPRNG draws must produce distinct action nonces");
        }
        assert_eq!(used.len(), 8);
    }

    #[tokio::test]
    async fn completion_builds_note_reshape_from_provider_witnesses() {
        let mut rng = StdRng::seed_from_u64(13);
        let intent = TransactionIntent {
            actions: vec![ActionIntent::NoteReshape(self_note_reshape_intent(
                &mut rng,
            ))],
            transaction_parameters: Default::default(),
            fee_funding: None,
            memo: None,
            nullifier_window: None,
        };
        let plan = complete_plan_with_compliance(
            intent,
            |queries| async move { UnregulatedProofProvider.get_batch_proofs(&queries).await },
            &mut rng,
            Default::default(),
            Some(1_700_000_000),
        )
        .await
        .expect("complete NoteReshape");
        let ActionPlan::NoteReshape(plan) = &plan.actions[0] else {
            panic!("expected NoteReshape")
        };
        assert_eq!(
            plan.compliance.witness.user_root,
            StateCommitment(decaf377::Fq::from(0u64))
        );
        plan.validate().expect("complete context must be valid");
    }

    #[tokio::test]
    async fn completion_assigns_distinct_action_and_fee_nonces() {
        let mut rng = StdRng::seed_from_u64(7);
        let intent = TransactionIntent {
            actions: vec![
                ActionIntent::Transfer(self_transfer_intent(&mut rng)),
                ActionIntent::Transfer(self_transfer_intent(&mut rng)),
            ],
            transaction_parameters: Default::default(),
            fee_funding: Some(self_transfer_intent(&mut rng)),
            memo: None,
            nullifier_window: None,
        };
        let plan = complete_plan_with_compliance(
            intent,
            |queries| async move { UnregulatedProofProvider.get_batch_proofs(&queries).await },
            &mut rng,
            Default::default(),
            Some(1_700_000_000),
        )
        .await
        .expect("complete transfers");
        let mut nonces = BTreeSet::new();
        for action in &plan.actions {
            let ActionPlan::Transfer(transfer) = action else {
                panic!("expected Transfer")
            };
            assert!(nonces.insert(transfer.compliance.nonce.to_bytes()));
            assert_eq!(transfer.compliance.timestamp, 1_700_000_000);
            transfer.validate().expect("complete transfer");
        }
        assert!(nonces.insert(
            plan.fee_funding
                .unwrap()
                .transfer
                .compliance
                .nonce
                .to_bytes()
        ));
        assert_eq!(nonces.len(), 3);
    }
}
