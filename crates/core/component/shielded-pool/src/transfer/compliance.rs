use anyhow::{anyhow, ensure, Result};
use decaf377::{Fq, Fr};
use penumbra_sdk_asset::Value;
use penumbra_sdk_compliance::{
    compute_transfer_dleqs, derive_transfer_salt, encrypt_transfer, IndexedLeaf,
    TransferComplianceCiphertext, TransferComplianceDleqProofs, TransferCompliancePublicInputs,
    TRANSFER_DLEQ_BYTES, TRANSFER_WIRE_BYTES,
};
use rand::{rngs::StdRng, SeedableRng};

use super::TransferOutputBody;
use crate::{
    transfer::{
        TransferComplianceCiphertextPublic, TransferComplianceDleqPublic,
        TransferCompliancePrivate, TransferCompliancePublic,
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

pub(crate) fn build_transfer_compliance(
    outputs: &[ShieldedOutputPlan],
    sender_leaf: &penumbra_sdk_compliance::ComplianceLeaf,
    asset_indexed_leaf: &IndexedLeaf,
    target_timestamp: u64,
    transfer_nonce_root: Fr,
) -> Result<(
    TransferComplianceCiphertext,
    TransferComplianceDleqProofs,
    TransferCompliancePublic,
    TransferCompliancePrivate,
)> {
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

    let sender_k_core = Fr::rand(&mut rng);
    let sender_k_ext = Fr::rand(&mut rng);
    let output_k_core = Fr::rand(&mut rng);
    let output_k_ext = Fr::rand(&mut rng);

    let dleqs = compute_transfer_dleqs(
        encryption.sender_r_core,
        encryption.sender_r_ext,
        encryption.output_r_core,
        encryption.output_r_ext,
        sender_k_core,
        sender_k_ext,
        output_k_core,
        output_k_ext,
        &sender_ack,
        &receiver_ack,
        asset_indexed_leaf.ring.policy_id_hash,
        asset_indexed_leaf.ring.resource_hash,
        asset_indexed_leaf.ring.permission_hash,
        sender_core_salt,
        sender_ext_salt,
        output_core_salt,
        output_ext_salt,
        target_timestamp,
    );

    let public = transfer_compliance_public_from_parts(&encryption.ciphertext, &dleqs);
    let private = TransferCompliancePrivate {
        transfer_nonce_root,
        sender_r_core: encryption.sender_r_core,
        sender_r_ext: encryption.sender_r_ext,
        output_r_core: encryption.output_r_core,
        output_r_ext: encryption.output_r_ext,
        is_flagged,
    };

    Ok((encryption.ciphertext, dleqs, public, private))
}

pub(crate) fn receiver_output_transfer_compliance(
    ciphertext: &TransferComplianceCiphertext,
    dleqs: &TransferComplianceDleqProofs,
) -> (Vec<u8>, Vec<u8>) {
    (ciphertext.to_bytes(), dleqs.to_bytes())
}

pub(crate) fn change_output_transfer_compliance() -> (Vec<u8>, Vec<u8>) {
    (Vec::new(), Vec::new())
}

pub(crate) fn parse_transfer_output_compliance(
    outputs: &[TransferOutputBody],
) -> Result<(TransferComplianceCiphertext, TransferComplianceDleqProofs)> {
    // Output 0 carries the receiver-leg compliance bundle. Output 1, when
    // present, is sender-owned change and must not carry transfer compliance bytes.
    let receiver_output = outputs
        .first()
        .ok_or_else(|| anyhow!("transfer requires at least one output"))?;
    ensure!(
        receiver_output.compliance_ciphertext.len() == TRANSFER_WIRE_BYTES,
        "receiver output transfer compliance ciphertext must be {TRANSFER_WIRE_BYTES} bytes, got {}",
        receiver_output.compliance_ciphertext.len()
    );
    ensure!(
        receiver_output.dleq_proofs.len() == TRANSFER_DLEQ_BYTES,
        "receiver output transfer DLEQ bundle must be {TRANSFER_DLEQ_BYTES} bytes, got {}",
        receiver_output.dleq_proofs.len()
    );
    for (index, output) in outputs.iter().enumerate().skip(1) {
        ensure!(
            output.compliance_ciphertext.is_empty(),
            "change output {} transfer compliance ciphertext must be empty",
            index
        );
        ensure!(
            output.dleq_proofs.is_empty(),
            "change output {} transfer DLEQ proofs must be empty",
            index
        );
    }
    Ok((
        TransferComplianceCiphertext::from_bytes(&receiver_output.compliance_ciphertext)?,
        TransferComplianceDleqProofs::from_bytes(&receiver_output.dleq_proofs)?,
    ))
}

pub(crate) fn transfer_compliance_public_from_parts(
    ciphertext: &TransferComplianceCiphertext,
    dleqs: &TransferComplianceDleqProofs,
) -> TransferCompliancePublic {
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

    TransferCompliancePublic {
        detection_ciphertext: detection_ciphertext.to_vec(),
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
        sender_core_dleq: TransferComplianceDleqPublic {
            c: dleqs.sender_core.c,
            s: Fq::from_le_bytes_mod_order(&dleqs.sender_core.s.to_bytes()),
        },
        sender_ext_dleq: TransferComplianceDleqPublic {
            c: dleqs.sender_ext.c,
            s: Fq::from_le_bytes_mod_order(&dleqs.sender_ext.s.to_bytes()),
        },
        output_core_dleq: TransferComplianceDleqPublic {
            c: dleqs.output_core.c,
            s: Fq::from_le_bytes_mod_order(&dleqs.output_core.s.to_bytes()),
        },
        output_ext_dleq: TransferComplianceDleqPublic {
            c: dleqs.output_ext.c,
            s: Fq::from_le_bytes_mod_order(&dleqs.output_ext.s.to_bytes()),
        },
    }
}
