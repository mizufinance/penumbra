use crate::*;
use anyhow::{ensure, Context, Result};
use decaf377_ka as ka;
use decaf377_rdsa::{Signature, SpendAuth, VerificationKey};
use sha2::{Digest, Sha256};
use shieldd_sdk_keys::{
    symmetric::{PayloadKind, WrappedMemoKey},
    PayloadKey,
};
use shieldd_sdk_shielded_pool::Note;
use shieldd_sdk_transaction::memo::{MemoCiphertext, MemoPlaintext};
use std::collections::BTreeSet;

pub fn decode_witness(bytes: &[u8]) -> Result<DisclosureWitness> {
    ensure!(
        bytes.len() <= MAX_WITNESS_BYTES,
        "witness exceeds size limit"
    );
    Ok(serde_json::from_slice(bytes)?)
}

pub fn decode_package(bytes: &[u8]) -> Result<DisclosurePackage> {
    ensure!(
        bytes.len() <= MAX_PACKAGE_BYTES,
        "package exceeds size limit"
    );
    let package: DisclosurePackage = serde_json::from_slice(bytes)?;
    ensure!(package.version == VERSION, "unsupported disclosure version");
    validate_request(&package.statement.request)?;
    Ok(package)
}

fn amount(value: &str) -> Result<u128> {
    let n: u128 = value.parse().context("invalid unsigned 128-bit amount")?;
    ensure!(n.to_string() == value, "noncanonical amount");
    Ok(n)
}

pub fn validate_request(request: &DisclosureRequest) -> Result<()> {
    ensure!(request.version == VERSION, "unsupported disclosure version");
    ensure!(
        !request.chain_id.is_empty() && request.chain_id.len() <= 256,
        "invalid chain identity"
    );
    ensure!(
        !request.outputs.is_empty() && request.outputs.len() <= MAX_OUTPUTS,
        "invalid selection size"
    );
    for text in [&request.recipient, &request.challenge]
        .into_iter()
        .flatten()
    {
        ensure!(
            !text.is_empty() && text.len() <= 1024,
            "invalid request context"
        );
    }
    let mut references = BTreeSet::new();
    for claim in &request.outputs {
        let r = &claim.reference;
        ensure!(r.height > 0, "invalid block height");
        ensure!(
            hex::decode(&r.transaction_id)?.len() == 32
                && r.transaction_id == r.transaction_id.to_lowercase(),
            "invalid transaction id"
        );
        // Height is location evidence, not an opportunity to count an output twice.
        ensure!(
            references.insert((&r.transaction_id, &r.action, r.output)),
            "duplicate output"
        );
        let names: BTreeSet<_> = claim.metadata_fields.iter().collect();
        ensure!(
            names.len() == claim.metadata_fields.len() && names.len() <= 64,
            "duplicate or excessive metadata fields"
        );
        ensure!(
            names.iter().all(|n| !n.is_empty() && n.len() <= 128),
            "invalid metadata field name"
        );
        if claim.spending_control {
            ensure!(
                request.challenge.as_ref().is_some_and(|x| !x.is_empty()),
                "spending control requires a fresh verifier challenge"
            );
            ensure!(
                matches!(r.action, ActionRef::Body(_)),
                "control requires an ordinary Transfer"
            );
        }
        if let Some(p) = &claim.predicate {
            validate_predicate(p)?;
        }
    }
    if let Some(total) = &request.total {
        ensure!(
            total.reveal || total.predicate.is_some(),
            "empty total claim"
        );
        if let Some(p) = &total.predicate {
            validate_predicate(p)?;
        }
    }
    Ok(())
}

fn validate_predicate(p: &AmountPredicate) -> Result<()> {
    match p {
        AmountPredicate::GreaterThan(v)
        | AmountPredicate::LessThan(v)
        | AmountPredicate::AtLeast(v)
        | AmountPredicate::AtMost(v) => {
            amount(v)?;
        }
        AmountPredicate::InclusiveRange { lower, upper } => {
            ensure!(amount(lower)? <= amount(upper)?, "inverted range")
        }
    }
    Ok(())
}

