use anyhow::{anyhow, Context, Result};
use decaf377::{Encoding, Fq};
use penumbra_sdk_compliance::TransferTierMetadataStatement;

use crate::{
    gnark::typed::{
        compliance_leaf_from_typed, indexed_leaf_from_typed, merkle_path_from_typed,
        point_affine_bytes, point_affine_bytes_with_fallback, ComplianceLeafBinary,
        IndexedLeafBinary, MerklePathBinary, PointAffineBytes,
    },
    public_input_hash::{transfer_statement_fields, transfer_statement_hash_from_public},
    transfer::{
        transfer_input_count, transfer_output_count, TransferComplianceCiphertextPublic,
        TransferProofPrivate, TransferProofPublic, TRANSFER_PROOF_LABEL,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferSpendWitnessV1 {
    pub nullifier: [u8; 32],
    pub spent_note_blinding: [u8; 32],
    pub spent_note_amount: [u8; 32],
    pub spent_note_asset_id: [u8; 32],
    pub spent_transmission_key: [u8; 32],
    pub spent_clue_key: [u8; 32],
    pub state_commitment_commitment: [u8; 32],
    pub state_commitment_position: u64,
    pub state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    pub spend_auth_randomizer: [u8; 32],
    pub is_dummy: bool,
    pub dummy_nullifier_seed: [u8; 32],
    pub dummy_spend_auth_key: [u8; 32],
    pub rk_affine: PointAffineBytes,
    pub spent_diversified_generator_affine: PointAffineBytes,
    pub spent_transmission_key_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferOutputWitnessV1 {
    pub note_commitment: [u8; 32],
    pub created_note_blinding: [u8; 32],
    pub created_note_amount: [u8; 32],
    pub created_note_asset_id: [u8; 32],
    pub created_transmission_key: [u8; 32],
    pub created_clue_key: [u8; 32],
    pub recipient_compliance_path: MerklePathBinary,
    pub recipient_compliance_position: u64,
    pub recipient_asset_id: [u8; 32],
    pub recipient_d: [u8; 32],
    /// Output 0 is the receiver leg. Output 1, when present, is sender-owned change.
    pub is_receiver: bool,
    pub created_diversified_generator_affine: PointAffineBytes,
    pub created_transmission_key_affine: PointAffineBytes,
    pub recipient_diversified_generator_affine: PointAffineBytes,
    pub recipient_transmission_key_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferComplianceCiphertextWitnessV1 {
    pub c2: [u8; 32],
    pub ciphertext: Vec<[u8; 32]>,
    pub subject_b_d: [u8; 32],
    pub ring_id_hash: [u8; 32],
    pub policy_id_hash: [u8; 32],
    pub resource_hash: [u8; 32],
    pub permission_hash: [u8; 32],
    pub tier: u64,
    pub statement_target_timestamp: [u8; 32],
    pub salt: [u8; 32],
    pub challenge: [u8; 32],
    pub response: [u8; 32],
    pub epk_affine: PointAffineBytes,
    pub derived_pk_affine: PointAffineBytes,
    pub enc_cmt_affine: PointAffineBytes,
    pub shared_point_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferTierRandomizersWitnessV1 {
    pub core: [u8; 32],
    pub ext: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferWitnessV1 {
    pub total_length: u32,
    pub n_in: u32,
    pub n_out: u32,
    pub anchor: [u8; 32],
    pub balance_commitment: [u8; 32],
    pub asset_anchor: [u8; 32],
    pub compliance_anchor: [u8; 32],
    pub target_timestamp: [u8; 32],
    pub claimed_statement_hash: [u8; 32],
    pub statement_fields: Vec<[u8; 32]>,
    pub action_balance_blinding: [u8; 32],
    pub ak: [u8; 32],
    pub nk: [u8; 32],
    pub asset_path: MerklePathBinary,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeafBinary,
    pub is_regulated: bool,
    pub sender_compliance_path: MerklePathBinary,
    pub sender_compliance_position: u64,
    pub sender_asset_id: [u8; 32],
    pub sender_d: [u8; 32],
    pub transfer_nonce_root: [u8; 32],
    pub detection_ciphertext: Vec<[u8; 32]>,
    pub sender_core: TransferComplianceCiphertextWitnessV1,
    pub sender_ext: TransferComplianceCiphertextWitnessV1,
    pub output_core: TransferComplianceCiphertextWitnessV1,
    pub output_ext: TransferComplianceCiphertextWitnessV1,
    pub sender_randomizers: TransferTierRandomizersWitnessV1,
    pub output_randomizers: TransferTierRandomizersWitnessV1,
    pub spends: Vec<TransferSpendWitnessV1>,
    pub outputs: Vec<TransferOutputWitnessV1>,
    pub balance_commitment_affine: PointAffineBytes,
    pub ak_affine: PointAffineBytes,
    pub asset_indexed_leaf_dk_pub_affine: PointAffineBytes,
    pub asset_indexed_leaf_ring_pk_affine: PointAffineBytes,
    pub sender_diversified_generator_affine: PointAffineBytes,
    pub sender_transmission_key_affine: PointAffineBytes,
}

fn compliance_leaf_parts(leaf: &ComplianceLeafBinary) -> ([u8; 80], [u8; 32], [u8; 32]) {
    (leaf.address, leaf.asset_id, leaf.d)
}

fn verification_key_point(
    vk: decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth>,
    label: &str,
) -> Result<decaf377::Element> {
    Encoding(vk.to_bytes())
        .vartime_decompress()
        .map_err(|e| anyhow!("decompress {label}: {e:?}"))
}

fn compliance_tier_witness(
    tier: &TransferComplianceCiphertextPublic,
) -> Result<TransferComplianceCiphertextWitnessV1> {
    let statement: &TransferTierMetadataStatement = &tier.proof.statement;
    Ok(TransferComplianceCiphertextWitnessV1 {
        c2: tier.c2.to_bytes(),
        ciphertext: tier
            .ciphertext
            .iter()
            .map(|value| value.to_bytes())
            .collect(),
        subject_b_d: statement.subject_b_d_bytes,
        ring_id_hash: statement.ring_id_hash_bytes,
        policy_id_hash: statement.policy_id_hash_bytes,
        resource_hash: statement.resource_hash_bytes,
        permission_hash: statement.permission_hash_bytes,
        tier: statement.tier.as_u64(),
        statement_target_timestamp: Fq::from(statement.target_timestamp).to_bytes(),
        salt: statement.salt_bytes,
        challenge: tier.proof.challenge.to_bytes(),
        response: tier.proof.response.to_bytes(),
        epk_affine: point_affine_bytes(tier.epk)?,
        derived_pk_affine: point_affine_bytes(tier.proof.derived_pk)?,
        enc_cmt_affine: point_affine_bytes(tier.proof.enc_cmt)?,
        shared_point_affine: point_affine_bytes(tier.proof.shared_point)?,
    })
}

impl TransferWitnessV1 {
    pub fn from_public_private(
        public: &TransferProofPublic,
        private: &TransferProofPrivate,
    ) -> Result<Self> {
        public.validate_shape()?;
        private.validate_shape()?;

        let claimed_statement_hash = transfer_statement_hash_from_public(public)
            .with_context(|| format!("compute {TRANSFER_PROOF_LABEL} statement hash"))?;
        let statement_fields = transfer_statement_fields(public)
            .with_context(|| format!("compute {TRANSFER_PROOF_LABEL} statement fields"))?;

        let sender_leaf = compliance_leaf_from_typed(&private.sender_leaf)?;
        let (_, sender_asset_id, sender_d) = compliance_leaf_parts(&sender_leaf);

        let spends = public
            .inputs
            .iter()
            .zip(private.inputs.iter())
            .enumerate()
            .map(|(index, (public_input, private_input))| {
                let state_commitment_auth_path = private_input
                    .state_commitment_proof
                    .auth_path()
                    .iter()
                    .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
                    .collect::<Vec<_>>();
                Ok(TransferSpendWitnessV1 {
                    nullifier: public_input.nullifier.0.to_bytes(),
                    spent_note_blinding: private_input.spent_note.note_blinding().to_bytes(),
                    spent_note_amount: Fq::from(private_input.spent_note.value().amount).to_bytes(),
                    spent_note_asset_id: private_input.spent_note.asset_id().0.to_bytes(),
                    spent_transmission_key: private_input.spent_note.transmission_key().0,
                    spent_clue_key: Fq::from_le_bytes_mod_order(
                        &private_input.spent_note.clue_key().0,
                    )
                    .to_bytes(),
                    state_commitment_commitment: private_input
                        .state_commitment_proof
                        .commitment()
                        .0
                        .to_bytes(),
                    state_commitment_position: u64::from(
                        private_input.state_commitment_proof.position(),
                    ),
                    state_commitment_auth_path,
                    spend_auth_randomizer: private_input.spend_auth_randomizer.to_bytes(),
                    is_dummy: private_input.is_dummy,
                    dummy_nullifier_seed: private_input.dummy_nullifier_seed.to_bytes(),
                    dummy_spend_auth_key: private_input.dummy_spend_auth_key.to_bytes(),
                    rk_affine: point_affine_bytes(verification_key_point(
                        public_input.rk,
                        &format!("rk_{index}"),
                    )?)?,
                    spent_diversified_generator_affine: point_affine_bytes(
                        private_input.spent_note.diversified_generator(),
                    )?,
                    spent_transmission_key_affine: point_affine_bytes(
                        Encoding(private_input.spent_note.transmission_key().0)
                            .vartime_decompress()
                            .map_err(|e| {
                                anyhow!("decompress spent transmission key {index}: {e:?}")
                            })?,
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let outputs = public
            .outputs
            .iter()
            .zip(private.outputs.iter())
            .enumerate()
            .map(|(index, (public_output, private_output))| {
                let recipient_leaf = compliance_leaf_from_typed(&private_output.recipient_leaf)?;
                let (_, recipient_asset_id, recipient_d) = compliance_leaf_parts(&recipient_leaf);
                Ok(TransferOutputWitnessV1 {
                    note_commitment: public_output.note_commitment.0.to_bytes(),
                    created_note_blinding: private_output.created_note.note_blinding().to_bytes(),
                    created_note_amount: Fq::from(private_output.created_note.value().amount)
                        .to_bytes(),
                    created_note_asset_id: private_output.created_note.asset_id().0.to_bytes(),
                    created_transmission_key: private_output.created_note.transmission_key().0,
                    created_clue_key: Fq::from_le_bytes_mod_order(
                        &private_output.created_note.clue_key().0,
                    )
                    .to_bytes(),
                    recipient_compliance_path: merkle_path_from_typed(
                        &private_output.recipient_compliance_path,
                    )?,
                    recipient_compliance_position: private_output.recipient_compliance_position,
                    recipient_asset_id,
                    recipient_d,
                    is_receiver: private_output.is_receiver,
                    created_diversified_generator_affine: point_affine_bytes(
                        private_output.created_note.diversified_generator(),
                    )?,
                    created_transmission_key_affine: point_affine_bytes(
                        Encoding(private_output.created_note.transmission_key().0)
                            .vartime_decompress()
                            .map_err(|e| {
                                anyhow!("decompress created transmission key {index}: {e:?}")
                            })?,
                    )?,
                    recipient_diversified_generator_affine: point_affine_bytes(
                        *private_output
                            .recipient_leaf
                            .address
                            .diversified_generator(),
                    )?,
                    recipient_transmission_key_affine: point_affine_bytes(
                        Encoding(private_output.recipient_leaf.address.transmission_key().0)
                            .vartime_decompress()
                            .map_err(|e| {
                                anyhow!("decompress recipient transmission key {index}: {e:?}")
                            })?,
                    )?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let mut witness = Self {
            total_length: 0,
            n_in: transfer_input_count() as u32,
            n_out: transfer_output_count() as u32,
            anchor: Fq::from(public.anchor).to_bytes(),
            balance_commitment: public.balance_commitment.to_bytes(),
            asset_anchor: public.asset_anchor.0.to_bytes(),
            compliance_anchor: public.compliance_anchor.0.to_bytes(),
            target_timestamp: public.target_timestamp.to_bytes(),
            claimed_statement_hash: claimed_statement_hash.to_bytes(),
            statement_fields: statement_fields
                .iter()
                .map(|value| value.to_bytes())
                .collect(),
            action_balance_blinding: private.action_balance_blinding.to_bytes(),
            ak: private.ak.to_bytes(),
            nk: private.nk.0.to_bytes(),
            asset_path: merkle_path_from_typed(&private.asset_path)?,
            asset_position: private.asset_position,
            asset_indexed_leaf: indexed_leaf_from_typed(&private.asset_indexed_leaf),
            is_regulated: private.is_regulated,
            sender_compliance_path: merkle_path_from_typed(&private.sender_compliance_path)?,
            sender_compliance_position: private.sender_compliance_position,
            sender_asset_id,
            sender_d,
            transfer_nonce_root: private.compliance.transfer_nonce_root.to_bytes(),
            detection_ciphertext: public
                .compliance
                .detection_ciphertext
                .iter()
                .map(|value| value.to_bytes())
                .collect(),
            sender_core: compliance_tier_witness(&public.compliance.sender_core)?,
            sender_ext: compliance_tier_witness(&public.compliance.sender_ext)?,
            output_core: compliance_tier_witness(&public.compliance.output_core)?,
            output_ext: compliance_tier_witness(&public.compliance.output_ext)?,
            sender_randomizers: TransferTierRandomizersWitnessV1 {
                core: private.compliance.sender.core.to_bytes(),
                ext: private.compliance.sender.ext.to_bytes(),
            },
            output_randomizers: TransferTierRandomizersWitnessV1 {
                core: private.compliance.output.core.to_bytes(),
                ext: private.compliance.output.ext.to_bytes(),
            },
            spends,
            outputs,
            balance_commitment_affine: point_affine_bytes(public.balance_commitment.0)?,
            ak_affine: point_affine_bytes(verification_key_point(private.ak, "ak")?)?,
            asset_indexed_leaf_dk_pub_affine: point_affine_bytes_with_fallback(
                private.asset_indexed_leaf.params.dk_pub,
                *penumbra_sdk_compliance::UNREGULATED_SINK_DK_PUB,
            )?,
            asset_indexed_leaf_ring_pk_affine: point_affine_bytes_with_fallback(
                private.asset_indexed_leaf.ring.ring_pk,
                *penumbra_sdk_compliance::UNREGULATED_SINK_RING_PK,
            )?,
            sender_diversified_generator_affine: point_affine_bytes(
                *private.sender_leaf.address.diversified_generator(),
            )?,
            sender_transmission_key_affine: point_affine_bytes(
                Encoding(private.sender_leaf.address.transmission_key().0)
                    .vartime_decompress()
                    .map_err(|e| anyhow!("decompress sender transmission key: {e:?}"))?,
            )?,
        };
        witness.total_length = u32::try_from(witness.encode()?.len())
            .map_err(|_| anyhow!("encoded {TRANSFER_PROOF_LABEL} witness exceeds u32"))?;
        Ok(witness)
    }
}
