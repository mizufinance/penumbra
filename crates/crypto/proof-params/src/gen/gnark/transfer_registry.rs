#[derive(Clone, Copy, Debug)]
struct GeneratedTransferProofFamily {
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

static TRANSFER_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {
    if let Some(dir) = std::env::var_os("PENUMBRA_GNARK_TRANSFER_ARTIFACT_DIR") {
        return load_verifying_key_json_artifact(Path::new(&dir), "transfer")
            .expect("can deserialize transfer VerifyingKey")
            .into();
    }
    load_verifying_key_json_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/gnark/artifacts/transfer/verifying_key.json"
    )))
    .expect("bundled transfer VerifyingKey is valid")
    .into()
});

static TRANSFER_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/transfer/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static TRANSFER_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/transfer/circuit_metadata.json"
));

static GENERATED_TRANSFER_PROOF_FAMILY: GeneratedTransferProofFamily = GeneratedTransferProofFamily {
    verification_key: &TRANSFER_PROOF_VERIFICATION_KEY,
    proving_key_bytes: TRANSFER_PROOF_PROVING_KEY_BYTES,
    metadata_bytes: TRANSFER_CIRCUIT_METADATA,
};

pub fn transfer_proof_verification_key() -> &'static PreparedVerifyingKey<Bls12_377> {
    &**GENERATED_TRANSFER_PROOF_FAMILY.verification_key
}

pub fn transfer_proving_key_bytes() -> &'static [u8] {
    GENERATED_TRANSFER_PROOF_FAMILY.proving_key_bytes
}

pub fn transfer_circuit_metadata() -> &'static [u8] {
    GENERATED_TRANSFER_PROOF_FAMILY.metadata_bytes
}