fn check_predicate(value: u128, p: &AmountPredicate) -> Result<()> {
    validate_predicate(p)?;
    ensure!(
        match p {
            AmountPredicate::GreaterThan(v) => value > amount(v)?,
            AmountPredicate::LessThan(v) => value < amount(v)?,
            AmountPredicate::AtLeast(v) => value >= amount(v)?,
            AmountPredicate::AtMost(v) => value <= amount(v)?,
            AmountPredicate::InclusiveRange { lower, upper } =>
                value >= amount(lower)? && value <= amount(upper)?,
        },
        "amount predicate is false"
    );
    Ok(())
}

pub fn metadata_commitment(opening: &MetadataOpening) -> Result<String> {
    let bytes = serde_json::to_vec(&opening.document)?;
    ensure!(
        bytes.len() <= MAX_DOCUMENT_BYTES,
        "metadata document too large"
    );
    let mut names = BTreeSet::new();
    for e in &opening.document.entries {
        ensure!(
            !e.name.is_empty() && e.name.len() <= 128,
            "invalid metadata name"
        );
        ensure!(
            names.insert((&e.action, e.output, &e.name)),
            "duplicate metadata entry"
        );
    }
    let mut hash = Sha256::new();
    hash.update(b"shieldd.disclosure.metadata.v1\0");
    hash.update(opening.salt);
    hash.update(bytes);
    Ok(format!(
        "shieldd-disclosure-v1:{}",
        hex::encode(hash.finalize())
    ))
}

pub fn control_message(request: &DisclosureRequest) -> Result<[u8; 32]> {
    validate_request(request)?;
    let mut hash = Sha256::new();
    hash.update(b"shieldd.disclosure.control.v1\0");
    hash.update(serde_json::to_vec(request)?);
    Ok(hash.finalize().into())
}

pub fn payload_key(note: &Note) -> Result<PayloadKey> {
    let secret = note
        .ephemeral_secret_key()
        .key_agreement_with(note.transmission_key())?;
    Ok(PayloadKey::derive(&secret, &note.ephemeral_public_key()))
}

fn validate_note(public: &PublicOutput, note: &Note) -> Result<()> {
    ensure!(
        hex::encode(note.commit().0.to_bytes()) == public.commitment,
        "note commitment mismatch"
    );
    ensure!(note.amount().value() > 0, "dummy note cannot be disclosed");
    Ok(())
}

fn decrypt_note(public: &PublicOutput, note: &Note) -> Result<PayloadKey> {
    let epk = ka::Public(public.ephemeral_key.as_slice().try_into()?);
    ensure!(note.ephemeral_public_key() == epk, "ephemeral key mismatch");
    let secret = note
        .ephemeral_secret_key()
        .key_agreement_with(note.transmission_key())?;
    let key = PayloadKey::derive(&secret, &epk);
    // The note is already parsed and its derived epk checked above. Comparing
    // authenticated plaintext bytes avoids parsing and deriving that key again.
    let plaintext = key.decrypt(public.encrypted_note.clone(), PayloadKind::Note)?;
    ensure!(plaintext == note.to_bytes(), "note ciphertext mismatch");
    Ok(key)
}

