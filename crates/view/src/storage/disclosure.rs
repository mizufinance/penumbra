use super::*;
use anyhow::ensure;
use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use shieldd_sdk_disclosure::{
    self as disclosure, DisclosureRequest, DisclosureWitness, MetadataDocument, MetadataOpening,
};
use shieldd_sdk_transaction::{
    memo::MemoPlaintext, plan::MemoPlan, Action, ActionPlan, TransactionPlan,
};
use shieldd_sdk_txhash::EffectingData;

/// Private authority retained before submission; it does not reserve or spend notes.
#[derive(Clone, Serialize, Deserialize)]
pub struct RetainedAuthority {
    pub action: u32,
    pub randomizer: [u8; 32],
}

impl Storage {
    /// Build and durably retain disclosure witnesses before returning for submission.
    #[cfg(all(feature = "prover", any(unix, windows)))]
    pub async fn build_transaction(
        &self,
        plan: TransactionPlan,
        witness: &shieldd_sdk_transaction::WitnessData,
        authorization: &shieldd_sdk_transaction::AuthorizationData,
    ) -> anyhow::Result<Transaction> {
        let tx = plan
            .clone()
            .build_concurrent(&self.full_viewing_key().await?, witness, authorization)
            .await?;
        self.retain_disclosure_authority(&tx, &plan).await?;
        Ok(tx)
    }

