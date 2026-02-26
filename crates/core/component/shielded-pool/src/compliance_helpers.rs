//! Helper functions for generating compliance ciphertexts and leaves.
//!
//! This module provides client-side utilities for creating compliance proofs
//! using the ACK (Address Compliance Key) system with ring_pk-derived keys.

use anyhow::Result;
use penumbra_sdk_asset::asset;
use penumbra_sdk_compliance::derive_compliance_scalar;
use penumbra_sdk_compliance::{crypto, structs::ComplianceCiphertext, ComplianceLeaf};
use penumbra_sdk_keys::Address;
use penumbra_sdk_num::Amount;
use rand_core::{CryptoRng, RngCore};

/// Generated compliance data for a transaction action.
#[derive(Clone, Debug)]
pub struct GeneratedComplianceData {
    /// The compliance ciphertext bytes.
    pub ciphertext: Vec<u8>,
    /// The compliance leaf for the user's registry entry.
    pub leaf: ComplianceLeaf,
    /// The ephemeral secret(s) for circuit witness.
    /// For spend: r_s. For output: r_1.
    pub ephemeral_secret: decaf377::Fr,
    /// Additional ephemeral secrets for output (r_2, r_3).
    pub r_2: Option<decaf377::Fr>,
    pub r_3: Option<decaf377::Fr>,
    /// Random salt for DLEQ metadata hash (encrypted in detection tier).
    pub salt: decaf377::Fq,
    /// DLEQ nonce k (for computing DLEQ proof natively).
    pub dleq_k: decaf377::Fr,
    /// Additional DLEQ nonces for output tiers 2 and 3.
    pub dleq_k_2: Option<decaf377::Fr>,
    pub dleq_k_3: Option<decaf377::Fr>,
}

/// Generate compliance ciphertext for a Spend action (detection + core).
pub fn generate_compliance_details_spend(
    rng: &mut (impl RngCore + CryptoRng),
    ring_pk: &decaf377::Element,
    dk_pub: &decaf377::Element,
    user_address: &Address,
    asset_id: asset::Id,
    amount: Amount,
    is_flagged: bool,
) -> Result<GeneratedComplianceData> {
    let b_d_fq = user_address
        .diversified_generator()
        .vartime_compress_to_field();
    let d = derive_compliance_scalar(b_d_fq);
    let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
    let ack = *ring_pk * d_fr;

    let salt = Fq::rand(rng);
    let dleq_k = Fr::rand(rng);

    let result = crypto::encrypt_spend(
        rng,
        &ack,
        dk_pub,
        user_address,
        asset_id,
        amount,
        is_flagged,
        salt,
    )?;

    let d = penumbra_sdk_compliance::derive_compliance_scalar(b_d_fq);
    let leaf = ComplianceLeaf::new(user_address.clone(), asset_id, d);

    Ok(GeneratedComplianceData {
        ciphertext: result.ciphertext.to_bytes(),
        leaf,
        ephemeral_secret: result.r_s,
        r_2: None,
        r_3: None,
        salt,
        dleq_k,
        dleq_k_2: None,
        dleq_k_3: None,
    })
}

/// Generate compliance ciphertext for an Output action (all 3 tiers).
pub fn generate_compliance_details_output(
    rng: &mut (impl RngCore + CryptoRng),
    ring_pk: &decaf377::Element,
    dk_pub: &decaf377::Element,
    recipient_address: &Address,
    sender_address: &Address,
    asset_id: asset::Id,
    amount: Amount,
    is_flagged: bool,
) -> Result<GeneratedComplianceData> {
    let recv_b_d_fq = recipient_address
        .diversified_generator()
        .vartime_compress_to_field();
    let recv_d = derive_compliance_scalar(recv_b_d_fq);
    let recv_d_fr = Fr::from_le_bytes_mod_order(&recv_d.to_bytes());
    let ack_receiver = *ring_pk * recv_d_fr;

    let sender_b_d_fq = sender_address
        .diversified_generator()
        .vartime_compress_to_field();
    let sender_d = derive_compliance_scalar(sender_b_d_fq);
    let sender_d_fr = Fr::from_le_bytes_mod_order(&sender_d.to_bytes());
    let ack_sender = *ring_pk * sender_d_fr;

    let salt = Fq::rand(rng);
    let dleq_k = Fr::rand(rng);
    let dleq_k_2 = Fr::rand(rng);
    let dleq_k_3 = Fr::rand(rng);

    let result = crypto::encrypt_output(
        rng,
        &ack_receiver,
        &ack_sender,
        dk_pub,
        recipient_address,
        sender_address,
        asset_id,
        amount,
        is_flagged,
        salt,
    )?;

    let leaf = ComplianceLeaf::new(recipient_address.clone(), asset_id, recv_d);

    Ok(GeneratedComplianceData {
        ciphertext: result.ciphertext.to_bytes(),
        leaf,
        ephemeral_secret: result.r_1,
        r_2: Some(result.r_2),
        r_3: Some(result.r_3),
        salt,
        dleq_k,
        dleq_k_2: Some(dleq_k_2),
        dleq_k_3: Some(dleq_k_3),
    })
}

