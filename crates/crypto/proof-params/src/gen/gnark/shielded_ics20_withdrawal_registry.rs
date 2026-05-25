#[derive(Clone, Copy, Debug)]
struct GeneratedShieldedIcs20WithdrawalProofFamily {
    id: u32,
    verification_key: &'static Lazy<PreparedVerifyingKey<Bls12_377>>,
    proving_key_bytes: &'static [u8],
    metadata_bytes: &'static [u8],
}

static SHIELDED_ICS20_WITHDRAWAL_PROOF_VERIFICATION_KEY: Lazy<
    PreparedVerifyingKey<Bls12_377>,
> = Lazy::new(|| {
    if let Some(dir) = std::env::var_os("PENUMBRA_GNARK_SHIELDED_ICS20_WITHDRAWAL_ARTIFACT_DIR") {
        return load_verifying_key_json_artifact(Path::new(&dir), "shielded_ics20_withdrawal")
            .expect("can deserialize shielded_ics20_withdrawal VerifyingKey")
            .into();
    }
    load_verifying_key_json_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../tools/gnark/artifacts/shielded_ics20_withdrawal/verifying_key.json"
    )))
    .expect("bundled shielded_ics20_withdrawal VerifyingKey is valid")
    .into()
});

static SHIELDED_ICS20_WITHDRAWAL_PROOF_PROVING_KEY_BYTES: &[u8] = {
    #[cfg(feature = "bundled-proving-keys")]
    {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tools/gnark/artifacts/shielded_ics20_withdrawal/proving_key.bin"
        ))
    }
    #[cfg(not(feature = "bundled-proving-keys"))]
    {
        &[]
    }
};

static SHIELDED_ICS20_WITHDRAWAL_CIRCUIT_METADATA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/gnark/artifacts/shielded_ics20_withdrawal/circuit_metadata.json"
));

static GENERATED_SHIELDED_ICS20_WITHDRAWAL_PROOF_FAMILIES:
    &[GeneratedShieldedIcs20WithdrawalProofFamily] = &[GeneratedShieldedIcs20WithdrawalProofFamily {
    id: 1,
    verification_key: &SHIELDED_ICS20_WITHDRAWAL_PROOF_VERIFICATION_KEY,
    proving_key_bytes: SHIELDED_ICS20_WITHDRAWAL_PROOF_PROVING_KEY_BYTES,
    metadata_bytes: SHIELDED_ICS20_WITHDRAWAL_CIRCUIT_METADATA,
}];

fn shielded_ics20_withdrawal_proof_family(
    family_id: u32,
) -> &'static GeneratedShieldedIcs20WithdrawalProofFamily {
    GENERATED_SHIELDED_ICS20_WITHDRAWAL_PROOF_FAMILIES
        .iter()
        .find(|family| family.id == family_id)
        .unwrap_or_else(|| panic!("unknown shielded ICS-20 withdrawal family id {family_id}"))
}

pub fn shielded_ics20_withdrawal_proof_verification_key(
    family_id: u32,
) -> &'static PreparedVerifyingKey<Bls12_377> {
    &**shielded_ics20_withdrawal_proof_family(family_id).verification_key
}

pub fn shielded_ics20_withdrawal_proving_key_bytes(family_id: u32) -> &'static [u8] {
    shielded_ics20_withdrawal_proof_family(family_id).proving_key_bytes
}

pub fn shielded_ics20_withdrawal_circuit_metadata(family_id: u32) -> &'static [u8] {
    shielded_ics20_withdrawal_proof_family(family_id).metadata_bytes
}
