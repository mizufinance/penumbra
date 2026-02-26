use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use penumbra_sdk_compliance::registry::ComplianceRegistryRead;
use penumbra_sdk_proof_params::OUTPUT_PROOF_VERIFICATION_KEY;
use penumbra_sdk_proto::{DomainType as _, StateWriteProto as _};
use penumbra_sdk_sct::component::{clock::EpochRead, source::SourceContext};

use crate::{component::NoteManager, event, output::OutputProofPublic, Output};

#[async_trait]
impl ActionHandler for Output {
    type CheckStatelessContext = ();

    async fn check_stateless(&self, _context: ()) -> Result<()> {
        let output = self;

        let asset_anchor = output.body.asset_anchor;
        let compliance_anchor = output.body.compliance_anchor;

        use penumbra_sdk_compliance::structs::{ComplianceCiphertext, OUTPUT_WIRE_BYTES};
        anyhow::ensure!(
            output.body.compliance_ciphertext.len() == OUTPUT_WIRE_BYTES,
            "output compliance ciphertext must be {OUTPUT_WIRE_BYTES} bytes, got {}",
            output.body.compliance_ciphertext.len()
        );
        let ct = ComplianceCiphertext::from_bytes(&output.body.compliance_ciphertext)
            .context("failed to deserialize compliance ciphertext")?;
        let (epk_1, epk_2, epk_3, c2_core, c2_ext, c2_sext, compliance_ciphertext) =
            ct.to_output_circuit_public_inputs();

        // Deserialize DLEQ proofs from body (c_1||s_1||c_2||s_2||c_3||s_3, 192 bytes)
        let (dleq_c_1, dleq_s_1, dleq_c_2, dleq_s_2, dleq_c_3, dleq_s_3) =
            if output.body.dleq_proofs.len() == 192 {
                let parse = |offset: usize| -> anyhow::Result<decaf377::Fq> {
                    let bytes: [u8; 32] = output.body.dleq_proofs[offset..offset + 32]
                        .try_into()
                        .context("dleq field must be 32 bytes")?;
                    decaf377::Fq::from_bytes_checked(&bytes)
                        .map_err(|_| anyhow::anyhow!("invalid dleq field element"))
                };
                (
                    parse(0)?,
                    parse(32)?,
                    parse(64)?,
                    parse(96)?,
                    parse(128)?,
                    parse(160)?,
                )
            } else {
                (
                    decaf377::Fq::from(0u64),
                    decaf377::Fq::from(0u64),
                    decaf377::Fq::from(0u64),
                    decaf377::Fq::from(0u64),
                    decaf377::Fq::from(0u64),
                    decaf377::Fq::from(0u64),
                )
            };
        let target_timestamp = decaf377::Fq::from(output.body.target_timestamp);

        output.proof.verify(
            &OUTPUT_PROOF_VERIFICATION_KEY,
            OutputProofPublic {
                balance_commitment: output.body.balance_commitment,
                note_commitment: output.body.note_payload.note_commitment,
                epk_1,
                epk_2,
                epk_3,
                c2_core,
                c2_ext,
                c2_sext,
                compliance_ciphertext,
                target_timestamp,
                dleq_c_1,
                dleq_s_1,
                dleq_c_2,
                dleq_s_2,
                dleq_c_3,
                dleq_s_3,
                asset_anchor,
                compliance_anchor,
                counterparty_leaf_hash: output.body.counterparty_leaf_hash,
            },
        )?;

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
