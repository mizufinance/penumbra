#[derive(Clone, Copy, Debug)]
struct GeneratedSplitProofFamily {
    id: u32,
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

static SPLIT1X4_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {
    if let Some(dir) = std::env::var_os("PENUMBRA_GNARK_SPLIT1X4_ARTIFACT_DIR") {
        return load_verifying_key_json_artifact(Path::new(&dir), "split1x4")
            .expect("can deserialize split1x4 VerifyingKey")
            .into();
    }
    load_verifying_key_json_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/gnark/artifacts/split1x4/verifying_key.json"
    )))
    .expect("bundled split1x4 VerifyingKey is valid")
    .into()
});

static SPLIT1X4_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/split1x4/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static SPLIT1X4_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/split1x4/circuit_metadata.json"
));

static SPLIT1X8_PROOF_VERIFICATION_KEY: Lazy<PreparedVerifyingKey<Bls12_377>> = Lazy::new(|| {
    if let Some(dir) = std::env::var_os("PENUMBRA_GNARK_SPLIT1X8_ARTIFACT_DIR") {
        return load_verifying_key_json_artifact(Path::new(&dir), "split1x8")
            .expect("can deserialize split1x8 VerifyingKey")
            .into();
    }
    load_verifying_key_json_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/gnark/artifacts/split1x8/verifying_key.json"
    )))
    .expect("bundled split1x8 VerifyingKey is valid")
    .into()
});

static SPLIT1X8_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/split1x8/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static SPLIT1X8_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/split1x8/circuit_metadata.json"
));

static GENERATED_SPLIT_PROOF_FAMILIES: &[GeneratedSplitProofFamily] = &[
    GeneratedSplitProofFamily {
        id: 1,
        verification_key: &SPLIT1X4_PROOF_VERIFICATION_KEY,
        proving_key_bytes: SPLIT1X4_PROOF_PROVING_KEY_BYTES,
        metadata_bytes: SPLIT1X4_CIRCUIT_METADATA,
    },
    GeneratedSplitProofFamily {
        id: 2,
        verification_key: &SPLIT1X8_PROOF_VERIFICATION_KEY,
        proving_key_bytes: SPLIT1X8_PROOF_PROVING_KEY_BYTES,
        metadata_bytes: SPLIT1X8_CIRCUIT_METADATA,
    },
];

fn split_proof_family(family_id: u32) -> &'static GeneratedSplitProofFamily {
    GENERATED_SPLIT_PROOF_FAMILIES
        .iter()
        .find(|family| family.id == family_id)
        .unwrap_or_else(|| panic!("unknown split family id {family_id}"))
}

pub fn split_proof_verification_key(family_id: u32) -> &'static PreparedVerifyingKey<Bls12_377> {
    &**split_proof_family(family_id).verification_key
}

pub fn split_proving_key_bytes(family_id: u32) -> &'static [u8] {
    split_proof_family(family_id).proving_key_bytes
}

pub fn split_circuit_metadata(family_id: u32) -> &'static [u8] {
    split_proof_family(family_id).metadata_bytes
}
