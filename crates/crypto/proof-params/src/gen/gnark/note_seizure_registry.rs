#[derive(Clone, Copy, Debug)]
struct GeneratedNoteSeizureProofFamily {
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    verifying_key_json_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

static NOTE_SEIZURE_PROOF_VERIFYING_KEY_JSON_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/note_seizure/verifying_key.json"
));

static NOTE_SEIZURE_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> =
    Lazy::new(|| {
        load_verifying_key_json_bytes(NOTE_SEIZURE_PROOF_VERIFYING_KEY_JSON_BYTES)
            .expect("bundled note seizure VerifyingKey is valid")
            .into()
    });

static NOTE_SEIZURE_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/note_seizure/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static NOTE_SEIZURE_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/note_seizure/circuit_metadata.json"
));

static GENERATED_NOTE_SEIZURE_PROOF_FAMILY: GeneratedNoteSeizureProofFamily =
    GeneratedNoteSeizureProofFamily {
        verification_key: &NOTE_SEIZURE_PROOF_VERIFICATION_KEY,
        proving_key_bytes: NOTE_SEIZURE_PROOF_PROVING_KEY_BYTES,
        verifying_key_json_bytes: NOTE_SEIZURE_PROOF_VERIFYING_KEY_JSON_BYTES,
        metadata_bytes: NOTE_SEIZURE_CIRCUIT_METADATA,
    };

pub fn note_seizure_proof_verification_key() -> &'static PreparedVerifyingKey<Bls12_377> {
    &**GENERATED_NOTE_SEIZURE_PROOF_FAMILY.verification_key
}

pub fn note_seizure_proving_key_bytes() -> &'static [u8] {
    GENERATED_NOTE_SEIZURE_PROOF_FAMILY.proving_key_bytes
}

pub fn note_seizure_verifying_key_json_bytes() -> &'static [u8] {
    GENERATED_NOTE_SEIZURE_PROOF_FAMILY.verifying_key_json_bytes
}

pub fn note_seizure_circuit_metadata() -> &'static [u8] {
    GENERATED_NOTE_SEIZURE_PROOF_FAMILY.metadata_bytes
}
