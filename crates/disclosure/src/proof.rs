use crate::*;
use anyhow::{ensure, Result};
use risc0_zkvm::Receipt;
use shieldd_disclosure_methods::DISCLOSURE_GUEST_ID;

pub fn verify(package: &DisclosurePackage) -> Result<VerificationResult> {
    ensure!(package.version == VERSION, "unsupported disclosure version");
    validate_request(&package.statement.request)?;
    match &package.evidence {
        Evidence::ZkReceipt(bytes) => {
            ensure!(bytes.len() <= MAX_PACKAGE_BYTES, "receipt too large");
            let receipt: Receipt = serde_json::from_slice(bytes)?;
            ensure!(
                matches!(&receipt.inner, risc0_zkvm::InnerReceipt::Succinct(_)),
                "only native succinct STARK receipts are supported"
            );
            let context = risc0_zkvm::VerifierContext::empty()
                .with_suites(risc0_zkvm::VerifierContext::default_hash_suites())
                .with_succinct_verifier_parameters(Default::default());
            receipt.verify_with_context(&context, DISCLOSURE_GUEST_ID)?;
            let statement: DisclosureStatement = serde_json::from_slice(&receipt.journal.bytes)?;
            ensure!(statement == package.statement, "receipt statement mismatch");
        }
        Evidence::PayloadKeys {
            keys,
            control_signatures,
        } => {
            ensure!(
                keys.len() == package.statement.outputs.len()
                    && keys.len() == control_signatures.len(),
                "payload key count mismatch"
            );
            let mut outputs = Vec::new();
            for ((output, key), signature) in package
                .statement
                .outputs
                .iter()
                .zip(keys)
                .zip(control_signatures)
            {
                let public = &output.public;
                let key = shieldd_sdk_keys::PayloadKey::try_from(key.clone())?;
                let epk = decaf377_ka::Public(public.ephemeral_key.as_slice().try_into()?);
                let ciphertext = shieldd_sdk_shielded_pool::NoteCiphertext(
                    public.encrypted_note.as_slice().try_into()?,
                );
                let note = shieldd_sdk_shielded_pool::Note::decrypt_with_payload_key(
                    &ciphertext,
                    &key,
                    &epk,
                )?;
                outputs.push(OutputWitness {
                    public: public.clone(),
                    note: note.to_bytes().to_vec(),
                    metadata: None,
                    control_signature: signature.clone(),
                });
            }
            let actual = evaluate(&DisclosureWitness {
                request: package.statement.request.clone(),
                outputs,
            })?;
            ensure!(
                actual == package.statement,
                "payload disclosure statement mismatch"
            );
        }
    }
    let verified_attestations = verify_context(package)?;
    Ok(VerificationResult {
        cryptography_verified: true,
        acceptance: Acceptance::NotChecked,
        verified_attestations,
    })
}

#[cfg(feature = "prover")]
pub fn prove(witness: &DisclosureWitness) -> Result<DisclosurePackage> {
    use risc0_zkvm::{ExecutorEnv, ExternalProver, Prover, ProverOpts};
    let statement = evaluate(witness)?;
    let bytes = serde_json::to_vec(witness)?;
    ensure!(bytes.len() <= MAX_WITNESS_BYTES, "witness too large");
    let env = ExecutorEnv::builder()
        .write_slice(&bytes)
        .segment_limit_po2(19)
        .build()?;
    // Explicit local prover: environment variables must never route private input to a service.
    let prover = ExternalProver::new("local", "r0vm");
    let info = prover.prove_with_opts(
        env,
        shieldd_disclosure_methods::DISCLOSURE_GUEST_ELF,
        &ProverOpts::succinct().with_dev_mode(false),
    )?;
    let package = DisclosurePackage {
        version: VERSION,
        statement,
        evidence: Evidence::ZkReceipt(serde_json::to_vec(&info.receipt)?),
        context: Vec::new(),
    };
    verify(&package)?;
    Ok(package)
}

pub fn export_payload_keys(witness: &DisclosureWitness) -> Result<DisclosurePackage> {
    ensure!(
        witness
            .request
            .outputs
            .iter()
            .all(|o| o.metadata_fields.is_empty()),
        "payload export does not carry private metadata documents; use a proof"
    );
    let statement = evaluate(witness)?;
    let keys = witness
        .outputs
        .iter()
        .map(|w| {
            let note = shieldd_sdk_shielded_pool::Note::try_from(w.note.as_slice())?;
            Ok(payload_key(&note)?.to_vec())
        })
        .collect::<Result<Vec<_>>>()?;
    let package = DisclosurePackage {
        version: VERSION,
        statement,
        evidence: Evidence::PayloadKeys {
            keys,
            control_signatures: witness
                .outputs
                .iter()
                .map(|w| w.control_signature.clone())
                .collect(),
        },
        context: Vec::new(),
    };
    verify(&package)?;
    Ok(package)
}

pub fn attestation_message(
    statement: &DisclosureStatement,
    attachment: &ContextAttachment,
) -> Result<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"shieldd.disclosure.attestation.v1\0");
    hash.update(serde_json::to_vec(&(
        statement,
        &attachment.name,
        &attachment.document_sha256,
        &attachment.text,
    ))?);
    Ok(hash.finalize().into())
}

fn verify_context(package: &DisclosurePackage) -> Result<Vec<String>> {
    use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
    ensure!(package.context.len() <= 64, "too many context attachments");
    let mut verified = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for context in &package.context {
        ensure!(
            !context.name.is_empty() && names.insert(&context.name),
            "empty or duplicate context name"
        );
        ensure!(
            context.name.len() <= 128 && context.text.len() <= MAX_DOCUMENT_BYTES,
            "context too large"
        );
        ensure!(
            hex::decode(&context.document_sha256)?.len() == 32,
            "invalid document digest"
        );
        if let Some(attestation) = &context.attestation {
            let vk =
                VerificationKey::<SpendAuth>::try_from(attestation.verification_key.as_slice())?;
            ensure!(
                !vk.is_identity(),
                "identity attestation is not authenticated"
            );
            let sig: [u8; 64] = attestation.signature.as_slice().try_into()?;
            vk.verify(
                &attestation_message(&package.statement, context)?,
                &Signature::from(sig),
            )?;
            verified.push(context.name.clone());
        }
    }
    Ok(verified)
}
