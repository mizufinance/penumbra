use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use decaf377::Fq;
use penumbra_sdk_compliance::registry::ComplianceRegistryRead;
use penumbra_sdk_proof_params::SPEND_PROOF_VERIFICATION_KEY;
use penumbra_sdk_proto::{DomainType, StateWriteProto as _};
use penumbra_sdk_sct::component::{
    clock::EpochRead,
    source::SourceContext,
    tree::{SctManager, VerificationExt},
};
use penumbra_sdk_txhash::TransactionContext;

use crate::{event, Spend, SpendProofPublic};

#[async_trait]
impl ActionHandler for Spend {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let spend = self;

        // 1. Check spend auth signature using provided spend auth key.
        spend
            .body
            .rk
            .verify(context.effect_hash.as_ref(), &spend.auth_sig)
            .context("spend auth signature failed to verify")?;

        // 2. Check that the proof verifies.
        let asset_anchor = spend.body.asset_anchor;
        let compliance_anchor = spend.body.compliance_anchor;

        use penumbra_sdk_compliance::structs::ComplianceCiphertext;
        let ct = ComplianceCiphertext::from_bytes(&spend.body.compliance_ciphertext)
            .context("failed to deserialize compliance ciphertext")?;
        let (epk, c2_core, compliance_ciphertext) = ct.to_spend_circuit_public_inputs();

        // Deserialize DLEQ proof from body (c || s, 64 bytes)
        let (dleq_c, dleq_s) = if spend.body.dleq_proof.len() == 64 {
            let c_bytes: [u8; 32] = spend.body.dleq_proof[..32]
                .try_into()
                .context("dleq_c must be 32 bytes")?;
            let c = Fq::from_bytes_checked(&c_bytes)
                .map_err(|_| anyhow::anyhow!("invalid dleq_c field element"))?;
            let s_bytes: [u8; 32] = spend.body.dleq_proof[32..64]
                .try_into()
                .context("dleq_s must be 32 bytes")?;
            let s = Fq::from_bytes_checked(&s_bytes)
                .map_err(|_| anyhow::anyhow!("invalid dleq_s field element"))?;
            (c, s)
        } else {
            (Fq::from(0u64), Fq::from(0u64))
        };
        let target_timestamp = Fq::from(spend.body.target_timestamp);

        let public = SpendProofPublic {
            anchor: context.anchor,
            balance_commitment: spend.body.balance_commitment,
            nullifier: spend.body.nullifier,
            rk: spend.body.rk,
            asset_anchor,
            compliance_anchor,
            epk,
            c2_core,
            compliance_ciphertext,
            target_timestamp,
            dleq_c,
            dleq_s,
            sender_leaf_hash: spend.body.sender_leaf_hash,
        };

        spend
            .proof
            .verify(&SPEND_PROOF_VERIFICATION_KEY, public)
            .context("a spend proof did not verify")?;

        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        // 1. Enforce Compliance: Validate anchors are valid historical anchors.
        state
            .validate_compliance_anchors(&self.body.compliance_anchor, &self.body.asset_anchor)
            .await
            .context("invalid compliance anchors")?;

        // 2. Enforce timestamp freshness (±1hr of block time).
        let block_time = state.get_current_block_timestamp().await?;
        let block_unix = block_time.unix_timestamp();
        anyhow::ensure!(block_unix >= 0, "block timestamp is negative");
        let block_timestamp = block_unix as u64;
        penumbra_sdk_compliance::registry::check_timestamp_freshness(
            self.body.target_timestamp,
            block_timestamp,
        )?;

        // 3. Check that the `Nullifier` has not been spent before.
        let spent_nullifier = self.body.nullifier;
        state.check_nullifier_unspent(spent_nullifier).await?;

        let source = state.get_current_source().expect("source should be set");

        state.nullify(self.body.nullifier, source.into()).await;

        // Also record an ABCI event for transaction indexing.
        state.record_proto(
            event::EventSpend {
                nullifier: self.body.nullifier,
            }
            .to_proto(),
        );

        Ok(())
    }
}
