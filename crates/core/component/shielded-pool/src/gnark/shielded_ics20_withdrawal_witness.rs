use anyhow::{anyhow, bail, Result};
use decaf377::{Encoding, Fq};

use crate::{
    gnark::typed::{
        compliance_leaf_from_typed, indexed_leaf_from_typed, merkle_path_from_typed,
        point_affine_bytes, point_affine_bytes_with_fallback, ComplianceLeafBinary,
        IndexedLeafBinary, MerklePathBinary, PointAffineBytes,
    },
    public_input_hash::{
        shielded_ics20_withdrawal_statement_fields,
        shielded_ics20_withdrawal_statement_hash_from_public,
    },
    shielded_ics20_withdrawal::{
        ShieldedIcs20WithdrawalChangePrivate, ShieldedIcs20WithdrawalChangePublic,
        ShieldedIcs20WithdrawalInputPrivate, ShieldedIcs20WithdrawalInputPublic,
        ShieldedIcs20WithdrawalProofPrivate, ShieldedIcs20WithdrawalProofPublic,
    },
    ShieldedIcs20WithdrawalFamilyId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShieldedIcs20WithdrawalSpendWitnessV1 {
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
pub struct ShieldedIcs20WithdrawalChangeWitnessV1 {
    pub note_commitment: [u8; 32],
    pub created_note_blinding: [u8; 32],
    pub created_note_amount: [u8; 32],
    pub created_note_asset_id: [u8; 32],
    pub created_transmission_key: [u8; 32],
    pub created_clue_key: [u8; 32],
    pub created_diversified_generator_affine: PointAffineBytes,
    pub created_transmission_key_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShieldedIcs20WithdrawalWitnessV1 {
    pub family_id: ShieldedIcs20WithdrawalFamilyId,
    pub total_length: u32,
    pub n_in: u32,
    pub anchor: [u8; 32],
    pub balance_commitment: [u8; 32],
    pub asset_anchor: [u8; 32],
    pub compliance_anchor: [u8; 32],
    pub target_timestamp: [u8; 32],
    pub outbound_asset_id: [u8; 32],
    pub outbound_amount: [u8; 32],
    pub withdrawal_effect_hash_lo: [u8; 32],
    pub withdrawal_effect_hash_hi: [u8; 32],
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
    pub sender_slot_id: [u8; 32],
    pub sender_slot_derivation: [u8; 32],
    pub sender_d: [u8; 32],
    pub spends: Vec<ShieldedIcs20WithdrawalSpendWitnessV1>,
    pub change_output: ShieldedIcs20WithdrawalChangeWitnessV1,
    pub balance_commitment_affine: PointAffineBytes,
    pub ak_affine: PointAffineBytes,
    pub asset_indexed_leaf_dk_pub_affine: PointAffineBytes,
    pub asset_indexed_leaf_ring_pk_affine: PointAffineBytes,
    pub sender_diversified_generator_affine: PointAffineBytes,
    pub sender_transmission_key_affine: PointAffineBytes,
}

fn compliance_leaf_parts(
    leaf: &ComplianceLeafBinary,
) -> ([u8; 80], [u8; 32], [u8; 32], [u8; 32], [u8; 32]) {
    (
        leaf.address,
        leaf.asset_id,
        leaf.slot_id,
        leaf.slot_derivation,
        leaf.d,
    )
}

fn verification_key_point(
    vk: decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth>,
    label: &str,
) -> Result<decaf377::Element> {
    Encoding(vk.to_bytes())
        .vartime_decompress()
        .map_err(|e| anyhow!("decompress {label}: {e:?}"))
}

fn spend_witness(
    public_input: &ShieldedIcs20WithdrawalInputPublic,
    private_input: &ShieldedIcs20WithdrawalInputPrivate,
    index: usize,
) -> Result<ShieldedIcs20WithdrawalSpendWitnessV1> {
    let state_commitment_auth_path = private_input
        .state_commitment_proof
        .auth_path()
        .iter()
        .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
        .collect::<Vec<_>>();

    Ok(ShieldedIcs20WithdrawalSpendWitnessV1 {
        nullifier: public_input.nullifier.0.to_bytes(),
        spent_note_blinding: private_input.spent_note.note_blinding().to_bytes(),
        spent_note_amount: Fq::from(private_input.spent_note.value().amount).to_bytes(),
        spent_note_asset_id: private_input.spent_note.asset_id().0.to_bytes(),
        spent_transmission_key: private_input.spent_note.transmission_key().0,
        spent_clue_key: Fq::from_le_bytes_mod_order(&private_input.spent_note.clue_key().0)
            .to_bytes(),
        state_commitment_commitment: private_input
            .state_commitment_proof
            .commitment()
            .0
            .to_bytes(),
        state_commitment_position: u64::from(private_input.state_commitment_proof.position()),
        state_commitment_auth_path,
        spend_auth_randomizer: private_input.spend_auth_randomizer.to_bytes(),
        is_dummy: private_input.is_dummy,
        dummy_nullifier_seed: private_input.dummy_nullifier_seed.to_bytes(),
        dummy_spend_auth_key: private_input.dummy_spend_auth_key.to_bytes(),
        rk_affine: point_affine_bytes(verification_key_point(
            public_input.rk,
            &format!("shielded_ics20_withdrawal_rk_{index}"),
        )?)?,
        spent_diversified_generator_affine: point_affine_bytes(
            private_input.spent_note.diversified_generator(),
        )?,
        spent_transmission_key_affine: point_affine_bytes(
            Encoding(private_input.spent_note.transmission_key().0)
                .vartime_decompress()
                .map_err(|e| anyhow!("decompress spent transmission key {index}: {e:?}"))?,
        )?,
    })
}

fn change_witness(
    public_output: &ShieldedIcs20WithdrawalChangePublic,
    private_output: &ShieldedIcs20WithdrawalChangePrivate,
) -> Result<ShieldedIcs20WithdrawalChangeWitnessV1> {
    Ok(ShieldedIcs20WithdrawalChangeWitnessV1 {
        note_commitment: public_output.note_commitment.0.to_bytes(),
        created_note_blinding: private_output.created_note.note_blinding().to_bytes(),
        created_note_amount: Fq::from(private_output.created_note.value().amount).to_bytes(),
        created_note_asset_id: private_output.created_note.asset_id().0.to_bytes(),
        created_transmission_key: private_output.created_note.transmission_key().0,
        created_clue_key: Fq::from_le_bytes_mod_order(&private_output.created_note.clue_key().0)
            .to_bytes(),
        created_diversified_generator_affine: point_affine_bytes(
            private_output.created_note.diversified_generator(),
        )?,
        created_transmission_key_affine: point_affine_bytes(
            Encoding(private_output.created_note.transmission_key().0)
                .vartime_decompress()
                .map_err(|e| anyhow!("decompress change transmission key: {e:?}"))?,
        )?,
    })
}

impl ShieldedIcs20WithdrawalWitnessV1 {
    pub fn from_public_private(
        public: &ShieldedIcs20WithdrawalProofPublic,
        private: &ShieldedIcs20WithdrawalProofPrivate,
    ) -> Result<Self> {
        public.validate_shape()?;
        private.validate_shape()?;
        if public.family_id != private.family_id {
            bail!(
                "shielded ICS-20 withdrawal witness family mismatch: public={} private={}",
                public.family_id.get(),
                private.family_id.get()
            );
        }

        let claimed_statement_hash =
            shielded_ics20_withdrawal_statement_hash_from_public(public)
                .map_err(|e| anyhow!("compute {} statement hash: {e}", public.family_id.label()))?;
        let statement_fields = shielded_ics20_withdrawal_statement_fields(public)
            .map_err(|e| anyhow!("compute {} statement fields: {e}", public.family_id.label()))?;

        let sender_leaf = compliance_leaf_from_typed(&private.sender_leaf)?;
        let (_, sender_asset_id, sender_slot_id, sender_slot_derivation, sender_d) =
            compliance_leaf_parts(&sender_leaf);

        let spends = public
            .inputs
            .iter()
            .zip(private.inputs.iter())
            .enumerate()
            .map(|(index, (public_input, private_input))| {
                spend_witness(public_input, private_input, index)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut witness = Self {
            family_id: public.family_id,
            total_length: 0,
            n_in: public.inputs.len() as u32,
            anchor: Fq::from(public.anchor).to_bytes(),
            balance_commitment: public.balance_commitment.to_bytes(),
            asset_anchor: public.asset_anchor.0.to_bytes(),
            compliance_anchor: public.compliance_anchor.0.to_bytes(),
            target_timestamp: public.target_timestamp.to_bytes(),
            outbound_asset_id: public.outbound_asset_id.to_bytes(),
            outbound_amount: public.outbound_amount.to_bytes(),
            withdrawal_effect_hash_lo: public.withdrawal_effect_hash_lo.to_bytes(),
            withdrawal_effect_hash_hi: public.withdrawal_effect_hash_hi.to_bytes(),
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
            sender_slot_id,
            sender_slot_derivation,
            sender_d,
            spends,
            change_output: change_witness(&public.change_output, &private.change_output)?,
            balance_commitment_affine: point_affine_bytes(public.balance_commitment.0)?,
            ak_affine: point_affine_bytes(verification_key_point(
                private.ak,
                "shielded_ics20_withdrawal_ak",
            )?)?,
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
            .map_err(|_| anyhow!("encoded {} witness exceeds u32", witness.family_id.label()))?;
        Ok(witness)
    }
}
