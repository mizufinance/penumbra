use anyhow::Error;
use cnidarium::StateRead;
use rand_core::OsRng;
use shieldd_sdk_compact_block::{component::StateReadExt as _, CompactBlock, StatePayload};
use shieldd_sdk_compliance::{
    effective_nullifier_key, ComplianceLeaf, ComplianceRegistryRead, MerklePath,
};
use shieldd_sdk_keys::{keys::SpendKey, FullViewingKey};
use shieldd_sdk_sct::{
    component::{clock::EpochRead, tree::SctRead},
    Nullifier,
};
use shieldd_sdk_shielded_pool::{component::StateReadExt as _, note, Note};
use shieldd_sdk_tct as tct;
use shieldd_sdk_transaction::{
    memo::MemoPlaintext, plan::MemoPlan, AuthorizationData, Transaction, TransactionPlan,
    WitnessData,
};
use shieldd_sdk_view::complete_plan_with_compliance;
pub use shieldd_sdk_view::planning_intent::{
    ActionIntent, NoteReshapeIntent, TransactionIntent, TransferIntent, WithdrawalIntent,
};
use std::collections::BTreeMap;

/// A bare-bones mock client for use exercising the state machine.
pub struct MockClient {
    latest_height: u64,
    sk: SpendKey,
    pub fvk: FullViewingKey,
    /// All notes, whether spent or not.
    pub notes: BTreeMap<note::StateCommitment, Note>,
    pub nullifiers: BTreeMap<note::StateCommitment, Nullifier>,
    /// Whether a note was spent or not.
    pub spent_notes: BTreeMap<note::StateCommitment, ()>,
    pub sct: shieldd_sdk_tct::Tree,
}

impl MockClient {
    pub fn new(sk: SpendKey) -> MockClient {
        Self {
            latest_height: u64::MAX,
            fvk: sk.full_viewing_key().clone(),
            sk,
            notes: Default::default(),
            spent_notes: Default::default(),
            nullifiers: Default::default(),
            sct: Default::default(),
        }
    }

    pub async fn with_sync_to_storage(
        mut self,
        storage: impl AsRef<cnidarium::Storage>,
    ) -> anyhow::Result<Self> {
        let latest = storage.as_ref().latest_snapshot();
        self.sync_to_latest(latest).await?;

        Ok(self)
    }

    pub async fn with_sync_to_inner_storage(
        mut self,
        storage: cnidarium::Storage,
    ) -> anyhow::Result<Self> {
        let latest = storage.latest_snapshot();
        self.sync_to_latest(latest).await?;

        Ok(self)
    }

    pub async fn sync_to_latest<R: StateRead + Send + Sync>(
        &mut self,
        state: R,
    ) -> anyhow::Result<()> {
        let height = state.get_block_height().await?;
        self.sync_to(height, state).await?;
        Ok(())
    }

    pub async fn sync_to<R: StateRead + Send + Sync>(
        &mut self,
        target_height: u64,
        state: R,
    ) -> anyhow::Result<()> {
        let start_height = self.latest_height.wrapping_add(1);
        for height in start_height..=target_height {
            let compact_block = state
                .compact_block(height)
                .await?
                .ok_or_else(|| anyhow::anyhow!("missing compact block for height {}", height))?;
            self.scan_block(compact_block.try_into()?, &state).await?;
            let (latest_height, root) = self.latest_height_and_sct_root();
            anyhow::ensure!(latest_height == height, "latest height should be updated");
            let expected_root = state
                .get_anchor_by_height(height)
                .await?
                .ok_or_else(|| anyhow::anyhow!("missing sct anchor for height {}", height))?;
            anyhow::ensure!(
                root == expected_root,
                format!(
                    "client sct root should match chain state: {:?} != {:?}",
                    root, expected_root
                )
            );
        }
        Ok(())
    }

