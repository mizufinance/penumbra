use anyhow::{anyhow, Result};
use decaf377::Fr;
use rand::{rngs::StdRng, SeedableRng};
use shieldd_sdk_asset::Value;
#[cfg(feature = "component")]
use shieldd_sdk_compliance::TRANSFER_WIRE_BYTES;
use shieldd_sdk_compliance::{
    derive_transfer_salt, encrypt_transfer, TransferComplianceCiphertext,
    TransferComplianceMetadata, TransferCompliancePublicInputs,
};

#[cfg(feature = "component")]
use super::TransferOutputBody;
use crate::{
    transfer::{
        TransferComplianceCiphertextPublic, TransferCompliancePrivate, TransferCompliancePublic,
        TransferTierRandomizers,
    },
    ShieldedOutputPlan,
};

pub(crate) const RECEIVER_OUTPUT_INDEX: usize = 0;
pub(crate) const CHANGE_OUTPUT_INDEX: usize = 1;

pub(crate) fn is_receiver_output_index(index: usize) -> bool {
    index == RECEIVER_OUTPUT_INDEX
}

pub(crate) fn is_change_output_index(index: usize) -> bool {
    index == CHANGE_OUTPUT_INDEX
}

fn transfer_compliance_rng_seed(transfer_nonce_root: Fr) -> [u8; 32] {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"pnxfer-cmprng")
        .hash(&transfer_nonce_root.to_bytes());
    let mut seed = [0u8; 32];
    seed.copy_from_slice(hash.as_bytes());
    seed
}

pub(crate) fn build_transfer_compliance(
    outputs: &[ShieldedOutputPlan],
    context: &crate::TransferContext,
    is_flagged: bool,
) -> Result<BuildTransferComplianceResult> {
    let sender_leaf = &context.witness.sender.leaf;
    let receiver_leaf = &context.recipient.leaf;
    let asset_policy = context.witness.policy.as_ref();
    let asset_indexed_leaf = &context.witness.asset.leaf;
    let target_timestamp = context.timestamp;
    let transfer_nonce_root = context.nonce;
    let receiver_output = outputs
        .get(RECEIVER_OUTPUT_INDEX)
        .ok_or_else(|| anyhow!("transfer requires at least one output"))?;
    let receiver_note = receiver_output.output_note(receiver_leaf.capk);
    let dk_pub = if context.witness.asset.is_regulated {
        asset_indexed_leaf.params.dk_pub
    } else {
        *shieldd_sdk_compliance::UNREGULATED_SINK_DK_PUB
    };

    let general_keys = if context.witness.asset.is_regulated {
        asset_indexed_leaf.ring.audit_keys.clone()
    } else {
        shieldd_sdk_compliance::AuditKeys::unregulated()
    };

    let detection_salt = derive_transfer_salt(transfer_nonce_root, b"detection");
    let sender_core_salt = derive_transfer_salt(transfer_nonce_root, b"sender_core");
    let sender_ext_salt = derive_transfer_salt(transfer_nonce_root, b"sender_ext");
    let output_core_salt = derive_transfer_salt(transfer_nonce_root, b"output_core");
    let output_ext_salt = derive_transfer_salt(transfer_nonce_root, b"output_ext");
    let mut rng = StdRng::from_seed(transfer_compliance_rng_seed(transfer_nonce_root));

    let encryption = encrypt_transfer(
        &mut rng,
        &general_keys,
        &sender_leaf.audit_keys,
        &receiver_leaf.audit_keys,
        &dk_pub,
        &receiver_note.address(),
        &sender_leaf.address,
        Value {
            amount: receiver_note.amount(),
            asset_id: receiver_note.asset_id(),
        },
        is_flagged,
        detection_salt,
        sender_core_salt,
        output_core_salt,
    )?;

    // A non-membership witness carries the predecessor leaf. None of that
    // unrelated predecessor's policy identifiers may enter the public
    // statement for an unregulated transfer.
    let asset_policy = if context.witness.asset.is_regulated {
        Some(asset_policy.ok_or_else(|| anyhow!("regulated transfer missing asset policy"))?)
    } else {
        None
    };
    let (ring_id, policy_id, resource, permission) = if let Some(asset_policy) = asset_policy {
        (
            asset_policy.ring.ring_id.as_str(),
            asset_policy.ring.policy_id.as_str(),
            asset_policy.ring.resource.as_str(),
            asset_policy.ring.permission.as_str(),
        )
    } else {
        ("", "", "", "")
    };
    let metadata = TransferComplianceMetadata::from_identifiers(
        ring_id,
        policy_id,
        resource,
        permission,
        target_timestamp,
        general_keys.epoch,
        sender_core_salt,
        sender_ext_salt,
        output_core_salt,
        output_ext_salt,
    );
    metadata.validate()?;

    let public = transfer_compliance_public_from_parts(&encryption.ciphertext, &metadata)?;
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
    };

    Ok(BuildTransferComplianceResult {
        ciphertext: encryption.ciphertext,
        metadata,
        public,
        private,
    })
}