    pub async fn prepare_disclosure_metadata(
        &self,
        document: MetadataDocument,
        return_address: Address,
    ) -> anyhow::Result<MemoPlan> {
        let mut salt = [0; 32];
        rand_core::OsRng.fill_bytes(&mut salt);
        let opening = MetadataOpening { salt, document };
        let commitment = disclosure::metadata_commitment(&opening)?;
        let memo = MemoPlan::new(
            &mut rand_core::OsRng,
            MemoPlaintext::new(return_address, commitment.clone())?,
        );
        let bytes = serde_json::to_vec(&opening)?;
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let conn = pool.get()?;
            let previous: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
            conn.pragma_update(None, "synchronous", "FULL")?;
            let written = conn.execute(
                "INSERT INTO disclosure_metadata (commitment, opening) VALUES (?1, ?2)",
                rusqlite::params![commitment, bytes],
            );
            let restored = conn.pragma_update(None, "synchronous", previous);
            written?;
            restored?;
            anyhow::Ok(())
        })
        .await??;
        Ok(memo)
    }

    /// Call with the final plan and built transaction, before submitting to a node.
    pub async fn retain_disclosure_authority(
        &self,
        tx: &Transaction,
        plan: &TransactionPlan,
    ) -> anyhow::Result<()> {
        let fvk = self.full_viewing_key().await?;
        ensure!(
            plan.effect_hash(&fvk)? == tx.effect_hash(),
            "plan does not match outgoing transaction"
        );
        let mut authorities = Vec::new();
        for (i, action) in plan.actions.iter().enumerate() {
            if let ActionPlan::Transfer(transfer) = action {
                let input = transfer
                    .spends
                    .first()
                    .context("missing mandatory real input")?;
                ensure!(input.note.amount().value() > 0, "dummy authority input");
                let Some(Action::Transfer(accepted)) = tx.actions().nth(i) else {
                    anyhow::bail!("action mismatch")
                };
                accepted.body.validate_shape()?;
                ensure!(
                    input.rk(&fvk) == accepted.body.inputs[0].rk,
                    "authority key mismatch"
                );
                authorities.push(RetainedAuthority {
                    action: u32::try_from(i)?,
                    randomizer: input.randomizer.to_bytes(),
                });
            }
        }
        let metadata = plan.memo.as_ref().map(|m| m.plaintext.text().to_owned());
        let tx_id = tx.id().to_string();
        let tx_bytes = tx.encode_to_vec();
        let authority_bytes = serde_json::to_vec(&authorities)?;
        let pool = self.pool.clone();
        spawn_blocking(move || {
            let conn = pool.get()?;
            let previous: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
            conn.pragma_update(None, "synchronous", "FULL")?;
            let written = conn.execute("INSERT INTO disclosure_outgoing (tx_id, tx_bytes, authorities, metadata_commitment) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(tx_id) DO NOTHING", rusqlite::params![tx_id, tx_bytes, authority_bytes, metadata]);
            let restored = conn.pragma_update(None, "synchronous", previous);
            written?;
            restored?;
            anyhow::Ok(())
        }).await?
    }

    pub async fn disclosure_authority(
        &self,
        tx_id: &str,
        action: u32,
    ) -> anyhow::Result<RetainedAuthority> {
        let pool = self.pool.clone();
        let tx_id = tx_id.to_owned();
        spawn_blocking(move || {
            let bytes: Option<Vec<u8>> = pool
                .get()?
                .query_row(
                    "SELECT authorities FROM disclosure_outgoing WHERE tx_id = ?1",
                    [tx_id],
                    |row| row.get(0),
                )
                .optional()?;
            let authorities: Vec<RetainedAuthority> = serde_json::from_slice(&bytes.context(
                "unavailable witness: historical payment has no retained authorization randomizer",
            )?)?;
            authorities
                .into_iter()
                .find(|a| a.action == action)
                .context("unavailable witness: selected Transfer authority was not retained")
        })
        .await?
    }

    pub async fn prepare_disclosure(
        &self,
        request: DisclosureRequest,
    ) -> anyhow::Result<DisclosureWitness> {
        disclosure::validate_request(&request)?;
        let mut transactions = Vec::new();
        for claim in &request.outputs {
            let (height, tx) = self
                .transaction_by_hash(&hex::decode(&claim.reference.transaction_id)?)
                .await?
                .context("selected accepted transaction unavailable in wallet")?;
            ensure!(
                height == claim.reference.height,
                "selected transaction height differs from wallet record"
            );
            transactions.push(tx);
        }
        let mut witness =
            disclosure::prepare(request, &transactions, &self.full_viewing_key().await?)?;
        for (claim, output) in witness.request.outputs.iter().zip(&mut witness.outputs) {
            if !claim.metadata_fields.is_empty() {
                let pool = self.pool.clone();
                let tx_id = claim.reference.transaction_id.clone();
                let bytes = spawn_blocking(move || {
                    pool.get()?.query_row("SELECT m.opening FROM disclosure_metadata m JOIN disclosure_outgoing o ON m.commitment = o.metadata_commitment WHERE o.tx_id = ?1", [tx_id], |row| row.get::<_, Vec<u8>>(0)).optional().map_err(anyhow::Error::from)
                }).await??.context("unavailable witness: payment metadata opening")?;
                output.metadata = Some(serde_json::from_slice(&bytes)?);
            }
        }
        Ok(witness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shieldd_sdk_keys::keys::{SpendKey, SpendKeyBytes};

    #[tokio::test]
    async fn metadata_and_missing_authority_do_not_change_wallet_availability() -> anyhow::Result<()>
    {
        let key = SpendKey::try_from(SpendKeyBytes([17; 32]))?;
        let fvk = key.full_viewing_key().clone();
        let storage =
            Storage::initialize(None::<&Utf8Path>, fvk.clone(), AppParameters::default()).await?;
        let before = storage.last_sync_height().await?;
        let document = MetadataDocument {
            entries: vec![disclosure::MetadataEntry {
                action: disclosure::ActionRef::Body(0),
                output: 0,
                name: "invoice".into(),
                value: "42".into(),
            }],
        };
        let first = storage
            .prepare_disclosure_metadata(document.clone(), fvk.payment_address(0u32.into()))
            .await?;
        let second = storage
            .prepare_disclosure_metadata(document, fvk.payment_address(0u32.into()))
            .await?;
        assert_ne!(first.plaintext.text(), second.plaintext.text());
        assert_eq!(storage.last_sync_height().await?, before);
        let error = storage
            .disclosure_authority(&"ab".repeat(32), 0)
            .await
            .err()
            .context("missing authority must fail")?;
        assert!(error.to_string().contains("unavailable witness"));
        let conn = storage.pool.get()?;
        let entries: u64 =
            conn.query_row("SELECT COUNT(*) FROM disclosure_metadata", [], |row| {
                row.get(0)
            })?;
        assert_eq!(entries, 2);
        Ok(())
    }
}
