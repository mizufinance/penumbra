use super::{Storage, TreeStore};
use anyhow::{Context, Result};
use shieldd_sdk_proto::{DomainType, Message};
use shieldd_sdk_tct::Proof;
use shieldd_sdk_transaction::{ActionPlan, TransactionPlan, WitnessData};

impl Storage {
    pub async fn witness_plan(&self, plan: &TransactionPlan) -> Result<WitnessData> {
        let pool = self.pool.clone();
        let plan = plan.clone();
        tokio::task::spawn_blocking(move || {
            let mut connection = pool.get()?;
            let mut transaction = connection.transaction()?;
            let sct = shieldd_sdk_tct::Tree::from_reader(&mut TreeStore(&mut transaction))?;
            let mut historical_nullifier_proofs = Vec::new();
            if plan.spends().any(|p| p.spend.note.amount() != 0u64.into()) {
                let window = plan
                    .nullifier_window
                    .context("spend-bearing plan is missing nullifier window")?;
                let window_bytes: Vec<u8> = transaction.query_row(
                    "SELECT v FROM kv WHERE k = 'nullifier_window'",
                    [],
                    |r| r.get(0),
                )?;
                let current_window: shieldd_sdk_sct::nullifier_generation::NullifierWindow =
                    shieldd_sdk_proto::core::component::sct::v1::NullifierWindow::decode(
                        window_bytes.as_slice(),
                    )?
                    .try_into()?;
                anyhow::ensure!(
                    window == current_window,
                    "transaction plan nullifier window is stale"
                );
                let fvk_bytes: Vec<u8> =
                    transaction.query_row("SELECT v FROM kv WHERE k = 'fvk'", [], |r| r.get(0))?;
                let fvk = shieldd_sdk_keys::FullViewingKey::decode(fvk_bytes.as_slice())?;
                for planned in plan.spends().filter(|p| {
                    p.spend.note.amount() != 0u64.into()
                        && u64::from(p.spend.position) < window.recent_position_floor
                }) {
                    let key = planned.witness.nullifier_key(&fvk)?;
                    let nullifier = planned.spend.nullifier(&key);
                    let mut statement = transaction.prepare_cached(
                        "SELECT nullifier, protocol_version, proof_bundle, cache_state, last_error
                     FROM historical_proof_cache WHERE nullifier = ?1",
                    )?;
                    let mut rows = statement.query([nullifier.to_bytes().to_vec()])?;
                    let row = rows.next()?.with_context(|| {
                        format!("historical proof cache is missing for {nullifier}")
                    })?;
                    let cache = Storage::decode_historical_cache(row)?;
                    historical_nullifier_proofs.push(cache.bundle_for(window)?);
                }
            }
            let mut commitments = plan
                .spends()
                .filter(|p| p.spend.note.amount() != 0u64.into())
                .map(|p| p.spend.note.commit())
                .collect::<Vec<_>>();
            commitments.extend(plan.actions.iter().filter_map(|action| match action {
                ActionPlan::Transfer(plan) => plan.accumulator_prior_commitment(),
                ActionPlan::ShieldedHostWithdrawal(plan) => plan.accumulator_prior_commitment(),

                _ => None,
            }));
            let proofs = commitments
                .into_iter()
                .map(|commitment| sct.witness(commitment).context("note commitment missing"))
                .collect::<Result<Vec<_>>>()?;
            let mut witness = WitnessData {
                anchor: sct.root(),
                state_commitment_proofs: proofs.into_iter().map(|p| (p.commitment(), p)).collect(),
                historical_nullifier_proofs,
            };
            for commitment in plan
                .spends()
                .filter(|p| p.spend.note.amount() == 0u64.into())
                .map(|p| p.spend.note.commit())
            {
                witness.add_proof(commitment, Proof::dummy(&mut rand_core::OsRng, commitment));
            }
            Ok(witness)
        })
        .await?
    }
}
