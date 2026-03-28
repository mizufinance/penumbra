use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use decaf377::Fq;
use penumbra_sdk_compliance::registry::ComplianceRegistryRead;
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_proof_params::SPEND_PROOF_VERIFICATION_KEY;
use penumbra_sdk_proto::{DomainType, StateWriteProto as _};
use penumbra_sdk_sct::component::{
    clock::EpochRead,
    source::SourceContext,
    tree::{SctManager, VerificationExt},
};
use penumbra_sdk_txhash::TransactionContext;

use crate::{event, Spend, SpendProofPublic};

#[derive(Clone, Debug)]
pub struct SpendCiphertextFields {
    pub epk: decaf377::Element,
    pub c2_core: Fq,
    pub compliance_ciphertext: Vec<Fq>,
}

#[derive(Clone, Debug)]
pub struct SpendDleqFields {
    pub target_timestamp: Fq,
    pub dleq_c: Fq,
    pub dleq_s: Fq,
}

pub fn spend_verify_auth_sig(spend: &Spend, context: &TransactionContext) -> Result<()> {
    spend
        .body
        .rk
        .verify(context.effect_hash.as_ref(), &spend.auth_sig)
        .context("spend auth signature failed to verify")
}

pub fn spend_parse_ciphertext_fields(spend: &Spend) -> Result<SpendCiphertextFields> {
    use penumbra_sdk_compliance::structs::{ComplianceCiphertext, SPEND_WIRE_BYTES};

    anyhow::ensure!(
        spend.body.compliance_ciphertext.len() == SPEND_WIRE_BYTES,
        "spend compliance ciphertext must be {SPEND_WIRE_BYTES} bytes, got {}",
        spend.body.compliance_ciphertext.len()
    );
    let ct = ComplianceCiphertext::from_bytes(&spend.body.compliance_ciphertext)
        .context("failed to deserialize compliance ciphertext")?;
    let (epk, c2_core, compliance_ciphertext) = ct.to_spend_circuit_public_inputs();

    Ok(SpendCiphertextFields {
        epk,
        c2_core,
        compliance_ciphertext,
    })
}

pub fn spend_parse_dleq_fields(spend: &Spend) -> Result<SpendDleqFields> {
    use penumbra_sdk_compliance::structs::SPEND_DLEQ_BYTES;

    anyhow::ensure!(
        spend.body.dleq_proof.len() == SPEND_DLEQ_BYTES,
        "spend dleq_proof must be {SPEND_DLEQ_BYTES} bytes, got {}",
        spend.body.dleq_proof.len()
    );
    let c_bytes: [u8; 32] = spend.body.dleq_proof[..32]
        .try_into()
        .context("dleq_c must be 32 bytes")?;
    let dleq_c = Fq::from_bytes_checked(&c_bytes)
        .map_err(|_| anyhow::anyhow!("invalid dleq_c field element"))?;
    let s_bytes: [u8; 32] = spend.body.dleq_proof[32..64]
        .try_into()
        .context("dleq_s must be 32 bytes")?;
    let dleq_s = Fq::from_bytes_checked(&s_bytes)
        .map_err(|_| anyhow::anyhow!("invalid dleq_s field element"))?;

    Ok(SpendDleqFields {
        target_timestamp: Fq::from(spend.body.target_timestamp),
        dleq_c,
        dleq_s,
    })
}

pub fn spend_build_public(
    spend: &Spend,
    context: &TransactionContext,
    ciphertext: SpendCiphertextFields,
    dleq: SpendDleqFields,
) -> SpendProofPublic {
    SpendProofPublic {
        anchor: context.anchor,
        balance_commitment: spend.body.balance_commitment,
        nullifier: spend.body.nullifier,
        rk: spend.body.rk,
        asset_anchor: spend.body.asset_anchor,
        compliance_anchor: spend.body.compliance_anchor,
        epk: ciphertext.epk,
        c2_core: ciphertext.c2_core,
        compliance_ciphertext: ciphertext.compliance_ciphertext,
        target_timestamp: dleq.target_timestamp,
        dleq_c: dleq.dleq_c,
        dleq_s: dleq.dleq_s,
        sender_leaf_hash: spend.body.sender_leaf_hash,
    }
}

pub fn spend_extract_public(
    spend: &Spend,
    context: &TransactionContext,
) -> Result<SpendProofPublic> {
    let ciphertext = spend_parse_ciphertext_fields(spend)?;
    let dleq = spend_parse_dleq_fields(spend)?;
    Ok(spend_build_public(spend, context, ciphertext, dleq))
}

pub fn spend_to_batch_item(spend: &Spend, public: SpendProofPublic) -> Result<BatchItem> {
    spend
        .proof
        .to_batch_item(public)
        .map_err(|e| anyhow::anyhow!(e))
}

/// Run spend stateless checks (auth sig) without proof verification, and return
/// a `BatchItem` for deferred batch verification. Used by the batch path in
/// `process_proposal`.
pub fn spend_check_stateless_and_extract(
    spend: &Spend,
    context: &TransactionContext,
) -> Result<BatchItem> {
    spend_verify_auth_sig(spend, context)?;
    let public = spend_extract_public(spend, context)?;
    spend_to_batch_item(spend, public)
}

#[async_trait]
impl ActionHandler for Spend {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        let spend = self;
        let item = spend_check_stateless_and_extract(spend, &context)?;
        batch::batch_verify(&SPEND_PROOF_VERIFICATION_KEY, std::slice::from_ref(&item))
            .map_err(|e| anyhow::anyhow!("a spend proof did not verify: {e}"))?;

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

        let source = state
            .get_current_source()
            .ok_or_else(|| anyhow::anyhow!("source should be set"))?;

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