pub(crate) struct BuildTransferComplianceResult {
    pub ciphertext: TransferComplianceCiphertext,
    pub metadata: TransferComplianceMetadata,
    pub public: TransferCompliancePublic,
    pub private: TransferCompliancePrivate,
}

pub(crate) struct TransferOutputComplianceBytes {
    pub compliance_ciphertext: Vec<u8>,
    pub compliance_metadata: Vec<u8>,
}

pub(crate) fn receiver_output_transfer_compliance(
    ciphertext: &TransferComplianceCiphertext,
    metadata: &TransferComplianceMetadata,
) -> Result<TransferOutputComplianceBytes> {
    Ok(TransferOutputComplianceBytes {
        compliance_ciphertext: ciphertext.to_bytes(),
        compliance_metadata: metadata.to_bytes()?,
    })
}

pub(crate) fn change_output_transfer_compliance() -> TransferOutputComplianceBytes {
    TransferOutputComplianceBytes {
        compliance_ciphertext: Vec::new(),
        compliance_metadata: Vec::new(),
    }
}

#[cfg(feature = "component")]
pub(crate) fn parse_transfer_output_compliance(
    outputs: &[TransferOutputBody],
) -> Result<(TransferComplianceCiphertext, TransferComplianceMetadata)> {
    let receiver_output = outputs
        .get(RECEIVER_OUTPUT_INDEX)
        .ok_or_else(|| anyhow!("transfer requires at least one output"))?;
    anyhow::ensure!(
        receiver_output.compliance_ciphertext.len() == TRANSFER_WIRE_BYTES,
        "receiver output transfer compliance ciphertext must be {TRANSFER_WIRE_BYTES} bytes, got {}",
        receiver_output.compliance_ciphertext.len()
    );
    for (index, output) in outputs.iter().enumerate().skip(CHANGE_OUTPUT_INDEX) {
        anyhow::ensure!(
            output.compliance_ciphertext.is_empty(),
            "change output {} transfer compliance ciphertext must be empty",
            index
        );
        anyhow::ensure!(
            output.compliance_metadata.is_empty(),
            "change output {} transfer compliance metadata must be empty",
            index
        );
    }
    let metadata = TransferComplianceMetadata::from_bytes(&receiver_output.compliance_metadata)?;
    Ok((
        TransferComplianceCiphertext::from_bytes(&receiver_output.compliance_ciphertext)?,
        metadata,
    ))
}

