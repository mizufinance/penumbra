use anyhow::{anyhow, Result};
use decaf377::Fr;
use penumbra_sdk_asset::Value;
#[cfg(feature = "component")]
use penumbra_sdk_compliance::TRANSFER_WIRE_BYTES;
use penumbra_sdk_compliance::{
    build_orbis_encrypted_seed_upload_package_with_randomness, derive_transfer_salt,
    encrypt_transfer, AssetPolicy, IndexedLeaf, TransferComplianceCiphertext,
    TransferCompliancePublicInputs, TransferOrbisUploadBundle, TransferTierKind,
    TransferTierMetadataStatement,
};
use rand::{rngs::StdRng, SeedableRng};

#[cfg(feature = "component")]
use super::TransferOutputBody;
use crate::{
    transfer::{
        TransferComplianceCiphertextPublic, TransferCompliancePrivate,
        TransferComplianceProofPublic, TransferCompliancePublic, TransferTierRandomizers,
    },
    ShieldedOutputPlan,
};

fn transfer_compliance_rng_seed(transfer_nonce_root: Fr) -> [u8; 32] {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"pnxfer-cmprng-v1")
        .hash(&transfer_nonce_root.to_bytes());
    let mut seed = [0u8; 32];
    seed.copy_from_slice(hash.as_bytes());
    seed
}

fn transfer_orbis_upload_rng_seed(transfer_nonce_root: Fr) -> [u8; 32] {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"pnxfer-orbis-v1")
        .hash(&transfer_nonce_root.to_bytes());
    let mut seed = [0u8; 32];
    seed.copy_from_slice(hash.as_bytes());
    seed
}

