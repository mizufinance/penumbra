use crate::*;
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub const CIRCUIT_ID: &str = "shieldd.disclosure.bls12-377.groth16.v1.32";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    circuit: String,
    development: bool,
    vk_sha256: String,
    pk_sha256: String,
}
fn artifacts() -> Result<(std::path::PathBuf, Manifest)> {
    let path = std::path::PathBuf::from(
        std::env::var("SHIELDD_DISCLOSURE_ARTIFACTS")
            .context("disclosure artifacts are not configured")?,
    );
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(path.join("manifest.json"))?)?;
    ensure!(manifest.circuit == CIRCUIT_ID, "wrong configured circuit");
    // Production approval is deliberately unavailable until an approved ceremony is pinned.
    ensure!(
        manifest.development && cfg!(all(feature = "development-artifacts", debug_assertions)),
        "no approved production disclosure setup is installed"
    );
    for hash in [&manifest.vk_sha256, &manifest.pk_sha256] {
        ensure!(hex::decode(hash)?.len() == 32, "invalid artifact digest");
    }
    Ok((path, manifest))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PredicateInput {
    op: String,
    lower: String,
    upper: String,
    result: String,
}
impl Default for PredicateInput {
    fn default() -> Self {
        Self {
            op: "0".into(),
            lower: "0".into(),
            upper: "0".into(),
            result: "0".into(),
        }
    }
}
fn predicate(p: Option<&AmountPredicate>, result: Option<bool>) -> Result<PredicateInput> {
    ensure!(
        p.is_some() == result.is_some(),
        "predicate result shape mismatch"
    );
    let mut out = PredicateInput::default();
    if let Some(p) = p {
        let (op, lo, hi) = match p {
            AmountPredicate::GreaterThan(v) => (1, v.as_str(), "0"),
            AmountPredicate::LessThan(v) => (2, v.as_str(), "0"),
            AmountPredicate::AtLeast(v) => (3, v.as_str(), "0"),
            AmountPredicate::AtMost(v) => (4, v.as_str(), "0"),
            AmountPredicate::InclusiveRange { lower, upper } => (5, lower.as_str(), upper.as_str()),
        };
        out = PredicateInput {
            op: op.to_string(),
            lower: lo.into(),
            upper: hi.into(),
            result: bit(result.unwrap()),
        };
    }
    Ok(out)
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Slot {
    active: String,
    commitment: String,
    reveal_amount: String,
    reveal_asset: String,
    reveal_recipient: String,
    amount: String,
    asset: String,
    generator: String,
    transmission: String,
    predicate: PredicateInput,
}
impl Default for Slot {
    fn default() -> Self {
        Self {
            active: bit(false),
            commitment: bit(false),
            reveal_amount: bit(false),
            reveal_asset: bit(false),
            reveal_recipient: bit(false),
            amount: bit(false),
            asset: bit(false),
            generator: bit(false),
            transmission: bit(false),
            predicate: PredicateInput::default(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct OpeningInput {
    blinding: String,
    amount: String,
    asset: String,
    generator: String,
    transmission: String,
    recovery: String,
}
impl Default for OpeningInput {
    fn default() -> Self {
        Self {
            blinding: bit(false),
            amount: bit(false),
            asset: bit(false),
            generator: bit(false),
            transmission: bit(false),
            recovery: bit(false),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Assignment {
    context: [String; 2],
    context_hash: String,
    slots: Vec<Slot>,
    total_enabled: String,
    total_reveal: String,
    total_amount: String,
    total_asset: String,
    total_predicate: PredicateInput,
    notes: Vec<OpeningInput>,
}
fn bit(v: bool) -> String {
    if v { "1" } else { "0" }.into()
}
fn decimal(f: decaf377::Fq) -> String {
    num_bigint::BigUint::from_bytes_le(&f.to_bytes()).to_string()
}
fn canonical_amount(v: &str) -> Result<String> {
    let n: u128 = v.parse()?;
    ensure!(n.to_string() == v, "noncanonical amount");
    Ok(v.into())
}
fn asset(v: &str) -> Result<String> {
    let a: shieldd_sdk_asset::asset::Id = v.parse()?;
    ensure!(a.to_string() == v, "noncanonical asset");
    Ok(decimal(a.0))
}
fn recipient(v: &str) -> Result<(String, String)> {
    let a: shieldd_sdk_keys::Address = v.parse()?;
    ensure!(a.to_string() == v, "noncanonical address");
    Ok((
        decimal(a.diversified_generator().vartime_compress_to_field()),
        decimal(
            decaf377::Fq::from_bytes_checked(&a.transmission_key().0)
                .map_err(|_| anyhow::anyhow!("invalid transmission key"))?,
        ),
    ))
}
fn assignment(s: &DisclosureStatement, openings: Option<&[NoteOpening]>) -> Result<Assignment> {
    validate_request(&s.request)?;
    ensure!(
        s.outputs.len() == s.request.outputs.len(),
        "output count mismatch"
    );
    let mut h = Sha256::new();
    h.update(b"shieldd.disclosure.statement.v1\0");
    h.update(serde_json::to_vec(s)?);
    let digest: [u8; 32] = h.finalize().into();
    let halves = [
        u128::from_le_bytes(digest[..16].try_into()?),
        u128::from_le_bytes(digest[16..].try_into()?),
    ];
    let hash = poseidon377::hash_2(
        &decaf377::Fq::from(332u64),
        (decaf377::Fq::from(halves[0]), decaf377::Fq::from(halves[1])),
    );
    let mut a = Assignment {
        context: halves.map(|n| n.to_string()),
        context_hash: decimal(hash),
        slots: vec![Slot::default(); MAX_OUTPUTS],
        total_enabled: bit(false),
        total_reveal: bit(false),
        total_amount: bit(false),
        total_asset: bit(false),
        total_predicate: PredicateInput::default(),
        notes: vec![OpeningInput::default(); MAX_OUTPUTS],
    };
    for (i, (c, o)) in s.request.outputs.iter().zip(&s.outputs).enumerate() {
        ensure!(c.reference == o.public.reference, "reference mismatch");
        ensure!(
            !c.memo && o.memo.is_none(),
            "memo disclosure requires payload keys"
        );
        let reveal_asset = c.asset || c.predicate.is_some();
        ensure!(
            c.amount == o.amount.is_some()
                && reveal_asset == o.asset.is_some()
                && c.recipient == o.recipient.is_some(),
            "field selection mismatch"
        );
        let (generator, transmission) = o
            .recipient
            .as_deref()
            .map(recipient)
            .transpose()?
            .unwrap_or((bit(false), bit(false)));
        a.slots[i] = Slot {
            active: bit(true),
            commitment: decimal(field(&o.public.commitment)?),
            reveal_amount: bit(c.amount),
            reveal_asset: bit(reveal_asset),
            reveal_recipient: bit(c.recipient),
            amount: o
                .amount
                .as_deref()
                .map(canonical_amount)
                .transpose()?
                .unwrap_or(bit(false)),
            asset: o
                .asset
                .as_deref()
                .map(asset)
                .transpose()?
                .unwrap_or(bit(false)),
            generator,
            transmission,
            predicate: predicate(c.predicate.as_ref(), o.predicate_result)?,
        };
    }
    ensure!(
        s.request.total.is_some() == s.selected_output_total.is_some(),
        "total shape mismatch"
    );
    if let (Some(c), Some(t)) = (&s.request.total, &s.selected_output_total) {
        ensure!(
            c.reveal == t.amount.is_some(),
            "total amount shape mismatch"
        );
        a.total_enabled = bit(true);
        a.total_reveal = bit(c.reveal);
        a.total_asset = asset(&t.asset)?;
        a.total_amount = t
            .amount
            .as_deref()
            .map(canonical_amount)
            .transpose()?
            .unwrap_or(bit(false));
        a.total_predicate = predicate(c.predicate.as_ref(), t.predicate_result)?;
    }
    if let Some(openings) = openings {
        ensure!(openings.len() == s.outputs.len(), "opening count mismatch");
        for (i, o) in openings.iter().enumerate() {
            let (generator, transmission) = recipient(&o.recipient)?;
            a.notes[i] = OpeningInput {
                blinding: decimal(field(&o.blinding)?),
                amount: canonical_amount(&o.amount)?,
                asset: asset(&o.asset)?,
                generator,
                transmission,
                recovery: decimal(field(&o.recovery)?),
            };
        }
    }
    Ok(a)
}
#[derive(Serialize)]
struct BackendRequest {
    assignment: Assignment,
    proof: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BackendResponse {
    #[serde(default)]
    proof: String,
    verified: bool,
}
fn backend(op: &str, assignment: Assignment, proof: &[u8]) -> Result<BackendResponse> {
    let (path, _) = artifacts()?;
    let executable = std::env::var("SHIELDD_DISCLOSURE_BACKEND")
        .context("local disclosure backend is not configured")?;
    let bytes = serde_json::to_vec(&BackendRequest {
        assignment,
        proof: STANDARD.encode(proof),
    })?;
    ensure!(bytes.len() <= MAX_WITNESS_BYTES, "backend input too large");
    let mut child = Command::new(executable)
        .arg(op)
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env(
            "GOMAXPROCS",
            std::env::var("GOMAXPROCS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|v| *v > 0)
                .unwrap_or(2)
                .min(2)
                .to_string(),
        )
        .spawn()?;
    let write = child
        .stdin
        .take()
        .context("missing prover stdin")?
        .write_all(&bytes);
    if let Err(e) = write {
        let _ = child.kill();
        let _ = child.wait();
        return Err(e.into());
    }
    let out = child.wait_with_output()?;
    ensure!(out.status.success(), "local disclosure backend failed");
    ensure!(
        out.stdout.len() <= MAX_PACKAGE_BYTES,
        "backend output too large"
    );
    let response: BackendResponse = serde_json::from_slice(&out.stdout)?;
    ensure!(response.verified, "backend rejected proof");
    ensure!(
        op != "verify" || response.proof.is_empty(),
        "unexpected proof output"
    );
    Ok(response)
}

pub fn verify(package: &DisclosurePackage) -> Result<VerificationResult> {
    ensure!(package.version == VERSION, "unsupported disclosure version");
    validate_request(&package.statement.request)?;
    match &package.evidence {
        Evidence::Groth16 {
            circuit,
            verification_key_sha256,
            proof,
            control_signatures,
        } => {
            ensure!(circuit == CIRCUIT_ID, "unsupported circuit");
            let (_, m) = artifacts()?;
            ensure!(
                verification_key_sha256 == &m.vk_sha256,
                "wrong verification key"
            );
            ensure!(proof.len() <= 4096, "proof too large");
            backend("verify", assignment(&package.statement, None)?, proof)?;
            verify_controls(&package.statement, control_signatures)?;
        }
        Evidence::Openings {
            openings,
            control_signatures,
        } => {
            require_full_openings(&package.statement.request)?;
            let public = package
                .statement
                .outputs
                .iter()
                .map(|o| o.public.clone())
                .collect::<Vec<_>>();
            ensure!(
                describe_openings(&package.statement.request, &public, openings)?
                    == package.statement,
                "opening statement mismatch"
            );
            verify_controls(&package.statement, control_signatures)?;
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
    Ok(VerificationResult {
        cryptography_verified: true,
        acceptance: Acceptance::NotChecked,
    })
}
fn signatures(w: &DisclosureWitness) -> Vec<Option<Vec<u8>>> {
    w.outputs
        .iter()
        .map(|o| o.control_signature.clone())
        .collect()
}
fn openings(w: &DisclosureWitness) -> Result<Vec<NoteOpening>> {
    w.outputs
        .iter()
        .map(|o| {
            Ok(note_opening(&shieldd_sdk_shielded_pool::Note::try_from(
                o.note.as_slice(),
            )?))
        })
        .collect()
}
fn require_full_openings(r: &DisclosureRequest) -> Result<()> {
    ensure!(r.outputs.iter().all(|c|c.amount&&c.asset&&c.recipient&&!c.memo),"full openings reveal amount, asset, recipient and blinding; memo disclosure requires payload keys");
    Ok(())
}
pub fn export_openings(w: &DisclosureWitness) -> Result<DisclosurePackage> {
    require_full_openings(&w.request)?;
    let p = DisclosurePackage {
        version: VERSION,
        statement: evaluate(w)?,
        evidence: Evidence::Openings {
            openings: openings(w)?,
            control_signatures: signatures(w),
        },
    };
    verify(&p)?;
    Ok(p)
}
pub fn export_payload_keys(w: &DisclosureWitness) -> Result<DisclosurePackage> {
    let keys = w
        .outputs
        .iter()
        .map(|o| {
            Ok(payload_key(&shieldd_sdk_shielded_pool::Note::try_from(
                o.note.as_slice(),
            )?)?
            .to_vec())
        })
        .collect::<Result<Vec<_>>>()?;
    let p = DisclosurePackage {
        version: VERSION,
        statement: evaluate(w)?,
        evidence: Evidence::PayloadKeys {
            keys,
            control_signatures: signatures(w),
        },
    };
    verify(&p)?;
    Ok(p)
}
#[cfg(feature = "prover")]
pub fn prove(w: &DisclosureWitness) -> Result<DisclosurePackage> {
    let statement = evaluate(w)?;
    let private = openings(w)?;
    let (_, m) = artifacts()?;
    let response = backend("prove", assignment(&statement, Some(&private))?, &[])?;
    let p = DisclosurePackage {
        version: VERSION,
        statement,
        evidence: Evidence::Groth16 {
            circuit: CIRCUIT_ID.into(),
            verification_key_sha256: m.vk_sha256,
            proof: STANDARD.decode(response.proof)?,
            control_signatures: signatures(w),
        },
    };
    verify(&p)?;
    Ok(p)
}