    pub async fn scan_block<R: StateRead + Send + Sync>(
        &mut self,
        block: CompactBlock,
        state: &R,
    ) -> anyhow::Result<()> {
        use shieldd_sdk_tct::Witness::*;

        if self.latest_height.wrapping_add(1) != block.height {
            anyhow::bail!(
                "wrong block height {} for latest height {}",
                block.height,
                self.latest_height
            );
        }

        for payload in block.state_payloads {
            match payload {
                StatePayload::Note { note: payload, .. } => {
                    match payload.trial_decrypt(&self.fvk) {
                        Some(note) => {
                            self.sct.insert(Keep, payload.note_commitment)?;
                            let position = self
                                .position(payload.note_commitment)
                                .expect("newly inserted note should be present in sct");
                            let nk = match state.get_asset_policy(note.asset_id()).await? {
                                Some(policy) => {
                                    let leaf = state
                                        .get_user_leaf(&note.address(), note.asset_id())
                                        .await?
                                        .ok_or_else(|| {
                                            anyhow::anyhow!(
                                                "regulated note is missing its compliance leaf"
                                            )
                                        })?;
                                    effective_nullifier_key(
                                        *self.fvk.nullifier_key(),
                                        self.fvk.incoming(),
                                        &note.address(),
                                        note.asset_id(),
                                        policy.ring.ring_pk,
                                        leaf.rnk_dh_pk,
                                        true,
                                    )?
                                }
                                None => *self.fvk.nullifier_key(),
                            };
                            let nullifier =
                                Nullifier::derive(&nk, position, &payload.note_commitment);
                            self.notes.insert(payload.note_commitment, note.clone());
                            self.nullifiers.insert(payload.note_commitment, nullifier);
                        }
                        None => {
                            self.sct.insert(Forget, payload.note_commitment)?;
                        }
                    }
                }
                StatePayload::VolumeAccumulator { payload, .. } => {
                    let witness = match payload.trial_decrypt(self.fvk.outgoing()) {
                        Some((_, true)) => Keep,
                        _ => Forget,
                    };
                    self.sct.insert(witness, payload.commitment)?;
                }
                StatePayload::RolledUp { commitment, .. } => {
                    if self.notes.contains_key(&commitment) {
                        // This is a note we anticipated, so retain its auth path.
                        self.sct.insert(Keep, commitment)?;
                    } else {
                        // This is someone else's note.
                        self.sct.insert(Forget, commitment)?;
                    }
                }
            }
        }

        // Mark spent nullifiers
        for nullifier in block.nullifiers {
            // skip if we don't know about this nullifier
            if !self.nullifiers.values().any(move |n| *n == nullifier) {
                continue;
            }

            self.spent_notes.insert(
                *self
                    .nullifiers
                    .iter()
                    .find_map(|(k, v)| if *v == nullifier { Some(k) } else { None })
                    .unwrap(),
                (),
            );
        }

        self.sct.end_block()?;
        if block.epoch_root.is_some() {
            self.sct.end_epoch()?;
        }

        self.latest_height = block.height;

        Ok(())
    }

    pub fn latest_height_and_sct_root(&self) -> (u64, shieldd_sdk_tct::Root) {
        (self.latest_height, self.sct.root())
    }

    pub fn note_by_commitment(&self, commitment: &note::StateCommitment) -> Option<Note> {
        self.notes.get(commitment).cloned()
    }

    pub fn position(&self, commitment: note::StateCommitment) -> Option<shieldd_sdk_tct::Position> {
        self.sct.witness(commitment).map(|proof| proof.position())
    }

    pub fn witness_commitment(
        &self,
        commitment: note::StateCommitment,
    ) -> Option<shieldd_sdk_tct::Proof> {
        self.sct.witness(commitment)
    }

    pub fn witness_plan(&self, plan: &TransactionPlan) -> Result<WitnessData, Error> {
        let action_spends = plan.actions.iter().flat_map(|action| action.spends());
        let fee_funding_spends = plan
            .fee_funding
            .iter()
            .flat_map(|fee_funding| fee_funding.transfer.spends.iter());
        let commitments = action_spends
            .chain(fee_funding_spends)
            .map(|spend| spend.note.commit());

        let witness = |commitment| {
            self.sct
                .witness(commitment)
                .ok_or_else(|| anyhow::anyhow!("note commitment {commitment:?} unknown to client"))
                .map(|proof| (commitment, proof))
        };

        Ok(WitnessData {
            anchor: self.sct.root(),
            state_commitment_proofs: commitments.map(witness).collect::<Result<_, Error>>()?,
            historical_nullifier_proofs: Default::default(),
        })
    }

    pub fn authorize_plan(&self, plan: &TransactionPlan) -> Result<AuthorizationData, Error> {
        plan.authorize(OsRng, &self.sk)
    }

    pub async fn witness_auth_build(&self, plan: &TransactionPlan) -> Result<Transaction, Error> {
        let witness_data = self.witness_plan(plan)?;
        let auth_data = self.authorize_plan(plan)?;
        plan.clone()
            .build_concurrent(&self.fvk, &witness_data, &auth_data)
            .await
    }