pub(crate) fn build_transfer_compliance(
    outputs: &[ShieldedOutputPlan],
    sender_leaf: &penumbra_sdk_compliance::ComplianceLeaf,
    asset_policy: &AssetPolicy,
    asset_indexed_leaf: &IndexedLeaf,
    target_timestamp: u64,
    transfer_nonce_root: Fr,
) -> Result<BuildTransferComplianceResult> {
    // Transfer compliance always describes output 0, the external receiver leg.
    // Output 1, when present, is sender-owned change and contributes to balance
    // correctness but not to compliance plaintext construction.
    let receiver_output = outputs
        .first()
        .ok_or_else(|| anyhow!("transfer requires at least one output"))?;
    let receiver_note = receiver_output.output_note();
    let receiver_leaf = receiver_output
        .compliance_leaf
        .clone()
        .ok_or_else(|| anyhow!("receiver output missing compliance leaf"))?;

    let ring_pk = if receiver_output.is_regulated {
        asset_indexed_leaf.ring.ring_pk
    } else {
        *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK
    };
    let dk_pub = if receiver_output.is_regulated {
        asset_indexed_leaf.params.dk_pub
    } else {
        *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB
    };

    let receiver_amount: u128 = receiver_note.amount().into();
    let is_flagged = receiver_amount >= asset_indexed_leaf.params.threshold;

    let sender_ack = ring_pk * Fr::from_le_bytes_mod_order(&sender_leaf.d.to_bytes());
    let receiver_ack = ring_pk * Fr::from_le_bytes_mod_order(&receiver_leaf.d.to_bytes());

    let detection_salt = derive_transfer_salt(transfer_nonce_root, b"detection");
    let sender_core_salt = derive_transfer_salt(transfer_nonce_root, b"sender_core");
    let sender_ext_salt = derive_transfer_salt(transfer_nonce_root, b"sender_ext");
    let output_core_salt = derive_transfer_salt(transfer_nonce_root, b"output_core");
    let output_ext_salt = derive_transfer_salt(transfer_nonce_root, b"output_ext");
    let mut rng = StdRng::from_seed(transfer_compliance_rng_seed(transfer_nonce_root));

    let encryption = encrypt_transfer(
        &mut rng,
        &sender_ack,
        &receiver_ack,
        &dk_pub,
        &receiver_note.address(),
        &sender_leaf.address,
        Value {
            amount: receiver_note.amount(),
            asset_id: receiver_note.asset_id(),
        },
        is_flagged,
        detection_salt,
    )?;

    let sender_b_d = sender_leaf
        .address
        .diversified_generator()
        .vartime_compress_to_field();
    let receiver_b_d = receiver_note
        .address()
        .diversified_generator()
        .vartime_compress_to_field();
    let policy_id = &asset_policy.ring.policy_id;
    let ring_id = &asset_policy.ring.ring_id;
    let resource = &asset_policy.ring.resource;
    let permission = &asset_policy.ring.permission;
    let mut upload_rng = StdRng::from_seed(transfer_orbis_upload_rng_seed(transfer_nonce_root));
    let bundle = TransferOrbisUploadBundle {
        sender_core: build_orbis_encrypted_seed_upload_package_with_randomness(
            &mut upload_rng,
            &ring_pk,
            encryption.sender.core.seed,
            encryption.sender.core.r,
            TransferTierMetadataStatement::from_identifiers(
                sender_b_d,
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::SenderCore,
                target_timestamp,
                sender_core_salt,
            ),
            ring_id,
            policy_id,
            resource,
            permission,
            TransferTierKind::SenderCore,
            target_timestamp,
            sender_core_salt,
        )?,
        sender_ext: build_orbis_encrypted_seed_upload_package_with_randomness(
            &mut upload_rng,
            &ring_pk,
            encryption.sender.ext.seed,
            encryption.sender.ext.r,
            TransferTierMetadataStatement::from_identifiers(
                sender_b_d,
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::SenderExt,
                target_timestamp,
                sender_ext_salt,
            ),
            ring_id,
            policy_id,
            resource,
            permission,
            TransferTierKind::SenderExt,
            target_timestamp,
            sender_ext_salt,
        )?,
        output_core: build_orbis_encrypted_seed_upload_package_with_randomness(
            &mut upload_rng,
            &ring_pk,
            encryption.output.core.seed,
            encryption.output.core.r,
            TransferTierMetadataStatement::from_identifiers(
                receiver_b_d,
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::OutputCore,
                target_timestamp,
                output_core_salt,
            ),
            ring_id,
            policy_id,
            resource,
            permission,
            TransferTierKind::OutputCore,
            target_timestamp,
            output_core_salt,
        )?,
        output_ext: build_orbis_encrypted_seed_upload_package_with_randomness(
            &mut upload_rng,
            &ring_pk,
            encryption.output.ext.seed,
            encryption.output.ext.r,
            TransferTierMetadataStatement::from_identifiers(
                receiver_b_d,
                ring_id,
                policy_id,
                resource,
                permission,
                TransferTierKind::OutputExt,
                target_timestamp,
                output_ext_salt,
            ),
            ring_id,
            policy_id,
            resource,
            permission,
            TransferTierKind::OutputExt,
            target_timestamp,
            output_ext_salt,
        )?,
    };

    let public = transfer_compliance_public_from_parts(&encryption.ciphertext, &bundle)?;
    let private = TransferCompliancePrivate {
        transfer_nonce_root,
        sender: TransferTierRandomizers {
            core: encryption.sender.core.r,
            ext: encryption.sender.ext.r,
        },
        output: TransferTierRandomizers {
            core: encryption.output.core.r,
            ext: encryption.output.ext.r,
        },
        is_flagged,
    };

    Ok(BuildTransferComplianceResult {
        ciphertext: encryption.ciphertext,
        bundle,
        public,
        private,
    })
}

pub(crate) struct BuildTransferComplianceResult {
    pub ciphertext: TransferComplianceCiphertext,
    pub bundle: TransferOrbisUploadBundle,
    pub public: TransferCompliancePublic,
    pub private: TransferCompliancePrivate,
}

pub(crate) struct TransferOutputComplianceBytes {
    pub compliance_ciphertext: Vec<u8>,
    pub orbis_upload_bundle: Vec<u8>,
}