pub(crate) fn transfer_compliance_public_from_parts(
    ciphertext: &TransferComplianceCiphertext,
    metadata: &TransferComplianceMetadata,
) -> Result<TransferCompliancePublic> {
    metadata.validate()?;
    let TransferCompliancePublicInputs {
        sender_core_epk,
        sender_ext_epk,
        output_core_epk,
        output_ext_epk,
        sender_core_c2,
        sender_ext_c2,
        output_core_c2,
        output_ext_c2,
        master_wrappings,
        sender_core_key_confirmation,
        output_core_key_confirmation,
        detection_ciphertext,
        sender_core_ciphertext,
        sender_ext_ciphertext,
        output_core_ciphertext,
        output_ext_ciphertext,
    } = ciphertext.to_transfer_circuit_public_inputs();

    Ok(TransferCompliancePublic {
        detection_ciphertext: detection_ciphertext.to_vec(),
        metadata: metadata.clone(),
        master_wrappings,
        sender_core_key_confirmation,
        output_core_key_confirmation,
        sender_core: TransferComplianceCiphertextPublic {
            epk: sender_core_epk,
            c2: sender_core_c2,
            ciphertext: sender_core_ciphertext.to_vec(),
        },
        sender_ext: TransferComplianceCiphertextPublic {
            epk: sender_ext_epk,
            c2: sender_ext_c2,
            ciphertext: sender_ext_ciphertext.to_vec(),
        },
        output_core: TransferComplianceCiphertextPublic {
            epk: output_core_epk,
            c2: output_core_c2,
            ciphertext: output_core_ciphertext.to_vec(),
        },
        output_ext: TransferComplianceCiphertextPublic {
            epk: output_ext_epk,
            c2: output_ext_c2,
            ciphertext: output_ext_ciphertext.to_vec(),
        },
    })
}

#[cfg(test)]
mod tests {
    use decaf377::Fq;
    use rand::{rngs::StdRng, SeedableRng};
    use shieldd_sdk_asset::asset;

    #[test]
    fn unregulated_compliance_ignores_authenticated_predecessor_policy() {
        let asset_id = asset::REGISTRY.parse_unit("test_usd").id();
        let predecessor_asset_id = asset_id.0 - Fq::from(1u64);
        let mut first_rng = StdRng::seed_from_u64(0x554e_5245_4750_4f4c);
        let mut second_rng = StdRng::seed_from_u64(0x554e_5245_4750_4f4c);

        let (first_public, _first_private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_hidden_arity_roundtrip_inputs_for_asset_populated(
                &mut first_rng,
                asset_id,
                predecessor_asset_id,
                1,
                false,
            );
        let (second_public, _second_private) = crate::test_proof_helpers::proof_test_helpers::
            build_transfer_hidden_arity_roundtrip_inputs_for_asset_populated(
                &mut second_rng,
                asset_id,
                predecessor_asset_id,
                999_999,
                false,
            );

        // The authenticated registry root must change with its predecessor leaf,
        // but no unrelated predecessor policy may influence audit semantics.
        assert_ne!(first_public.asset_anchor, second_public.asset_anchor);
        assert_eq!(
            first_public.compliance.metadata,
            second_public.compliance.metadata
        );
        let empty_hash = shieldd_sdk_compliance::indexed_tree::string_to_fq("");
        let metadata = &first_public.compliance.metadata;
        assert_eq!(metadata.ring_id_hash().unwrap(), empty_hash);
        assert_eq!(metadata.policy_id_hash().unwrap(), empty_hash);
        assert_eq!(metadata.resource_hash().unwrap(), empty_hash);
        assert_eq!(metadata.permission_hash().unwrap(), empty_hash);
        assert_eq!(
            first_public.compliance.detection_ciphertext,
            second_public.compliance.detection_ciphertext
        );
        for (first, second) in [
            (
                &first_public.compliance.sender_core,
                &second_public.compliance.sender_core,
            ),
            (
                &first_public.compliance.sender_ext,
                &second_public.compliance.sender_ext,
            ),
            (
                &first_public.compliance.output_core,
                &second_public.compliance.output_core,
            ),
            (
                &first_public.compliance.output_ext,
                &second_public.compliance.output_ext,
            ),
        ] {
            assert_eq!(first.epk, second.epk);
            assert_eq!(first.c2, second.c2);
            assert_eq!(first.ciphertext, second.ciphertext);
        }
    }
}
