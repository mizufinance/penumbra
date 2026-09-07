use anyhow::{anyhow, bail, Result};
use decaf377::{Encoding, Fq};

use crate::{
    gnark::typed::{
        compliance_leaf_from_typed, indexed_leaf_from_typed, merkle_path_from_typed,
        point_affine_bytes, IndexedLeafBinary, MerklePathBinary, PointAffineBytes,
    },
    gnark::RecoveryCapsuleWitness,
    note_reshape::{
        NoteReshapeInputPrivate, NoteReshapeInputPublic, NoteReshapeOutputPrivate,
        NoteReshapeOutputPublic, NoteReshapeProofPrivate, NoteReshapeProofPublic,
    },
    public_input_hash::note_reshape_statement_hash_from_public,
    NoteReshapeFamilyId,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteReshapeSpendWitness {
    pub(crate) is_dummy: bool,
    pub nullifier: [u8; 32],
    pub(crate) dummy_nullifier_seed: [u8; 32],
    pub spent_note_blinding: [u8; 32],
    pub spent_note_amount: [u8; 32],
    pub spent_note_recovery_commitment: [u8; 32],
    pub state_commitment_commitment: [u8; 32],
    pub state_commitment_position: u64,
    pub state_commitment_auth_path: Vec<[[u8; 32]; 3]>,
    pub spend_auth_randomizer: [u8; 32],
    pub rk_affine: PointAffineBytes,
    pub history_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteReshapeOutputWitness {
    pub note_commitment: [u8; 32],
    pub recovery_commitment: [u8; 32],
    pub created_note_blinding: [u8; 32],
    pub created_note_amount: [u8; 32],
    pub recovery_capsule: RecoveryCapsuleWitness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteReshapeSharedNoteContextWitness {
    pub asset_id: [u8; 32],
    pub diversified_generator_affine: PointAffineBytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteReshapeWitness {
    pub family_id: NoteReshapeFamilyId,
    pub total_length: u32,
    pub n_in: u32,
    pub n_out: u32,
    pub anchor: [u8; 32],
    pub claimed_statement_hash: [u8; 32],
    pub asset_anchor: [u8; 32],
    pub compliance_anchor: [u8; 32],
    pub routing_tag: [u8; 32],
    pub routing_parameter_set_id: [u8; 32],
    pub recent_position_floor: [u8; 32],
    pub action_balance_blinding: [u8; 32],
    pub nk: [u8; 32],
    pub asset_path: MerklePathBinary,
    pub asset_position: u64,
    pub asset_indexed_leaf: IndexedLeafBinary,
    pub asset_indexed_leaf_dk_pub_affine: PointAffineBytes,
    pub asset_indexed_leaf_ring_pk_affine: PointAffineBytes,
    pub is_regulated: bool,
    pub regulated_precision: u8,
    pub unregulated_precision: u8,
    pub routing_as_of_height: u64,
    pub routing_nonce: [u8; 32],
    pub sender_compliance_path: MerklePathBinary,
    pub sender_compliance_position: u64,
    pub sender_capk_affine: PointAffineBytes,
    pub sender_rnk_dh_pk_affine: PointAffineBytes,
    pub sender_rnk_commitment: [u8; 32],
    pub sender_status: [u8; 32],
    pub shared: NoteReshapeSharedNoteContextWitness,
    pub spends: Vec<NoteReshapeSpendWitness>,
    pub outputs: Vec<NoteReshapeOutputWitness>,
    pub balance_commitment_affine: PointAffineBytes,
    pub ak_affine: PointAffineBytes,
}

fn compliance_leaf_parts(
    leaf: &crate::gnark::typed::ComplianceLeafBinary,
) -> (PointAffineBytes, PointAffineBytes, [u8; 32], [u8; 32]) {
    (
        leaf.capk_affine.clone(),
        leaf.rnk_dh_pk_affine.clone(),
        leaf.rnk_commitment,
        leaf.status,
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
    public: &NoteReshapeInputPublic,
    private: &NoteReshapeInputPrivate,
    index: usize,
) -> Result<NoteReshapeSpendWitness> {
    let state_commitment_auth_path = private
        .state_commitment_proof
        .auth_path()
        .iter()
        .map(|siblings| siblings.map(|sibling| Fq::from(sibling).to_bytes()))
        .collect::<Vec<_>>();
    Ok(NoteReshapeSpendWitness {
        is_dummy: private.is_dummy,
        nullifier: public.nullifier.0.to_bytes(),
        dummy_nullifier_seed: private.dummy_nullifier_seed.to_bytes(),
        spent_note_blinding: private.spent_note.note_blinding().to_bytes(),
        spent_note_amount: Fq::from(private.spent_note.value().amount).to_bytes(),
        spent_note_recovery_commitment: private.spent_note.recovery_commitment().0.to_bytes(),
        state_commitment_commitment: private.state_commitment_proof.commitment().0.to_bytes(),
        state_commitment_position: u64::from(private.state_commitment_proof.position()),
        state_commitment_auth_path,
        spend_auth_randomizer: private.spend_auth_randomizer.to_bytes(),
        rk_affine: point_affine_bytes(verification_key_point(public.rk, &format!("rk_{index}"))?)?,
        history_required: public.history_required,
    })
}

fn output_witness(
    public: &NoteReshapeOutputPublic,
    private: &NoteReshapeOutputPrivate,
    capk: decaf377::Element,
) -> Result<NoteReshapeOutputWitness> {
    Ok(NoteReshapeOutputWitness {
        note_commitment: public.note_commitment.0.to_bytes(),
        recovery_commitment: public.recovery_commitment.0.to_bytes(),
        created_note_blinding: private.created_note.note_blinding().to_bytes(),
        created_note_amount: Fq::from(private.created_note.value().amount).to_bytes(),
        recovery_capsule: RecoveryCapsuleWitness::from_note(&private.created_note, capk)?,
    })
}

impl NoteReshapeWitness {
    pub fn from_public_private(
        public: &NoteReshapeProofPublic,
        private: &NoteReshapeProofPrivate,
    ) -> Result<Self> {
        public.validate_shape()?;
        private.validate_shape()?;
        if public.family_id != private.family_id {
            bail!(
                "note reshape witness family mismatch: public={} private={}",
                public.family_id.get(),
                private.family_id.get()
            );
        }
        if public.inputs.len() != private.inputs.len()
            || public.outputs.len() != private.outputs.len()
        {
            bail!("note reshape public/private witness vector lengths differ")
        }

        let claimed_statement_hash = note_reshape_statement_hash_from_public(public)
            .map_err(|e| anyhow!("compute {} statement hash: {e}", public.family_id.label()))?;
        let spends = public
            .inputs
            .iter()
            .zip(private.inputs.iter())
            .enumerate()
            .map(|(index, (public, private))| spend_witness(public, private, index))
            .collect::<Result<Vec<_>>>()?;
        let outputs = public
            .outputs
            .iter()
            .zip(private.outputs.iter())
            .map(|(public, output)| output_witness(public, output, private.sender_leaf.capk))
            .collect::<Result<Vec<_>>>()?;
        let first_input = private
            .inputs
            .first()
            .ok_or_else(|| anyhow!("note reshape witness requires a real first input"))?;
        let shared = NoteReshapeSharedNoteContextWitness {
            asset_id: first_input.spent_note.asset_id().0.to_bytes(),
            diversified_generator_affine: point_affine_bytes(
                first_input.spent_note.diversified_generator(),
            )?,
        };
        let sender_leaf = compliance_leaf_from_typed(&private.sender_leaf)?;
        let (sender_capk_affine, sender_rnk_dh_pk_affine, sender_rnk_commitment, sender_status) =
            compliance_leaf_parts(&sender_leaf);

        let mut witness = Self {
            family_id: public.family_id,
            total_length: 0,
            n_in: public.inputs.len() as u32,
            n_out: public.outputs.len() as u32,
            anchor: Fq::from(public.anchor).to_bytes(),
            claimed_statement_hash: claimed_statement_hash.to_bytes(),
            asset_anchor: public.asset_anchor.0.to_bytes(),
            compliance_anchor: public.compliance_anchor.0.to_bytes(),
            routing_tag: Fq::from(public.routing_tag.value).to_bytes(),
            routing_parameter_set_id: public.routing_parameter_set_id.to_bytes(),
            recent_position_floor: Fq::from(public.recent_position_floor).to_bytes(),
            action_balance_blinding: private.action_balance_blinding.to_bytes(),
            nk: private.nk.0.to_bytes(),
            asset_path: merkle_path_from_typed(&private.asset_path)?,
            asset_position: private.asset_position,
            asset_indexed_leaf: indexed_leaf_from_typed(&private.asset_indexed_leaf),
            asset_indexed_leaf_dk_pub_affine: point_affine_bytes(
                private.asset_indexed_leaf.params.dk_pub,
            )?,
            asset_indexed_leaf_ring_pk_affine: point_affine_bytes(
                private.asset_indexed_leaf.ring.ring_pk,
            )?,
            is_regulated: private.is_regulated,
            regulated_precision: private.routing_parameters.regulated_precision.bits(),
            unregulated_precision: private.routing_parameters.unregulated_precision.bits(),
            routing_as_of_height: private.routing_parameters.as_of_height,
            routing_nonce: private.routing_nonce.to_bytes(),
            sender_compliance_path: merkle_path_from_typed(&private.sender_compliance_path)?,
            sender_compliance_position: private.sender_compliance_position,
            sender_capk_affine,
            sender_rnk_dh_pk_affine,
            sender_rnk_commitment,
            sender_status,
            shared,
            spends,
            outputs,
            balance_commitment_affine: point_affine_bytes(public.balance_commitment.0)?,
            ak_affine: point_affine_bytes(verification_key_point(private.ak, "ak")?)?,
        };
        witness.total_length = u32::try_from(witness.encode()?.len())
            .map_err(|_| anyhow!("encoded {} witness exceeds u32", witness.family_id.label()))?;
        Ok(witness)
    }
}
