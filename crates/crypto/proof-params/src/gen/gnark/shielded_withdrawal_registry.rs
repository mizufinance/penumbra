// Code generated from shielded_withdrawal_families.json. DO NOT EDIT.
#[derive(Clone, Copy, Debug)]
struct GeneratedShieldedWithdrawalProofFamily {
    id: u32,
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    verifying_key_json_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

static SHIELDED_WITHDRAWAL_PROOF_VERIFYING_KEY_JSON_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/shielded_withdrawal/verifying_key.json"
));

static SHIELDED_WITHDRAWAL_PROOF_VERIFICATION_KEY: Lazy<
    PreparedVerifyingKey<Bls12_377>,
> = Lazy::new(|| {
    load_verifying_key_json_bytes(SHIELDED_WITHDRAWAL_PROOF_VERIFYING_KEY_JSON_BYTES)
    .expect("bundled shielded_withdrawal VerifyingKey is valid")
    .into()
});

static SHIELDED_WITHDRAWAL_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/shielded_withdrawal/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static SHIELDED_WITHDRAWAL_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/shielded_withdrawal/circuit_metadata.json"
));

static GENERATED_SHIELDED_WITHDRAWAL_PROOF_FAMILIES:
    &[GeneratedShieldedWithdrawalProofFamily] = &[
    GeneratedShieldedWithdrawalProofFamily {
        id: 1,
        verification_key: &SHIELDED_WITHDRAWAL_PROOF_VERIFICATION_KEY,
        proving_key_bytes: SHIELDED_WITHDRAWAL_PROOF_PROVING_KEY_BYTES,
        verifying_key_json_bytes: SHIELDED_WITHDRAWAL_PROOF_VERIFYING_KEY_JSON_BYTES,
        metadata_bytes: SHIELDED_WITHDRAWAL_CIRCUIT_METADATA,
    },
];

fn shielded_withdrawal_proof_family(
    family_id: u32,
) -> &'static GeneratedShieldedWithdrawalProofFamily {
    GENERATED_SHIELDED_WITHDRAWAL_PROOF_FAMILIES
        .iter()
        .find(|family| family.id == family_id)
        .unwrap_or_else(|| panic!("unknown shielded withdrawal family id {family_id}"))
}

pub fn shielded_withdrawal_proof_verification_key(
    family_id: u32,
) -> &'static PreparedVerifyingKey<Bls12_377> {
    &**shielded_withdrawal_proof_family(family_id).verification_key
}

pub fn shielded_withdrawal_proving_key_bytes(family_id: u32) -> &'static [u8] {
    shielded_withdrawal_proof_family(family_id).proving_key_bytes
}

pub fn shielded_withdrawal_verifying_key_json_bytes(family_id: u32) -> &'static [u8] {
    shielded_withdrawal_proof_family(family_id).verifying_key_json_bytes
}

pub fn shielded_withdrawal_circuit_metadata(family_id: u32) -> &'static [u8] {
    shielded_withdrawal_proof_family(family_id).metadata_bytes
}
