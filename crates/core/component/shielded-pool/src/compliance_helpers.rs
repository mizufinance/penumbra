//! Helper functions for generating compliance ciphertexts and leaves.
//!
//! This module provides client-side utilities for creating compliance proofs
//! using the ACK (Wallet Compliance Key) system.

use anyhow::Result;
use penumbra_sdk_asset::asset;
use penumbra_sdk_compliance::{crypto, structs::ComplianceCiphertext, ComplianceLeaf, IndexedLeaf};
use penumbra_sdk_keys::{keys::AddressComplianceKey, Address};
use penumbra_sdk_num::Amount;
use rand_core::{CryptoRng, RngCore};

/// Generated compliance data for a transaction action.
///
/// This struct groups all the compliance-related data generated during transaction
/// construction, making it easier to pass around and use in proofs.
#[derive(Clone, Debug)]
pub struct GeneratedComplianceData {
    /// The compliance ciphertext containing encrypted transaction details.
    /// This gets included in the transaction's public inputs.
    pub ciphertext: Vec<u8>,
    /// The compliance leaf for the user's registry entry.
    /// This is used as a witness in the ZK proof.
    pub leaf: penumbra_sdk_compliance::ComplianceLeaf,
    /// The ephemeral secret used to encrypt the compliance data.
    /// This must be provided as a private witness to the circuit.
    pub ephemeral_secret: decaf377::Fr,
}

/// Generate compliance ciphertext and leaf for a transaction.
///
/// This function handles both regulated and unregulated assets:
/// - **Regulated**: Encrypts to the real user's ACK from the registry
/// - **Unregulated**: Encrypts to BLACK_HOLE_ACK (unlinkable)
///
/// # Arguments
///
/// * `rng` - Random number generator for ephemeral key generation
/// * `user_ack` - The user's Wallet Compliance Key (from registry, or dummy if unregulated)
/// * `user_address` - The user's address (contains diversifier)
/// * `date` - The current date (Unix day index: timestamp / 86400)
/// * `asset_id` - The asset being transacted
/// * `amount` - The amount being transacted
/// * `counterparty_address` - The other party's address in the transaction
/// * `asset_leaf` - The indexed leaf containing policy (dk_pub, threshold)
///
/// # Returns
///
/// A `GeneratedComplianceData` struct containing the ciphertext bytes, compliance leaf,
/// and ephemeral secret needed by the circuit as a private witness.
pub fn generate_compliance_details(
    rng: &mut (impl RngCore + CryptoRng),
    user_ack: &AddressComplianceKey,
    user_address: &Address,
    date: u64,
    asset_id: asset::Id,
    amount: Amount,
    counterparty_address: &Address,
    asset_leaf: &IndexedLeaf,
) -> Result<GeneratedComplianceData> {
    let result = crypto::encrypt_compliance_details(
        rng,
        user_ack,
        user_address,
        date,
        asset_id,
        amount,
        counterparty_address,
        asset_leaf,
    )?;

    // Create the compliance leaf for ZK proof
    let leaf = ComplianceLeaf {
        address: user_address.clone(),
        key: user_ack.clone(),
        asset_id,
    };

    Ok(GeneratedComplianceData {
        ciphertext: result.ciphertext.to_bytes(),
        leaf,
        ephemeral_secret: result.ephemeral_secret,
    })
}

/// Helper to convert Unix timestamp (seconds) to day index for ACK derivation.
#[inline]
pub fn timestamp_to_day_index(timestamp: u64) -> u64 {
    timestamp / 86400 // 86400 seconds per day
}

/// Serialize compliance ciphertext to bytes for inclusion in public inputs.
///
/// The format is:
/// - 32 bytes: ephemeral public key (compressed point)
/// - 32 bytes: detection tag
/// - Variable: encrypted core (asset_id + amount) with auth tag
/// - Variable: encrypted extension (counterparty address) with auth tag
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
        key: Some(compliance_pb::ComplianceViewingKey {
            inner: leaf.key.0.vartime_compress().0.to_vec(),
        }),
        asset_id: Some(leaf.asset_id.into()),
    }
}

/// Parse a `ComplianceLeaf` from its proto representation.
pub fn compliance_leaf_from_proto(
    proto: compliance_pb::ComplianceLeaf,
    context: &str,
) -> Result<ComplianceLeaf> {
    let address = proto
        .address
        .ok_or_else(|| anyhow::anyhow!("missing address in {}", context))?
        .try_into()?;
    let key = proto
        .key
        .ok_or_else(|| anyhow::anyhow!("missing key in {}", context))?;
    let key_bytes: [u8; 32] = key
        .inner
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid compliance key length in {}", context))?;
    let key_element = decaf377::Encoding(key_bytes)
        .vartime_decompress()
        .map_err(|_| anyhow::anyhow!("invalid compliance key encoding in {}", context))?;
    let asset_id = proto
        .asset_id
        .ok_or_else(|| anyhow::anyhow!("missing asset_id in {}", context))?
        .try_into()?;
    Ok(ComplianceLeaf {
        address,
        key: AddressComplianceKey::new(key_element),
        asset_id,
    })
}

/// Convert an `IndexedLeaf` to its proto representation.
pub fn indexed_leaf_to_proto(
    leaf: &penumbra_sdk_compliance::IndexedLeaf,
) -> compliance_pb::IndexedLeafData {
    compliance_pb::IndexedLeafData {
        value: leaf.value.to_bytes().to_vec(),
        next_index: leaf.next_index,
        next_value: leaf.next_value.to_bytes().to_vec(),
        dk_pub: leaf.policy.dk_pub.vartime_compress().0.to_vec(),
        threshold: leaf.policy.threshold.to_le_bytes().to_vec(),
    }
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

/// Parse an IndexedLeaf from an optional proto, defaulting to unregulated leaf if absent.
pub fn parse_indexed_leaf_or_default(
    proto: Option<compliance_pb::IndexedLeafData>,
) -> Result<penumbra_sdk_compliance::IndexedLeaf> {
    proto
        .map(penumbra_sdk_compliance::IndexedLeaf::try_from)
        .transpose()?
        .ok_or(())
        .or_else(|_| {
            Ok(penumbra_sdk_compliance::IndexedLeaf {
                value: Fq::from(0u64),
                next_index: 0,
                next_value: penumbra_sdk_compliance::indexed_tree::FQ_MAX.clone(),
                policy: penumbra_sdk_compliance::AssetPolicy::default_unregulated(),
            })
        })
}
