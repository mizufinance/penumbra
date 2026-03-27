use anyhow::{Context, Result};
use async_trait::async_trait;
use cnidarium::StateWrite;
use cnidarium_component::ActionHandler;
use penumbra_sdk_compliance::registry::ComplianceRegistryRead;
use penumbra_sdk_proof_params::batch::{self, BatchItem};
use penumbra_sdk_proof_params::OUTPUT_PROOF_VERIFICATION_KEY;
use penumbra_sdk_proto::{DomainType as _, StateWriteProto as _};
use penumbra_sdk_sct::component::{clock::EpochRead, source::SourceContext};

use crate::{component::NoteManager, event, output::OutputProofPublic, Output};

#[derive(Clone, Debug)]
pub struct OutputCiphertextFields {
    pub epk_1: decaf377::Element,
    pub epk_2: decaf377::Element,
    pub epk_3: decaf377::Element,
    pub c2_core: decaf377::Fq,
    pub c2_ext: decaf377::Fq,
    pub c2_sext: decaf377::Fq,
    pub compliance_ciphertext: Vec<decaf377::Fq>,
}

#[derive(Clone, Debug)]
pub struct OutputDleqFields {
    pub target_timestamp: decaf377::Fq,
    pub dleq_c_1: decaf377::Fq,
    pub dleq_s_1: decaf377::Fq,
    pub dleq_c_2: decaf377::Fq,
    pub dleq_s_2: decaf377::Fq,
    pub dleq_c_3: decaf377::Fq,
    pub dleq_s_3: decaf377::Fq,
}

pub fn output_parse_ciphertext_fields(output: &Output) -> Result<OutputCiphertextFields> {
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

    Ok(OutputCiphertextFields {
        epk_1,
        epk_2,
        epk_3,
        c2_core,
        c2_ext,
        c2_sext,
        compliance_ciphertext,
    })
}

pub fn output_parse_dleq_fields(output: &Output) -> Result<OutputDleqFields> {
    use penumbra_sdk_compliance::structs::OUTPUT_DLEQ_BYTES;

    anyhow::ensure!(
        output.body.dleq_proofs.len() == OUTPUT_DLEQ_BYTES,
        "output dleq_proofs must be {OUTPUT_DLEQ_BYTES} bytes, got {}",
        output.body.dleq_proofs.len()
    );
    let parse_dleq = |offset: usize| -> anyhow::Result<decaf377::Fq> {
        let bytes: [u8; 32] = output.body.dleq_proofs[offset..offset + 32]
            .try_into()
            .context("dleq field must be 32 bytes")?;
        decaf377::Fq::from_bytes_checked(&bytes)
            .map_err(|_| anyhow::anyhow!("invalid dleq field element"))
    };

    Ok(OutputDleqFields {
        target_timestamp: decaf377::Fq::from(output.body.target_timestamp),
        dleq_c_1: parse_dleq(0)?,
        dleq_s_1: parse_dleq(32)?,
        dleq_c_2: parse_dleq(64)?,
        dleq_s_2: parse_dleq(96)?,
        dleq_c_3: parse_dleq(128)?,
        dleq_s_3: parse_dleq(160)?,
    })
}

pub fn output_build_public(
    output: &Output,
    ciphertext: OutputCiphertextFields,
    dleq: OutputDleqFields,
) -> OutputProofPublic {
    OutputProofPublic {
        balance_commitment: output.body.balance_commitment,
        note_commitment: output.body.note_payload.note_commitment,
        epk_1: ciphertext.epk_1,
        epk_2: ciphertext.epk_2,
        epk_3: ciphertext.epk_3,
        c2_core: ciphertext.c2_core,
        c2_ext: ciphertext.c2_ext,
        c2_sext: ciphertext.c2_sext,
        compliance_ciphertext: ciphertext.compliance_ciphertext,
        target_timestamp: dleq.target_timestamp,
        dleq_c_1: dleq.dleq_c_1,
        dleq_s_1: dleq.dleq_s_1,
        dleq_c_2: dleq.dleq_c_2,
        dleq_s_2: dleq.dleq_s_2,
        dleq_c_3: dleq.dleq_c_3,
        dleq_s_3: dleq.dleq_s_3,
        asset_anchor: output.body.asset_anchor,
        compliance_anchor: output.body.compliance_anchor,
        counterparty_leaf_hash: output.body.counterparty_leaf_hash,
    }
}

pub fn output_extract_public(output: &Output) -> Result<OutputProofPublic> {
    let ciphertext = output_parse_ciphertext_fields(output)?;
    let dleq = output_parse_dleq_fields(output)?;
    Ok(output_build_public(output, ciphertext, dleq))
}

pub fn output_to_batch_item(output: &Output, public: OutputProofPublic) -> Result<BatchItem> {
    output.proof.to_batch_item(public)
}

/// Run output stateless checks (ciphertext length) without proof verification, and return
/// a `BatchItem` for deferred batch verification. Used by the batch path in
/// `process_proposal`.
pub fn output_check_stateless_and_extract(output: &Output) -> Result<BatchItem> {
    let public = output_extract_public(output)?;
    output_to_batch_item(output, public)
}

#[async_trait]
impl ActionHandler for Output {
    type CheckStatelessContext = ();

    async fn check_stateless(&self, _context: ()) -> Result<()> {
        let item = output_check_stateless_and_extract(self)?;
        batch::batch_verify(&OUTPUT_PROOF_VERIFICATION_KEY, std::slice::from_ref(&item))
            .map_err(|e| anyhow::anyhow!("output proof did not verify: {e}"))?;

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
            .ok_or_else(|| anyhow::anyhow!("source should be set during execution"))?;

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
