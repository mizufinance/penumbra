use anyhow::Error;
use cnidarium::StateRead;
use penumbra_sdk_compact_block::{component::StateReadExt as _, CompactBlock, StatePayload};
use penumbra_sdk_compliance::{ComplianceLeaf, ComplianceRegistryRead, MerklePath};
use penumbra_sdk_keys::{keys::SpendKey, FullViewingKey};
use penumbra_sdk_sct::{
    component::{clock::EpochRead, tree::SctRead},
    Nullifier,
};
use penumbra_sdk_shielded_pool::{note, Note};
use penumbra_sdk_tct as tct;
use penumbra_sdk_transaction::{
    memo::MemoPlaintext,
    plan::{ActionPlan, MemoPlan},
    AuthorizationData, Transaction, TransactionPlan, WitnessData,
};
use penumbra_sdk_view::enrich_plan_with_compliance;
use rand_core::OsRng;
use std::collections::BTreeMap;
use tracing;

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
    pub sct: penumbra_sdk_tct::Tree,
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

    pub async fn sync_to_latest<R: StateRead>(&mut self, state: R) -> anyhow::Result<()> {
        let height = state.get_block_height().await?;
        self.sync_to(height, state).await?;
        Ok(())
    }

    pub async fn sync_to<R: StateRead>(
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
            self.scan_block(compact_block.try_into()?)?;
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

    pub fn scan_block(&mut self, block: CompactBlock) -> anyhow::Result<()> {
        use penumbra_sdk_tct::Witness::*;

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
                            let nullifier = self
                                .nullifier(payload.note_commitment)
                                .expect("newly inserted note should be present in sct");
                            self.notes.insert(payload.note_commitment, note.clone());
                            self.nullifiers.insert(payload.note_commitment, nullifier);
                        }
                        None => {
                            self.sct.insert(Forget, payload.note_commitment)?;
                        }
                    }
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

    pub fn latest_height_and_sct_root(&self) -> (u64, penumbra_sdk_tct::Root) {
        (self.latest_height, self.sct.root())
    }

    pub fn note_by_commitment(&self, commitment: &note::StateCommitment) -> Option<Note> {
        self.notes.get(commitment).cloned()
    }

    pub fn position(
        &self,
        commitment: note::StateCommitment,
    ) -> Option<penumbra_sdk_tct::Position> {
        self.sct.witness(commitment).map(|proof| proof.position())
    }

    pub fn nullifier(&self, commitment: note::StateCommitment) -> Option<Nullifier> {
        let position = self.position(commitment);

        if position.is_none() {
            return None;
        }
        let nk = self.fvk.nullifier_key();

        Some(Nullifier::derive(&nk, position.unwrap(), &commitment))
    }

    pub fn witness_commitment(
        &self,
        commitment: note::StateCommitment,
    ) -> Option<penumbra_sdk_tct::Proof> {
        self.sct.witness(commitment)
    }

    pub fn witness_plan(&self, plan: &TransactionPlan) -> Result<WitnessData, Error> {
        let commitments = plan.actions.iter().flat_map(|action| match action {
            ActionPlan::Transfer(plan) => plan
                .spends
                .iter()
                .map(|spend| spend.note.commit())
                .collect::<Vec<_>>(),
            ActionPlan::Consolidate(plan) => plan
                .spends
                .iter()
                .map(|spend| spend.note.commit())
                .collect::<Vec<_>>(),
            ActionPlan::Split(plan) => plan
                .spends
                .iter()
                .map(|spend| spend.note.commit())
                .collect::<Vec<_>>(),
            ActionPlan::ShieldedIcs20Withdrawal(plan) => plan
                .spends
                .iter()
                .map(|spend| spend.note.commit())
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        });

        let witness = |commitment| {
            self.sct
                .witness(commitment)
                .ok_or_else(|| anyhow::anyhow!("note commitment {commitment:?} unknown to client"))
                .map(|proof| (commitment, proof))
        };

        Ok(WitnessData {
            anchor: self.sct.root(),
            state_commitment_proofs: commitments.map(witness).collect::<Result<_, Error>>()?,
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

    /// Build a transaction with compliance enrichment from state.
    ///
    /// This method enriches the plan with compliance data (anchors, Merkle paths, etc.)
    /// from the provided state before building the transaction.
    pub async fn witness_auth_build_with_compliance<S: StateRead + Send + Sync>(
        &self,
        plan: &mut TransactionPlan,
        state: S,
    ) -> Result<Transaction, Error> {
        // Read block timestamp from state before enrichment.
        // Tests use fake chain times (e.g. 2022), but SystemTime::now() returns
        // real time. Pass the block timestamp so DLEQ proofs and on-chain freshness
        // checks are consistent.
        let block_ts = state
            .get_current_block_timestamp()
            .await
            .ok()
            .map(|t| t.unix_timestamp() as u64);

        // Enrich the plan with compliance data
        self.enrich_plan_with_compliance_internal(plan, state, block_ts)
            .await?;
        // Populate FMD clues if not already set (stateless checks require
        // num_clues == num_note_creating_outputs).
        if plan.detection_data.is_none() {
            plan.populate_detection_data(&mut OsRng, Default::default());
        }
        // Populate memo if outputs exist but no memo set.
        if plan.memo.is_none() && plan.num_outputs() > 0 {
            let (return_address, _) = self.fvk.incoming().payment_address(0u32.into());
            plan.memo = Some(MemoPlan::new(
                &mut OsRng,
                MemoPlaintext::new(return_address, String::new())?,
            ));
        }
        // Then build normally
        let witness_data = self.witness_plan(plan)?;
        let auth_data = self.authorize_plan(plan)?;
        plan.clone()
            .build_concurrent(&self.fvk, &witness_data, &auth_data)
            .await
    }

    /// Enrich a transaction plan with compliance data from state.
    ///
    /// Uses the shared enrichment function from the view crate with StateReadComplianceProvider.
    async fn enrich_plan_with_compliance_internal<S: StateRead + Send + Sync>(
        &self,
        plan: &mut TransactionPlan,
        state: S,
        target_timestamp: Option<u64>,
    ) -> Result<(), Error> {
        let provider = StateReadComplianceProvider::new(state);
        enrich_plan_with_compliance(plan, &provider, &mut OsRng, target_timestamp).await
    }

    pub fn notes_by_asset(
        &self,
        asset_id: penumbra_sdk_asset::asset::Id,
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
        asset_id: penumbra_sdk_asset::asset::Id,
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
impl<S: StateRead + Send + Sync> penumbra_sdk_compliance::ComplianceProofProvider
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
        asset_id: penumbra_sdk_asset::asset::Id,
    ) -> anyhow::Result<penumbra_sdk_compliance::AssetProofData> {
        // Use the IMT-based get_asset_proof_data for proper indexed leaf
        let proof_data = self.state.get_asset_proof_data(asset_id).await?;

        let path = MerklePath {
            layers: proof_data
                .auth_path
                .layers
                .into_iter()
                .map(|layer| penumbra_sdk_compliance::MerklePathLayer {
                    siblings: layer.siblings,
                })
                .collect(),
        };
        Ok(penumbra_sdk_compliance::AssetProofData {
            auth_path: path,
            position: proof_data.position,
            indexed_leaf: proof_data.indexed_leaf,
            is_regulated: proof_data.is_regulated,
        })
    }

    async fn get_asset_policy(
        &self,
        asset_id: penumbra_sdk_asset::asset::Id,
    ) -> anyhow::Result<Option<penumbra_sdk_compliance::AssetPolicy>> {
        self.state.get_asset_policy(asset_id).await
    }

    async fn get_user_proof(
        &self,
        address: &penumbra_sdk_keys::Address,
        asset_id: penumbra_sdk_asset::asset::Id,
    ) -> anyhow::Result<penumbra_sdk_compliance::UserProofData> {
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
                    .map(|siblings| penumbra_sdk_compliance::MerklePathLayer {
                        siblings: siblings.iter().map(|s| s.0.to_bytes().to_vec()).collect(),
                    })
                    .collect(),
            };

            return Ok(penumbra_sdk_compliance::UserProofData {
                auth_path: path,
                position,
                leaf,
            });
        }

        // Unregulated assets can still build without a registered user leaf.
        let asset_proof = self.get_asset_proof(asset_id).await?;
        if !asset_proof.is_regulated {
            let b_d_fq = address.diversified_generator().vartime_compress_to_field();
            let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
            let synthetic_leaf = ComplianceLeaf {
                address: address.clone(),
                asset_id,
                d,
            };
            return Ok(penumbra_sdk_compliance::UserProofData {
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
        queries: &[(penumbra_sdk_keys::Address, penumbra_sdk_asset::asset::Id)],
    ) -> anyhow::Result<penumbra_sdk_compliance::BatchComplianceData> {
        use penumbra_sdk_compliance::{
            AssetProofData, BatchComplianceData, IndexedMerkleTree, UserProofData,
        };
        use std::collections::BTreeMap;

        // Read trees ONCE to ensure consistency between anchors and proofs
        let asset_tree = self.state.get_asset_imt().await?;
        let user_tree = self.state.get_user_tree().await?;

        // Get anchors from the same tree instances used for proofs
        let asset_anchor = tct::StateCommitment(asset_tree.root().0);
        let compliance_anchor = tct::StateCommitment(user_tree.root().0);

        let mut asset_proofs = BTreeMap::new();
        let mut asset_policies = BTreeMap::new();
        let mut user_proofs = BTreeMap::new();

        for (address, asset_id) in queries {
            // Generate asset proof from the SAME tree we got the anchor from
            if !asset_proofs.contains_key(asset_id) {
                let value = asset_id.0;
                let is_regulated = self.state.is_asset_regulated(*asset_id).await?;

                let (path, position, indexed_leaf) = if asset_tree.contains(value) {
                    // Explicitly present asset - use membership proof regardless of regulation.
                    let (pos, leaf, auth_path) = asset_tree.membership_proof(value)?;
                    (MerklePath::from_auth_path(auth_path), pos, leaf)
                } else {
                    // Asset absent from the IMT - use non-membership proof.
                    let (pos, leaf, auth_path) = asset_tree.non_membership_proof(value)?;

                    // DEBUG: Verify the proof before returning
                    let verified = IndexedMerkleTree::verify_auth_path(
                        pos,
                        &leaf,
                        &auth_path,
                        asset_tree.root(),
                        asset_tree.depth(),
                    );
                    tracing::debug!(
                        ?asset_id,
                        pos,
                        ?leaf.value,
                        ?leaf.next_value,
                        path_len = auth_path.len(),
                        verified,
                        "non-membership proof"
                    );
                    if !verified {
                        tracing::error!("IMT non-membership proof verification FAILED");
                    }

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
                            .map(|siblings| penumbra_sdk_compliance::MerklePathLayer {
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
                    let b_d_fq = address.diversified_generator().vartime_compress_to_field();
                    let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
                    let synthetic_leaf = ComplianceLeaf {
                        address: address.clone(),
                        asset_id: *asset_id,
                        d,
                    };
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
    use penumbra_sdk_asset::{asset, Value};
    use penumbra_sdk_keys::keys::{Bip44Path, SeedPhrase, SpendKey};
    use penumbra_sdk_shielded_pool::{
        Note, Rseed, ShieldedInputPlan, ShieldedOutputPlan, TransferPlan,
    };
    use penumbra_sdk_tct::Witness;
    use penumbra_sdk_transaction::{ActionPlan, TransactionPlan};
    use rand_core::OsRng;

    #[test]
    fn witness_plan_includes_hidden_arity_transfer_spend_proof() {
        let sk =
            SpendKey::from_seed_phrase_bip44(SeedPhrase::generate(&mut OsRng), &Bip44Path::new(0));
        let mut client = MockClient::new(sk);
        let fvk = client.fvk.clone();
        let (address, _) = fvk.incoming().payment_address(0u32.into());

        let note = Note::from_parts(
            address.clone(),
            Value {
                amount: 100u64.into(),
                asset_id: asset::Id(Fq::from(1u64)),
            },
            Rseed::generate(&mut OsRng),
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
        let transfer = TransferPlan::from_spend_output(spend.into(), output.into(), Fr::from(9u64))
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
}
