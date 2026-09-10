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

pub fn predicate_result(value: u128, p: &AmountPredicate) -> Result<bool> {
    validate_predicate(p)?;
    Ok(match p {
        AmountPredicate::GreaterThan(v) => value > amount(v)?,
        AmountPredicate::LessThan(v) => value < amount(v)?,
        AmountPredicate::AtLeast(v) => value >= amount(v)?,
        AmountPredicate::AtMost(v) => value <= amount(v)?,
        AmountPredicate::InclusiveRange { lower, upper } => {
            value >= amount(lower)? && value <= amount(upper)?
        }
    })
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

pub fn note_opening(note: &Note) -> NoteOpening {
    NoteOpening {
        blinding: hex::encode(note.note_blinding().to_bytes()),
        amount: note.amount().value().to_string(),
        asset: note.asset_id().to_string(),
        recipient: note.address().to_string(),
        recovery: hex::encode(note.recovery_commitment().0.to_bytes()),
    }
}

pub fn field(value: &str) -> Result<decaf377::Fq> {
    let bytes: [u8; 32] = hex::decode(value)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid field length"))?;
    ensure!(hex::encode(bytes) == value, "noncanonical field");
    Ok(decaf377::Fq::from_bytes_checked(&bytes).map_err(|_| anyhow::anyhow!("invalid field"))?)
}

pub fn describe_openings(
    request: &DisclosureRequest,
    public: &[PublicOutput],
    openings: &[NoteOpening],
) -> Result<DisclosureStatement> {
    validate_request(request)?;
    ensure!(
        public.len() == request.outputs.len() && openings.len() == public.len(),
        "opening count mismatch"
    );
    let mut outputs = Vec::new();
    let mut sum = 0u128;
    let mut total_asset = None;
    for ((claim, p), o) in request.outputs.iter().zip(public).zip(openings) {
        ensure!(claim.reference == p.reference, "output reference mismatch");
        let address: shieldd_sdk_keys::Address = o.recipient.parse()?;
        ensure!(address.to_string() == o.recipient, "noncanonical address");
        let asset: shieldd_sdk_asset::asset::Id = o.asset.parse()?;
        ensure!(asset.to_string() == o.asset, "noncanonical asset");
        let value = amount(&o.amount)?;
        ensure!(value > 0, "dummy note cannot be disclosed");
        let commitment = shieldd_sdk_shielded_pool::note::commitment_from_address(
            address.clone(),
            shieldd_sdk_asset::Value {
                amount: value.into(),
                asset_id: asset,
            },
            field(&o.blinding)?,
            shieldd_sdk_shielded_pool::RecoveryCommitment(field(&o.recovery)?),
        )?;
        ensure!(
            hex::encode(commitment.0.to_bytes()) == p.commitment,
            "note commitment mismatch"
        );
        if request.total.is_some() {
            if let Some(a) = &total_asset {
                ensure!(a == &o.asset, "mixed assets in selected-output total")
            }
            total_asset = Some(o.asset.clone());
            sum = sum
                .checked_add(value)
                .context("selected-output total overflow")?;
        }
        outputs.push(DisclosedOutput {
            public: p.clone(),
            amount: claim.amount.then(|| o.amount.clone()),
            asset: (claim.asset || claim.predicate.is_some()).then(|| o.asset.clone()),
            recipient: claim.recipient.then(|| o.recipient.clone()),
            memo: None,
            predicate_result: claim
                .predicate
                .as_ref()
                .map(|q| predicate_result(value, q))
                .transpose()?,
        });
    }
    let selected_output_total = request
        .total
        .as_ref()
        .map(|t| -> Result<DisclosedTotal> {
            Ok(DisclosedTotal {
                asset: total_asset.context("empty total")?,
                amount: t.reveal.then(|| sum.to_string()),
                predicate_result: t
                    .predicate
                    .as_ref()
                    .map(|p| predicate_result(sum, p))
                    .transpose()?,
            })
        })
        .transpose()?;
    Ok(DisclosureStatement {
        request: request.clone(),
        outputs,
        selected_output_total,
    })
}

pub fn verify_controls(
    statement: &DisclosureStatement,
    signatures: &[Option<Vec<u8>>],
) -> Result<()> {
    ensure!(
        signatures.len() == statement.outputs.len(),
        "signature count mismatch"
    );
    let digest = control_message(&statement.request)?;
    for ((claim, o), sig) in statement
        .request
        .outputs
        .iter()
        .zip(&statement.outputs)
        .zip(signatures)
    {
        if claim.spending_control {
            let key = o
                .public
                .spend_verification_key
                .as_deref()
                .context("not an ordinary Transfer")?;
            let vk = VerificationKey::<SpendAuth>::try_from(key)?;
            ensure!(!vk.is_identity(), "identity authority");
            let bytes: [u8; 64] = sig
                .as_deref()
                .context("missing control signature")?
                .try_into()?;
            vk.verify(&digest, &Signature::from(bytes))?;
        } else {
            ensure!(sig.is_none(), "unexpected control signature");
        }
    }
    Ok(())
}

pub fn evaluate(witness: &DisclosureWitness) -> Result<DisclosureStatement> {
    let notes = witness
        .outputs
        .iter()
        .map(|w| Note::try_from(w.note.as_slice()))
        .collect::<Result<Vec<_>, _>>()?;
    let openings = notes.iter().map(note_opening).collect::<Vec<_>>();
    let public = witness
        .outputs
        .iter()
        .map(|w| w.public.clone())
        .collect::<Vec<_>>();
    let mut statement = describe_openings(&witness.request, &public, &openings)?;
    for ((claim, w), (note, o)) in witness
        .request
        .outputs
        .iter()
        .zip(&witness.outputs)
        .zip(notes.iter().zip(&mut statement.outputs))
    {
        if claim.memo {
            let key = decrypt_note(&w.public, note)?;
            let wrapped = WrappedMemoKey::try_from(w.public.wrapped_memo_key.as_slice())?;
            let memo_key = wrapped.decrypt_outgoing(&key)?;
            let ciphertext = MemoCiphertext(
                w.public
                    .memo_ciphertext
                    .as_deref()
                    .context("transaction has no memo")?
                    .try_into()?,
            );
            let plaintext = MemoPlaintext::try_from(
                MemoCiphertext::decrypt_bytes(&memo_key, ciphertext)?.to_vec(),
            )?;
            o.memo = Some(DisclosedMemo {
                return_address: plaintext.return_address().to_string(),
                text: plaintext.text().to_owned(),
            });
        }
    }
    verify_controls(
        &statement,
        &witness
            .outputs
            .iter()
            .map(|w| w.control_signature.clone())
            .collect::<Vec<_>>(),
    )?;
    Ok(statement)
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
    })
}