    /// Complete a fixture intent using the current chain witnesses and parameters.
    pub async fn complete_intent<S: StateRead + Send + Sync>(
        &self,
        mut intent: TransactionIntent,
        state: S,
    ) -> Result<TransactionPlan, Error> {
        if intent
            .actions
            .iter()
            .any(|action| !action.spends().is_empty())
            || intent.fee_funding.is_some()
        {
            intent.nullifier_window = Some(
                shieldd_sdk_sct::nullifier_tree::generation_state(&state)
                    .await?
                    .window(),
            );
        }
        let timestamp = state
            .get_current_block_timestamp()
            .await?
            .unix_timestamp()
            .try_into()?;
        let routing = state.get_current_discovery_parameters().await?;
        if intent.memo.is_none() && intent.has_outputs() {
            intent.memo = Some(MemoPlan::new(
                &mut OsRng,
                MemoPlaintext::new(
                    self.fvk.incoming().payment_address(0u32.into()),
                    String::new(),
                )?,
            ));
        }
        let provider = StateReadComplianceProvider::new(state);
        complete_plan_with_compliance(
            intent,
            |queries| async move {
                shieldd_sdk_compliance::ComplianceProofProvider::get_batch_proofs(
                    &provider, &queries,
                )
                .await
                .map(|compliance| shieldd_sdk_view::CompletionData {
                    compliance,
                    volumes: vec![],
                })
            },
            &mut OsRng,
            routing,
            Some(timestamp),
            true,
        )
        .await
    }

    pub fn notes_by_asset(
        &self,
        asset_id: shieldd_sdk_asset::asset::Id,
    ) -> impl Iterator<Item = &Note> + '_ {
        self.notes
            .values()
            .filter(move |n| n.asset_id() == asset_id)
    }

    pub fn spent_note(&self, commitment: &note::StateCommitment) -> bool {
        self.spent_notes.contains_key(commitment)
    }

    pub fn spendable_notes_by_asset(
        &self,
        asset_id: shieldd_sdk_asset::asset::Id,
    ) -> impl Iterator<Item = &Note> + '_ {
        self.notes
            .values()
            .filter(move |n| n.asset_id() == asset_id && !self.spent_note(&n.commit()))
    }
}

/// A compliance proof provider backed by StateRead.
/// Used by mock-client for test transaction enrichment.
pub struct StateReadComplianceProvider<S> {
    state: S,
}

impl<S> StateReadComplianceProvider<S> {
    pub fn new(state: S) -> Self {
        Self { state }
    }
}