pub(crate) fn receiver_output_transfer_compliance(
    ciphertext: &TransferComplianceCiphertext,
    bundle: &TransferOrbisUploadBundle,
) -> TransferOutputComplianceBytes {
    TransferOutputComplianceBytes {
        compliance_ciphertext: ciphertext.to_bytes(),
        orbis_upload_bundle: bundle
            .to_bytes()
            .expect("Orbis upload bundle should serialize"),
    }
}

pub(crate) fn change_output_transfer_compliance() -> TransferOutputComplianceBytes {
    TransferOutputComplianceBytes {
        compliance_ciphertext: Vec::new(),
        orbis_upload_bundle: Vec::new(),
    }
}

#[cfg(feature = "component")]
pub(crate) fn parse_transfer_output_compliance(
    outputs: &[TransferOutputBody],
) -> Result<(TransferComplianceCiphertext, TransferOrbisUploadBundle)> {
    // Output 0 carries the receiver-leg compliance bundle. Output 1, when
    // present, is sender-owned change and must not carry transfer compliance bytes.
    let receiver_output = outputs
        .first()
        .ok_or_else(|| anyhow!("transfer requires at least one output"))?;
    anyhow::ensure!(
        receiver_output.compliance_ciphertext.len() == TRANSFER_WIRE_BYTES,
        "receiver output transfer compliance ciphertext must be {TRANSFER_WIRE_BYTES} bytes, got {}",
        receiver_output.compliance_ciphertext.len()
    );
    for (index, output) in outputs.iter().enumerate().skip(1) {
        anyhow::ensure!(
            output.compliance_ciphertext.is_empty(),
            "change output {} transfer compliance ciphertext must be empty",
            index
        );
        anyhow::ensure!(
            output.orbis_upload_bundle.is_empty(),
            "change output {} Orbis upload bundle must be empty",
            index
        );
    }
    let bundle = TransferOrbisUploadBundle::from_bytes(&receiver_output.orbis_upload_bundle)?;
    bundle.validate()?;
    Ok((
        TransferComplianceCiphertext::from_bytes(&receiver_output.compliance_ciphertext)?,
        bundle,
    ))
}

pub(crate) fn transfer_compliance_public_from_parts(
    ciphertext: &TransferComplianceCiphertext,
    bundle: &TransferOrbisUploadBundle,
) -> Result<TransferCompliancePublic> {
    let TransferCompliancePublicInputs {
        sender_core_epk,
        sender_ext_epk,
        output_core_epk,
        output_ext_epk,
        sender_core_c2,
        sender_ext_c2,
        output_core_c2,
        output_ext_c2,
        detection_ciphertext,
        sender_core_ciphertext,
        sender_ext_ciphertext,
        output_core_ciphertext,
        output_ext_ciphertext,
    } = ciphertext.to_transfer_circuit_public_inputs();

    Ok(TransferCompliancePublic {
        detection_ciphertext: detection_ciphertext.to_vec(),
        sender_core: TransferComplianceCiphertextPublic {
            epk: sender_core_epk,
            c2: sender_core_c2,
            ciphertext: sender_core_ciphertext.to_vec(),
            proof: TransferComplianceProofPublic::try_from_package(&bundle.sender_core)?,
        },
        sender_ext: TransferComplianceCiphertextPublic {
            epk: sender_ext_epk,
            c2: sender_ext_c2,
            ciphertext: sender_ext_ciphertext.to_vec(),
            proof: TransferComplianceProofPublic::try_from_package(&bundle.sender_ext)?,
        },
        output_core: TransferComplianceCiphertextPublic {
            epk: output_core_epk,
            c2: output_core_c2,
            ciphertext: output_core_ciphertext.to_vec(),
            proof: TransferComplianceProofPublic::try_from_package(&bundle.output_core)?,
        },
        output_ext: TransferComplianceCiphertextPublic {
            epk: output_ext_epk,
            c2: output_ext_c2,
            ciphertext: output_ext_ciphertext.to_vec(),
            proof: TransferComplianceProofPublic::try_from_package(&bundle.output_ext)?,
        },
    })
}
