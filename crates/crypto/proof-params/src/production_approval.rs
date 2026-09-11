//! Exact verifying-key approvals for release builds.
use anyhow::{ensure, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approvals {
    schema: String,
    keys: Vec<Approval>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    circuit: String,
    verifying_key_sha256_hex: String,
    verifying_key_json_sha256_hex: String,
    approval_reference: String,
}

pub fn approved(bytes: &[u8], circuit: &str, hash: &str, json_hash: &str) -> Result<bool> {
    let approvals: Approvals = serde_json::from_slice(bytes)?;
    ensure!(
        approvals.schema == "shieldd.gnark.production_keys.v1",
        "unsupported proof approval schema"
    );
    let mut seen = std::collections::BTreeSet::new();
    for key in &approvals.keys {
        ensure!(seen.insert(&key.circuit), "duplicate proof approval");
        ensure!(
            !key.circuit.is_empty() && !key.approval_reference.trim().is_empty(),
            "incomplete proof approval"
        );
        for digest in [
            &key.verifying_key_sha256_hex,
            &key.verifying_key_json_sha256_hex,
        ] {
            ensure!(
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid approved key digest"
            );
        }
    }
    Ok(approvals.keys.iter().any(|key| {
        key.circuit == circuit
            && key.verifying_key_sha256_hex == hash
            && key.verifying_key_json_sha256_hex == json_hash
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_only_approval_cannot_approve_the_rust_json_key() {
        let hash = "12".repeat(32);
        let bytes = serde_json::to_vec(&serde_json::json!({"schema":"shieldd.gnark.production_keys.v1",
            "keys":[{"circuit":"transfer","verifying_key_sha256_hex":hash,"approval_reference":"review/approved-ceremony"}]})).unwrap();
        assert!(approved(&bytes, "transfer", &hash, &hash).is_err());
    }

    #[test]
    fn approval_requires_exact_key_and_circuit() {
        let hash = "12".repeat(32);
        let bytes = serde_json::to_vec(&serde_json::json!({"schema":"shieldd.gnark.production_keys.v1",
            "keys":[{"circuit":"transfer","verifying_key_sha256_hex":hash,"verifying_key_json_sha256_hex":hash,"approval_reference":"review/approved-ceremony"}]})).unwrap();
        assert!(approved(&bytes, "transfer", &hash, &hash).unwrap());
        assert!(!approved(&bytes, "transfer", &"34".repeat(32), &hash).unwrap());
        assert!(!approved(&bytes, "transfer", &hash, &"56".repeat(32)).unwrap());
        assert!(!approved(&bytes, "note_reshape1x8", &hash, &hash).unwrap());
        assert!(!approved(
            include_bytes!("../production_keys.json"),
            "transfer",
            &hash,
            &hash
        )
        .unwrap());
        assert!(approved(
            br#"{"schema":"unknown","keys":[]}"#,
            "transfer",
            &hash,
            &hash
        )
        .is_err());
    }
}
