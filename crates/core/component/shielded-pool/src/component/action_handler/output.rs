use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use penumbra_sdk_compliance::registry::ComplianceRegistryRead;
use penumbra_sdk_proof_params::OUTPUT_PROOF_VERIFICATION_KEY;
use penumbra_sdk_proto::{DomainType as _, StateWriteProto as _};
use penumbra_sdk_sct::component::{clock::EpochRead, source::SourceContext};

use crate::{component::NoteManager, event, output::OutputProofPublic, Output};

/// Maximum allowed time difference (in seconds) between block timestamp and target_timestamp.
/// Transactions outside this window will be rejected.
const MAX_TIMESTAMP_DELTA_SECONDS: u64 = 3600; // 1 hour

#[async_trait]
impl ActionHandler for Output {
    type CheckStatelessContext = ();

    async fn check_stateless(&self, _context: ()) -> Result<()> {
        let output = self;

        // Use anchors from the action body (set during proof generation).
        // The stateful check will validate these anchors against chain state.
        let asset_anchor = output.body.asset_anchor;
        let compliance_anchor = output.body.compliance_anchor;

        use penumbra_sdk_compliance::structs::ComplianceCiphertext;
        let ct = ComplianceCiphertext::from_bytes(&output.body.compliance_ciphertext)
            .context("failed to deserialize compliance ciphertext")?;
        let (compliance_epk, compliance_epk_g, compliance_ciphertext) =
            ct.to_circuit_public_inputs();

        output.proof.verify(
            &OUTPUT_PROOF_VERIFICATION_KEY,
            OutputProofPublic {
                balance_commitment: output.body.balance_commitment,
                note_commitment: output.body.note_payload.note_commitment,
                compliance_epk,
                compliance_epk_g,
                compliance_ciphertext,
                asset_anchor,
                compliance_anchor,
                target_timestamp: output.body.target_timestamp,
                receiver_leaf_hash: output.body.receiver_leaf_hash,
                counterparty_leaf_hash: output.body.counterparty_leaf_hash,
            },
        )?;

        Ok(())
    }

    async fn check_and_execute<S: StateWrite>(&self, mut state: S) -> Result<()> {
        // 1. Validate target_timestamp is within acceptable window
        let block_time = state.get_current_block_timestamp().await?;
        let block_timestamp = block_time.unix_timestamp() as u64;
        let target_timestamp = self.body.target_timestamp;

        let delta = if block_timestamp >= target_timestamp {
            block_timestamp - target_timestamp
        } else {
            target_timestamp - block_timestamp
        };

        if delta > MAX_TIMESTAMP_DELTA_SECONDS {
            return Err(anyhow!(
                "target_timestamp {} is outside acceptable window (block time: {}, delta: {} seconds, max: {} seconds)",
                target_timestamp,
                block_timestamp,
                delta,
                MAX_TIMESTAMP_DELTA_SECONDS
            ));
        }

        // 2. Enforce Compliance: Validate anchors are valid historical anchors.
        // The proof was already verified in check_stateless using the anchors from body.
        // Here we validate that those anchors exist in the historical anchor records.
        // This allows proofs to be generated at any past block height (similar to SCT).
        state
            .validate_compliance_anchors(&self.body.compliance_anchor, &self.body.asset_anchor)
            .await
            .context("invalid compliance anchors")?;

        // 3. Execute the Output logic (Minting the note)
        let source = state
            .get_current_source()
            .expect("source should be set during execution");

        state
            .add_note_payload(self.body.note_payload.clone(), source.into())
            .await;

        state.record_proto(
            event::EventOutput {
                note_commitment: self.body.note_payload.note_commitment,
            }
            .to_proto(),
        );

        Ok(())
    }
}