#[async_trait::async_trait]
impl<S: StateRead + Send + Sync> shieldd_sdk_compliance::ComplianceProofProvider
    for StateReadComplianceProvider<S>
{
    async fn get_compliance_anchor(&self) -> anyhow::Result<tct::StateCommitment> {
        let root = self.state.get_user_tree_root().await?;
        Ok(tct::StateCommitment(root.0))
    }

    async fn get_asset_anchor(&self) -> anyhow::Result<tct::StateCommitment> {
        let root = self.state.get_asset_imt_root().await?;
        Ok(tct::StateCommitment(root.0))
    }

    async fn get_asset_proof(
        &self,
        asset_id: shieldd_sdk_asset::asset::Id,
    ) -> anyhow::Result<shieldd_sdk_compliance::AssetProofData> {
        // Use the IMT-based get_asset_proof_data for proper indexed leaf
        let proof_data = self.state.get_asset_proof_data(asset_id).await?;

        let path = MerklePath {
            layers: proof_data
                .auth_path
                .layers
                .into_iter()
                .map(|layer| shieldd_sdk_compliance::MerklePathLayer {
                    siblings: layer.siblings,
                })
                .collect(),
        };
        Ok(shieldd_sdk_compliance::AssetProofData {
            auth_path: path,
            position: proof_data.position,
            indexed_leaf: proof_data.indexed_leaf,
            is_regulated: proof_data.is_regulated,
        })
    }

    async fn get_asset_policy(
        &self,
        asset_id: shieldd_sdk_asset::asset::Id,
    ) -> anyhow::Result<Option<shieldd_sdk_compliance::AssetPolicy>> {
        self.state.get_asset_policy(asset_id).await
    }

    async fn get_user_proof(
        &self,
        address: &shieldd_sdk_keys::Address,
        asset_id: shieldd_sdk_asset::asset::Id,
    ) -> anyhow::Result<shieldd_sdk_compliance::UserProofData> {
        if let Some(position) = self.state.get_user_leaf_position(address, asset_id).await? {
            let path_layers = self.state.get_user_auth_path(position).await?;
            let leaf = self
                .state
                .get_user_leaf(address, asset_id)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "user leaf not found for address {:?} and asset {:?}",
                        address,
                        asset_id
                    )
                })?;

            let path = MerklePath {
                layers: path_layers
                    .into_iter()
                    .map(|siblings| shieldd_sdk_compliance::MerklePathLayer {
                        siblings: siblings.iter().map(|s| s.0.to_bytes().to_vec()).collect(),
                    })
                    .collect(),
            };

            return Ok(shieldd_sdk_compliance::UserProofData {
                auth_path: path,
                position,
                leaf,
            });
        }

        // Unregulated assets can still build without a registered user leaf.
        let asset_proof = self.get_asset_proof(asset_id).await?;
        if !asset_proof.is_regulated {
            let synthetic_leaf = ComplianceLeaf::synthetic_unregulated(address.clone(), asset_id);
            return Ok(shieldd_sdk_compliance::UserProofData {
                auth_path: MerklePath::default(),
                position: 0,
                leaf: synthetic_leaf,
            });
        }

        Err(anyhow::anyhow!(
            "user not registered in compliance tree for address {:?} and asset {:?}",
            address,
            asset_id
        ))
    }

    /// Override get_batch_proofs to ensure anchor/proof consistency.
    ///
    /// CRITICAL: We read each tree ONCE and use the same instance for both
    /// the anchor and the proofs. This prevents the bug where anchor and proofs
    /// come from different tree deserializations (which could differ due to
    /// serialization issues or timing).
    async fn get_batch_proofs(
        &self,
        queries: &[shieldd_sdk_compliance::ComplianceQuery],
    ) -> anyhow::Result<shieldd_sdk_compliance::BatchComplianceData> {
        use shieldd_sdk_compliance::{AssetProofData, BatchComplianceData, UserProofData};
        use std::collections::BTreeMap;

        // Read trees ONCE to ensure consistency between anchors and proofs
        let asset_tree = self.state.reconstruct_asset_tree().await?;
        let user_tree = self.state.reconstruct_user_tree().await?;

        // Get anchors from the same tree instances used for proofs
        let asset_anchor = tct::StateCommitment(asset_tree.root().0);
        let compliance_anchor = tct::StateCommitment(user_tree.root().0);

        let mut asset_proofs = BTreeMap::new();
        let mut asset_policies = BTreeMap::new();
        let mut user_proofs = BTreeMap::new();

        for shieldd_sdk_compliance::ComplianceQuery { address, asset_id } in queries {
            // Generate asset proof with the same membership/non-membership
            // semantics enforced by the circuit.
            if !asset_proofs.contains_key(asset_id) {
                let value = asset_id.0;
                let is_regulated = self.state.is_asset_regulated(*asset_id).await?;

                let (path, position, indexed_leaf) = if is_regulated {
                    let (pos, leaf, auth_path) = asset_tree.membership_proof(value)?;
                    (MerklePath::from_auth_path(auth_path), pos, leaf)
                } else {
                    let (pos, leaf, auth_path) = asset_tree.non_membership_proof(value)?;
                    (MerklePath::from_auth_path(auth_path), pos, leaf)
                };

                if is_regulated {
                    let policy =
                        self.state
                            .get_asset_policy(*asset_id)
                            .await?
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "missing asset policy for regulated asset {}",
                                    asset_id
                                )
                            })?;
                    asset_policies.insert(*asset_id, policy);
                }

                asset_proofs.insert(
                    *asset_id,
                    AssetProofData {
                        auth_path: path,
                        position,
                        indexed_leaf,
                        is_regulated,
                    },
                );
            }

            // Generate user proof
            let key = (address.clone(), *asset_id);
            if !user_proofs.contains_key(&key) {
                let is_regulated = asset_proofs.get(asset_id).unwrap().is_regulated;

                let user_proof = if let Some(position) = self
                    .state
                    .get_user_leaf_position(address, *asset_id)
                    .await?
                {
                    let auth_path = user_tree.auth_path(position)?;
                    let leaf = self
                        .state
                        .get_user_leaf(address, *asset_id)
                        .await?
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "user leaf not found for address {:?} and asset {:?}",
                                address,
                                asset_id
                            )
                        })?;

                    let path = MerklePath {
                        layers: auth_path
                            .into_iter()
                            .map(|siblings| shieldd_sdk_compliance::MerklePathLayer {
                                siblings: siblings
                                    .iter()
                                    .map(|s| s.0.to_bytes().to_vec())
                                    .collect(),
                            })
                            .collect(),
                    };

                    UserProofData {
                        auth_path: path,
                        position,
                        leaf,
                    }
                } else if !is_regulated {
                    // Unregulated fallback: synthetic leaf with real d so leaf commitment
                    // matches what generate_compliance_details creates.
                    let synthetic_leaf =
                        ComplianceLeaf::synthetic_unregulated(address.clone(), *asset_id);
                    UserProofData {
                        auth_path: MerklePath::default(),
                        position: 0,
                        leaf: synthetic_leaf,
                    }
                } else {
                    return Err(anyhow::anyhow!(
                        "user not registered for address {:?} and asset {:?}",
                        address,
                        asset_id
                    ));
                };

                user_proofs.insert(key, user_proof);
            }
        }

        Ok(BatchComplianceData {
            compliance_anchor,
            asset_anchor,
            asset_proofs,
            asset_policies,
            user_proofs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::MockClient;
    use decaf377::{Fq, Fr};
    use rand_core::OsRng;
    use shieldd_sdk_asset::{asset, Value};
    use shieldd_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
    use shieldd_sdk_shielded_pool::{
        Note, RecoveryCommitment, Rseed, ShieldedInputPlan, ShieldedOutputPlan,
    };
    use shieldd_sdk_tct::Witness;
    use shieldd_sdk_transaction::{ActionPlan, FeeFundingPlan, TransactionPlan};

    #[test]
    fn witness_plan_includes_hidden_arity_transfer_spend_proof() {
        let sk =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(&mut OsRng), &Bip44Path::new(0))
                .expect("test spend key should satisfy key refinements");
        let mut client = MockClient::new(sk);
        let fvk = client.fvk.clone();
        let address = fvk.incoming().payment_address(0u32.into());

        let note = Note::from_parts(
            address.clone(),
            Value {
                amount: 100u64.into(),
                asset_id: asset::Id(Fq::from(1u64)),
            },
            Rseed::generate(&mut OsRng),
            RecoveryCommitment::unavailable(),
        )
        .expect("build note");
        let commitment = note.commit();
        client
            .sct
            .insert(Witness::Keep, commitment)
            .expect("insert note commitment");

        let spend = ShieldedInputPlan::new(&mut OsRng, note.clone(), 0u64.into());
        let output = ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: 60u64.into(),
                asset_id: asset::Id(note.asset_id().0),
            },
            address,
        );

        let transfer = shieldd_sdk_shielded_pool::test_plan_helpers::transfer(
            vec![spend.into()],
            vec![output.into()],
            Fr::from(9u64),
        )
        .expect("build transfer");
        let plan = TransactionPlan {
            actions: vec![ActionPlan::Transfer(transfer)],
            ..Default::default()
        };

        let witness_data = client
            .witness_plan(&plan)
            .expect("witness transfer hidden-arity plan");
        assert!(
            witness_data
                .state_commitment_proofs
                .contains_key(&commitment),
            "hidden-arity transfer spent note commitment should be witnessed",
        );
    }

    #[test]
    fn witness_plan_includes_fee_funding_transfer_spend_proof() {
        let sk =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(&mut OsRng), &Bip44Path::new(0))
                .expect("test spend key should satisfy key refinements");
        let mut client = MockClient::new(sk);
        let fvk = client.fvk.clone();
        let address = fvk.incoming().payment_address(0u32.into());

        let note = Note::from_parts(
            address.clone(),
            Value {
                amount: 100u64.into(),
                asset_id: asset::Id(Fq::from(1u64)),
            },
            Rseed::generate(&mut OsRng),
            RecoveryCommitment::unavailable(),
        )
        .expect("build note");
        let commitment = note.commit();
        client
            .sct
            .insert(Witness::Keep, commitment)
            .expect("insert note commitment");

        let spend = ShieldedInputPlan::new(&mut OsRng, note.clone(), 0u64.into());
        let output = ShieldedOutputPlan::new(
            &mut OsRng,
            Value {
                amount: 100u64.into(),
                asset_id: asset::Id(note.asset_id().0),
            },
            address,
        );

        let transfer = shieldd_sdk_shielded_pool::test_plan_helpers::fee_funding(
            vec![spend.into()],
            vec![output.into()],
            Fr::from(9u64),
        )
        .expect("build fee-funding transfer");
        let plan = TransactionPlan {
            fee_funding: Some(FeeFundingPlan { transfer }),
            ..Default::default()
        };

        let witness_data = client
            .witness_plan(&plan)
            .expect("witness fee-funding transfer plan");
        assert!(
            witness_data
                .state_commitment_proofs
                .contains_key(&commitment),
            "fee-funding spent note commitment should be witnessed",
        );
    }
}