/// Serialize compliance ciphertext to bytes for inclusion in public inputs.
pub fn serialize_compliance_ciphertext(ciphertext: &ComplianceCiphertext) -> Vec<u8> {
    ciphertext.to_bytes()
}

// ============================================================================
// Proto conversion helpers (shared between spend/plan.rs and output/plan.rs)
// ============================================================================

use decaf377::{Fq, Fr};
use penumbra_sdk_proto::core::component::compliance::v1 as compliance_pb;
use penumbra_sdk_tct::StateCommitment;

/// Convert a `ComplianceLeaf` to its proto representation.
pub fn compliance_leaf_to_proto(leaf: &ComplianceLeaf) -> compliance_pb::ComplianceLeaf {
    compliance_pb::ComplianceLeaf {
        address: Some(leaf.address.clone().into()),
        asset_id: Some(leaf.asset_id.into()),
        d: leaf.d.to_bytes().to_vec(),
    }
}

/// Parse a `ComplianceLeaf` from its proto representation.
pub fn compliance_leaf_from_proto(
    proto: compliance_pb::ComplianceLeaf,
    context: &str,
) -> Result<ComplianceLeaf> {
    let address: Address = proto
        .address
        .ok_or_else(|| anyhow::anyhow!("missing address in {}", context))?
        .try_into()?;
    let asset_id = proto
        .asset_id
        .ok_or_else(|| anyhow::anyhow!("missing asset_id in {}", context))?
        .try_into()?;
    let d = if proto.d.is_empty() {
        Fq::from(0u64)
    } else {
        let arr: [u8; 32] = proto
            .d
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid d length in {}", context))?;
        Fq::from_bytes_checked(&arr)
            .map_err(|_| anyhow::anyhow!("invalid d bytes in {}", context))?
    };
    Ok(ComplianceLeaf::new(address, asset_id, d))
}

/// Convert an `IndexedLeaf` to its proto representation.
pub fn indexed_leaf_to_proto(
    leaf: &penumbra_sdk_compliance::IndexedLeaf,
) -> compliance_pb::IndexedLeafData {
    leaf.clone().into()
}

/// Parse an ephemeral secret (Fr) from proto bytes.
pub fn parse_ephemeral_secret(bytes: &[u8]) -> Result<Option<Fr>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid ephemeral secret length"))?;
    let fr = Fr::from_bytes_checked(&arr)
        .map_err(|_| anyhow::anyhow!("invalid ephemeral secret bytes"))?;
    Ok(Some(fr))
}

/// Parse a tx_blinding_nonce (Fr) from proto bytes, defaulting to zero if empty.
pub fn parse_tx_blinding_nonce(bytes: &[u8]) -> Result<Fr> {
    if bytes.is_empty() {
        return Ok(Fr::from(0u64));
    }
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid tx_blinding_nonce length"))?;
    Fr::from_bytes_checked(&arr).map_err(|_| anyhow::anyhow!("invalid tx_blinding_nonce bytes"))
}

/// Parse a StateCommitment from an optional proto, defaulting to zero commitment if absent.
pub fn parse_state_commitment_or_default(
    proto: Option<penumbra_sdk_proto::penumbra::crypto::tct::v1::StateCommitment>,
) -> Result<StateCommitment> {
    proto
        .map(|c| c.try_into())
        .transpose()?
        .ok_or(())
        .or_else(|_| Ok(StateCommitment(Fq::from(0u64))))
}

/// Parse a MerklePath from an optional proto, defaulting to empty path if absent.
pub fn parse_merkle_path_or_default(
    proto: Option<compliance_pb::MerklePath>,
) -> Result<penumbra_sdk_compliance::MerklePath> {
    proto
        .map(|p| p.try_into())
        .transpose()?
        .ok_or(())
        .or_else(|_| Ok(penumbra_sdk_compliance::MerklePath::default()))
}

/// Parse an IndexedLeaf from an optional proto, defaulting to sentinel if absent.
pub fn parse_indexed_leaf_or_default(
    proto: Option<compliance_pb::IndexedLeafData>,
) -> Result<penumbra_sdk_compliance::IndexedLeaf> {
    proto
        .map(penumbra_sdk_compliance::IndexedLeaf::try_from)
        .transpose()?
        .ok_or(())
        .or_else(|_| {
            Ok(penumbra_sdk_compliance::IndexedLeaf::with_default_policy(
                Fq::from(0u64),
                0,
                penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
            ))
        })
}
