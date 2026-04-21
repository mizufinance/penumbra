#[derive(Clone, Copy, Debug)]
struct GeneratedConsolidateProofFamily {
    id: u32,
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

static CONSOLIDATE2X1_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {
    if let Some(dir) = std::env::var_os("PENUMBRA_GNARK_CONSOLIDATE2X1_ARTIFACT_DIR") {
        return load_verifying_key_json_artifact(Path::new(&dir), "consolidate2x1")
            .expect("can deserialize consolidate2x1 VerifyingKey")
            .into();
    }
    load_verifying_key_json_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/gnark/artifacts/consolidate2x1/verifying_key.json"
    )))
    .expect("bundled consolidate2x1 VerifyingKey is valid")
    .into()
});

static CONSOLIDATE2X1_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/consolidate2x1/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static CONSOLIDATE2X1_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/consolidate2x1/circuit_metadata.json"
));

static CONSOLIDATE4X1_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {
    if let Some(dir) = std::env::var_os("PENUMBRA_GNARK_CONSOLIDATE4X1_ARTIFACT_DIR") {
        return load_verifying_key_json_artifact(Path::new(&dir), "consolidate4x1")
            .expect("can deserialize consolidate4x1 VerifyingKey")
            .into();
    }
    load_verifying_key_json_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/gnark/artifacts/consolidate4x1/verifying_key.json"
    )))
    .expect("bundled consolidate4x1 VerifyingKey is valid")
    .into()
});

static CONSOLIDATE4X1_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/consolidate4x1/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static CONSOLIDATE4X1_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/consolidate4x1/circuit_metadata.json"
));

static CONSOLIDATE8X1_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {
    if let Some(dir) = std::env::var_os("PENUMBRA_GNARK_CONSOLIDATE8X1_ARTIFACT_DIR") {
        return load_verifying_key_json_artifact(Path::new(&dir), "consolidate8x1")
            .expect("can deserialize consolidate8x1 VerifyingKey")
            .into();
    }
    load_verifying_key_json_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/gnark/artifacts/consolidate8x1/verifying_key.json"
    )))
    .expect("bundled consolidate8x1 VerifyingKey is valid")
    .into()
});

static CONSOLIDATE8X1_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/consolidate8x1/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static CONSOLIDATE8X1_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/consolidate8x1/circuit_metadata.json"
));

static GENERATED_CONSOLIDATE_PROOF_FAMILIES: &[GeneratedConsolidateProofFamily] = &[
    GeneratedConsolidateProofFamily {
        id: 1,
        verification_key: &CONSOLIDATE2X1_PROOF_VERIFICATION_KEY,
        proving_key_bytes: CONSOLIDATE2X1_PROOF_PROVING_KEY_BYTES,
        metadata_bytes: CONSOLIDATE2X1_CIRCUIT_METADATA,
    },
    GeneratedConsolidateProofFamily {
        id: 2,
        verification_key: &CONSOLIDATE4X1_PROOF_VERIFICATION_KEY,
        proving_key_bytes: CONSOLIDATE4X1_PROOF_PROVING_KEY_BYTES,
        metadata_bytes: CONSOLIDATE4X1_CIRCUIT_METADATA,
    },
    GeneratedConsolidateProofFamily {
        id: 3,
        verification_key: &CONSOLIDATE8X1_PROOF_VERIFICATION_KEY,
        proving_key_bytes: CONSOLIDATE8X1_PROOF_PROVING_KEY_BYTES,
        metadata_bytes: CONSOLIDATE8X1_CIRCUIT_METADATA,
    },
];

fn consolidate_proof_family(family_id: u32) -> &'static GeneratedConsolidateProofFamily {
    GENERATED_CONSOLIDATE_PROOF_FAMILIES
        .iter()
        .find(|family| family.id == family_id)
        .unwrap_or_else(|| panic!("unknown consolidate family id {family_id}"))
}

pub fn consolidate_proof_verification_key(
    family_id: u32,
) -> &'static PreparedVerifyingKey<Bls12_377> {
    &**consolidate_proof_family(family_id).verification_key
}

pub fn consolidate_proving_key_bytes(family_id: u32) -> &'static [u8] {
    consolidate_proof_family(family_id).proving_key_bytes
}

pub fn consolidate_circuit_metadata(family_id: u32) -> &'static [u8] {
    consolidate_proof_family(family_id).metadata_bytes
}