pub fn evaluate(witness: &DisclosureWitness) -> Result<DisclosureStatement> {
    validate_request(&witness.request)?;
    ensure!(
        witness.outputs.len() == witness.request.outputs.len(),
        "selection/witness count mismatch"
    );
    let mut disclosed = Vec::new();
    let mut sum = 0u128;
    let mut total_asset = None;
    let control_digest = witness
        .request
        .outputs
        .iter()
        .any(|claim| claim.spending_control)
        .then(|| control_message(&witness.request))
        .transpose()?;
    let mut verified_controls = BTreeSet::new();
    for (claim, w) in witness.request.outputs.iter().zip(&witness.outputs) {
        ensure!(
            claim.reference == w.public.reference,
            "output reference mismatch"
        );
        let note = Note::try_from(w.note.as_slice())?;
        validate_note(&w.public, &note)?;
        let value = note.amount().value();
        let asset = note.asset_id().to_string();
        if let Some(p) = &claim.predicate {
            check_predicate(value, p)?;
        }
        if witness.request.total.is_some() {
            if let Some(a) = &total_asset {
                ensure!(a == &asset, "mixed assets in selected-output total");
            }
            total_asset = Some(asset.clone());
            sum = sum
                .checked_add(value)
                .context("selected-output total overflow")?;
        }
        if claim.spending_control {
            let key: [u8; 32] = w
                .public
                .spend_verification_key
                .as_deref()
                .context("not an ordinary Transfer")?
                .try_into()?;
            let sig: [u8; 64] = w
                .control_signature
                .as_deref()
                .context("missing control signature")?
                .try_into()?;
            // Reuse is scoped to this request digest, never across challenges.
            if !verified_controls.contains(&(key, sig)) {
                let vk = VerificationKey::<SpendAuth>::try_from(key.as_slice())?;
                ensure!(
                    !vk.is_identity(),
                    "identity authority cannot prove secret control"
                );
                vk.verify(
                    control_digest
                        .as_ref()
                        .context("missing control challenge")?,
                    &Signature::from(sig),
                )?;
                verified_controls.insert((key, sig));
            }
        } else {
            ensure!(
                w.control_signature.is_none(),
                "unexpected control signature"
            );
        }
        let mut memo = None;
        let mut metadata = Vec::new();
        if claim.memo || !claim.metadata_fields.is_empty() {
            let key = decrypt_note(&w.public, &note)?;
            let wrapped = WrappedMemoKey::try_from(w.public.wrapped_memo_key.as_slice())?;
            let memo_key = wrapped.decrypt_outgoing(&key)?;
            let ciphertext = MemoCiphertext(
                w.public
                    .memo_ciphertext
                    .as_deref()
                    .context("transaction has no memo")?
                    .try_into()?,
            );
            let plaintext = MemoCiphertext::decrypt_bytes(&memo_key, ciphertext)?;
            if !claim.metadata_fields.is_empty() {
                let opening = w.metadata.as_ref().context("missing metadata opening")?;
                let commitment = metadata_commitment(opening)?;
                let text = &plaintext[shieldd_sdk_keys::address::ADDRESS_LEN_BYTES..];
                // Metadata commits to the padded memo text, not its hidden return address.
                ensure!(
                    text.starts_with(commitment.as_bytes())
                        && text[commitment.len()..].iter().all(|b| *b == 0),
                    "metadata commitment mismatch"
                );
                for name in &claim.metadata_fields {
                    let entry = opening
                        .document
                        .entries
                        .iter()
                        .find(|e| {
                            e.action == claim.reference.action
                                && e.output == claim.reference.output
                                && &e.name == name
                        })
                        .context("metadata field not committed for this output")?;
                    metadata.push(entry.clone());
                }
            }
            if claim.memo {
                let plaintext = MemoPlaintext::try_from(plaintext.to_vec())?;
                memo = Some(DisclosedMemo {
                    return_address: plaintext.return_address().to_string(),
                    text: plaintext.text().to_owned(),
                });
            }
        }
        disclosed.push(DisclosedOutput {
            public: w.public.clone(),
            amount: claim.amount.then(|| value.to_string()),
            asset: (claim.asset || claim.predicate.is_some()).then_some(asset),
            recipient: claim.recipient.then(|| note.address().to_string()),
            memo,
            metadata,
        });
    }
    let total = if let Some(claim) = &witness.request.total {
        if let Some(p) = &claim.predicate {
            check_predicate(sum, p)?;
        }
        Some(DisclosedTotal {
            asset: total_asset.context("empty total")?,
            amount: claim.reveal.then(|| sum.to_string()),
        })
    } else {
        None
    };
    Ok(DisclosureStatement {
        request: witness.request.clone(),
        outputs: disclosed,
        selected_output_total: total,
    })
}

/// Inspection describes claimed disclosures and capabilities; it does not verify them.
pub fn inspect(package: &DisclosurePackage) -> Result<Inspection> {
    ensure!(package.version == VERSION, "unsupported disclosure version");
    validate_request(&package.statement.request)?;
    let decryption = matches!(package.evidence, Evidence::PayloadKeys { .. });
    Ok(Inspection {
        statement: package.statement.clone(),
        grants_note_decryption: decryption,
        grants_transaction_wide_memo_decryption: decryption,
        context: package.context.clone(),
    })
}
