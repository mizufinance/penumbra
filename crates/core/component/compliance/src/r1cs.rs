//! R1CS gadgets for compliance verification.
//!
//! This module implements zero-knowledge constraint gadgets for proving compliance
//! properties without revealing sensitive transaction details.

use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};
use decaf377::r1cs::{ElementVar, FqVar};
use decaf377::{Fq, Fr};
use once_cell::sync::Lazy;

use penumbra_sdk_keys::keys::AddressComplianceKey;

use crate::indexed_tree::{IndexedLeaf, IMT_LEAF_DOMAIN_SEP};
use crate::structs::{ComplianceLeaf, MerklePath};
use crate::tree::DEFAULT_DEPTH;

/// Domain separator for detection key derivation.
static DETECTION_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.keytype.detection").as_bytes(),
    )
});

/// Domain separator for core key derivation.
static CORE_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.keytype.core").as_bytes(),
    )
});

/// Domain separator for extension key derivation.
static EXTENSION_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"penumbra.compliance.keytype.extension").as_bytes(),
    )
});

/// Verify a QuadTree Merkle authentication path in R1CS.
pub fn verify_quad_path(
    cs: ConstraintSystemRef<Fq>,
    leaf_var: FqVar,
    path: &MerklePath,
    position_var: FqVar,
) -> ark_relations::r1cs::Result<FqVar> {
    let mut current_hash = leaf_var;

    // Domain separator for hashing (consistent with QuadTree::hash_children)
    let zero_domain_sep = FqVar::new_constant(cs.clone(), Fq::from(0u64))?;

    // Convert position to little-endian bits
    let pos_bits = position_var.to_bits_le()?;

    // Process exactly DEFAULT_DEPTH layers to maintain fixed circuit size
    // (circuits must have fixed constraint count)
    for layer_idx in 0..(DEFAULT_DEPTH as usize) {
        let siblings = if layer_idx < path.layers.len() {
            // Use real siblings from witness path
            let layer = &path.layers[layer_idx];
            let mut vars = Vec::with_capacity(3);
            for sibling_bytes in &layer.siblings {
                let mut bytes_arr = [0u8; 32];
                bytes_arr.copy_from_slice(sibling_bytes);
                let sibling_fq = Fq::from_le_bytes_mod_order(&bytes_arr);
                vars.push(FqVar::new_witness(cs.clone(), || Ok(sibling_fq))?);
            }
            vars
        } else {
            // Use dummy zero siblings if path is shorter than FIXED_DEPTH
            vec![
                FqVar::new_witness(cs.clone(), || Ok(Fq::from(0u64)))?,
                FqVar::new_witness(cs.clone(), || Ok(Fq::from(0u64)))?,
                FqVar::new_witness(cs.clone(), || Ok(Fq::from(0u64)))?,
            ]
        };

        // Extract 2 bits for quad-tree navigation at this layer
        let bit_offset = layer_idx * 2;
        let bit_0 = if bit_offset < pos_bits.len() {
            pos_bits[bit_offset].clone()
        } else {
            Boolean::FALSE
        };
        let bit_1 = if bit_offset + 1 < pos_bits.len() {
            pos_bits[bit_offset + 1].clone()
        } else {
            Boolean::FALSE
        };

        // Construct the 4 children for quad-tree hashing based on position bits
        // child_index = bit_0 + 2*bit_1 (0, 1, 2, or 3)
        // Siblings array contains the 3 sibling hashes (excluding the current hash position)
        //
        // Native mapping (from auth_path):
        //   index=0: [current_hash, siblings[0], siblings[1], siblings[2]]
        //   index=1: [siblings[0], current_hash, siblings[1], siblings[2]]
        //   index=2: [siblings[0], siblings[1], current_hash, siblings[2]]
        //   index=3: [siblings[0], siblings[1], siblings[2], current_hash]
        let is_index_0 = bit_0.not().and(&bit_1.not())?; // !b0 && !b1
        let is_index_1 = bit_0.clone().and(&bit_1.not())?; // b0 && !b1
        let is_index_2 = bit_0.not().and(&bit_1.clone())?; // !b0 && b1
        let is_index_3 = bit_0.clone().and(&bit_1.clone())?; // b0 && b1

        // child_0: current_hash if index=0, else siblings[0]
        let child_0 = is_index_0.select(&current_hash, &siblings[0])?;

        // child_1: current_hash if index=1, else siblings[0] if index=0, else siblings[1]
        let child_1_not_1 = is_index_0.select(&siblings[0], &siblings[1])?;
        let child_1 = is_index_1.select(&current_hash, &child_1_not_1)?;

        // child_2: current_hash if index=2, else siblings[1] if index=0|1, else siblings[2]
        let child_2_not_2 = bit_1.select(&siblings[2], &siblings[1])?; // if bit_1=1 (index 2 or 3), siblings[2]; else siblings[1]
        let child_2 = is_index_2.select(&current_hash, &child_2_not_2)?;

        // child_3: current_hash if index=3, else siblings[2]
        let child_3 = is_index_3.select(&current_hash, &siblings[2])?;

        let parent_hash = poseidon377::r1cs::hash_4(
            cs.clone(),
            &zero_domain_sep,
            (child_0, child_1, child_2, child_3),
        )?;

        current_hash = parent_hash;
    }

    Ok(current_hash)
}

// ============================================================================
// Compliance Key R1CS Gadgets
// ============================================================================

/// R1CS variable representing a Address Compliance Key (ACK).
///
/// The ACK is a public curve point `ACK = msk * B_d` stored in the compliance registry.
/// In the circuit, we work with its variable representation to prove key derivation.
#[derive(Clone)]
pub struct AddressComplianceKeyVar {
    /// The elliptic curve point representing the address compliance key.
    pub inner: ElementVar,
}

impl AddressComplianceKeyVar {
    /// Create a new AddressComplianceKeyVar from an ElementVar.
    pub fn new(inner: ElementVar) -> Self {
        Self { inner }
    }

    /// Derive the daily public key for a specific key type and date in-circuit.
    ///
    /// # Algorithm (Circuit Version)
    ///
    /// Given:
    /// - `self.inner`: ACK (point) = msk * B_d
    /// - `date`: Unix day index (field element)
    /// - `diversified_generator`: B_d (point)
    /// - `domain`: Key type domain separator (detection, core, or extension)
    ///
    /// Compute:
    /// 1. `T = Poseidon(domain, (date, 0))`
    /// 2. `Q = T * B_d` (scalar multiplication)
    /// 3. `PK_day = ACK + Q` (point addition)
    ///
    /// Returns: `PK_day` as ElementVar
    pub fn derive_daily_public_key_with_domain(
        &self,
        cs: ConstraintSystemRef<Fq>,
        date: &FqVar,
        diversified_generator: &ElementVar,
        domain: &Fq,
    ) -> Result<ElementVar, SynthesisError> {
        let domain_sep = FqVar::new_constant(cs.clone(), *domain)?;
        let zero = FqVar::new_constant(cs.clone(), Fq::from(0u64))?;
        let tweak_fq = poseidon377::r1cs::hash_2(cs.clone(), &domain_sep, (date.clone(), zero))?;

        let q_point = diversified_generator.scalar_mul_le(tweak_fq.to_bits_le()?.iter())?;
        let pk_day = self.inner.clone() + q_point;

        Ok(pk_day)
    }

    /// Derive all three daily public keys (detection, core, extension) for a date.
    ///
    /// Returns: (pk_detection, pk_core, pk_extension)
    pub fn derive_all_daily_public_keys(
        &self,
        cs: ConstraintSystemRef<Fq>,
        date: &FqVar,
        diversified_generator: &ElementVar,
    ) -> Result<(ElementVar, ElementVar, ElementVar), SynthesisError> {
        let pk_detection = self.derive_daily_public_key_with_domain(
            cs.clone(),
            date,
            diversified_generator,
            &DETECTION_DOMAIN,
        )?;
        let pk_core = self.derive_daily_public_key_with_domain(
            cs.clone(),
            date,
            diversified_generator,
            &CORE_DOMAIN,
        )?;
        let pk_extension = self.derive_daily_public_key_with_domain(
            cs,
            date,
            diversified_generator,
            &EXTENSION_DOMAIN,
        )?;

        Ok((pk_detection, pk_core, pk_extension))
    }

    /// Get the inner ElementVar.
    pub fn inner(&self) -> &ElementVar {
        &self.inner
    }
}

impl AllocVar<AddressComplianceKey, Fq> for AddressComplianceKeyVar {
    fn new_variable<T: std::borrow::Borrow<AddressComplianceKey>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();

        let ack = f()?;
        let ack_point = ack.borrow().inner();

        // Allocate the curve point as an ElementVar
        //  Need to explicitly specify type parameter for Element
        let inner = ElementVar::new_variable(cs, || Ok(*ack_point), mode)?;

        Ok(Self { inner })
    }
}

// ============================================================================
// Compliance Leaf R1CS Gadgets
// ============================================================================

/// R1CS variable representing a Compliance Leaf.
///
/// This structure must match `structs::ComplianceLeaf` exactly so that
/// the commitment computed in-circuit matches the commitment in the registry.
pub struct ComplianceLeafVar {
    /// The wallet address (contains diversifier and transmission key).
    pub address: penumbra_sdk_keys::AddressVar,
    /// The address compliance key (public curve point).
    pub key: AddressComplianceKeyVar,
    /// The asset ID being regulated.
    pub asset_id: FqVar,
}

impl ComplianceLeafVar {
    /// Compute the Poseidon commitment of this leaf.
    ///
    /// This MUST match the logic in `structs::ComplianceLeaf::commit()` exactly!
    ///
    /// # Algorithm
    ///
    /// 1. Extract field elements from the address (diversified generator, transmission key, clue key)
    /// 2. Compress ACK curve point to field element
    /// 3. Hash all fields: `Hash(0, (ack_field, div_gen_x, div_gen_y, tx_key, clue_key, asset_id))`
    ///
    /// Returns: FqVar representing the leaf commitment
    pub fn commit(&self, cs: ConstraintSystemRef<Fq>) -> Result<FqVar, SynthesisError> {
        // Domain separator (must match structs::ComplianceLeaf::commit and COMPLIANCE_LEAF_DOMAIN_SEP)
        let domain_sep = FqVar::new_constant(
            cs.clone(),
            Fq::from_le_bytes_mod_order(
                blake2b_simd::blake2b(b"penumbra.compliance.leaf").as_bytes(),
            ),
        )?;

        // 1. Get diversified generator from address and compress to field element
        let div_gen = self.address.diversified_generator();
        let div_gen_compressed = div_gen.compress_to_field()?;

        // 2. Get transmission key from address and compress to field element
        let transmission_key = self.address.transmission_key();
        let transmission_key_s = transmission_key.compress_to_field()?;

        // 3. Compress ACK point to field element
        let ack_field = self.key.inner().compress_to_field()?;

        // 4. Hash: Poseidon_4(domain, (div_gen, tx_key, ack, asset_id))
        //    This MUST match the native structs::ComplianceLeaf::commit() logic exactly!
        //    Order: diversified_generator, transmission_key_s, ack_field, asset_id_field
        let commitment = poseidon377::r1cs::hash_4(
            cs,
            &domain_sep,
            (
                div_gen_compressed,
                transmission_key_s,
                ack_field,
                self.asset_id.clone(),
            ),
        )?;

        Ok(commitment)
    }
}

impl AllocVar<ComplianceLeaf, Fq> for ComplianceLeafVar {
    fn new_variable<T: std::borrow::Borrow<ComplianceLeaf>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();

        let leaf = f()?;
        let leaf_ref = leaf.borrow();

        // Allocate each field
        let address = penumbra_sdk_keys::AddressVar::new_variable(
            cs.clone(),
            || Ok(&leaf_ref.address),
            mode,
        )?;
        let key = AddressComplianceKeyVar::new_variable(cs.clone(), || Ok(&leaf_ref.key), mode)?;

        // Convert asset::Id to Fq
        let asset_id_fq = leaf_ref.asset_id.0;
        let asset_id = FqVar::new_variable(cs, || Ok(asset_id_fq), mode)?;

        Ok(Self {
            address,
            key,
            asset_id,
        })
    }
}

// ============================================================================
// Compliance Encryption Verification Gadgets
// ============================================================================

/// Verify that compliance ciphertext was correctly generated using ECDH.
///
/// This gadget proves the following in zero-knowledge:
/// 1. The ephemeral public key `R` was correctly computed: `R = r * B_d`
/// 2. The shared secret `S` was correctly computed: `S = r * PK_day`
///
/// # Inputs
///
/// - `ephemeral_secret` (r): Private witness - the random scalar used for ECDH
/// - `pk_day`: Public/witness - the daily public key derived from ACK
/// - `diversified_generator` (B_d): Public input - from the recipient's address
/// - `published_epk` (R): Public input - from the ciphertext (ephemeral public key)
/// - `published_shared_secret` (S): Witness - the ECDH shared secret
///
/// # Constraints
///
/// 1. `published_epk == ephemeral_secret * diversified_generator`
/// 2. `published_shared_secret == ephemeral_secret * pk_day`
///
/// # Returns
///
/// `Ok(())` if all constraints are satisfied, error otherwise.
pub fn verify_compliance_encryption(
    _cs: ConstraintSystemRef<Fq>,
    ephemeral_secret: &FqVar,             // r (witness)
    pk_day: &ElementVar,                  // PK_day = ACK + (T * B_d) (witness/public)
    diversified_generator: &ElementVar,   // B_d (public from address)
    published_epk: &ElementVar,           // R (public from ciphertext)
    published_shared_secret: &ElementVar, // S (witness)
) -> Result<(), SynthesisError> {
    // Convert ephemeral_secret to bits for scalar multiplication
    let r_bits = ephemeral_secret.to_bits_le()?;

    // Constraint 1: R = r * B_d
    let computed_epk = diversified_generator.scalar_mul_le(r_bits.iter())?;
    computed_epk.enforce_equal(published_epk)?;

    // Constraint 2: S = r * PK_day
    let computed_shared_secret = pk_day.scalar_mul_le(r_bits.iter())?;
    computed_shared_secret.enforce_equal(published_shared_secret)?;

    Ok(())
}

// ============================================================================
// Compliance Witness Structure
// ============================================================================

/// Witness data for compliance verification.
///
/// This struct groups all the private witness data needed to prove compliance
/// without revealing sensitive information. It's used by the `verify_compliance_integrity`
/// function to verify both asset registry and user registry inclusion.
#[derive(Clone)]
pub struct ComplianceWitness {
    /// Whether the asset is regulated (requires compliance checks).
    pub is_regulated: bool,
    /// Indexed Merkle Tree leaf for asset registry (membership or non-membership proof).
    pub asset_indexed_leaf: IndexedLeaf,
    /// Merkle path proving asset's regulatory status in the Asset Registry IMT.
    pub asset_path: MerklePath,
    /// Position of the leaf in the Asset Registry IMT.
    pub asset_position: u64,
    /// Merkle path proving user authorization in the Compliance Registry.
    pub compliance_path: MerklePath,
    /// Position of the user in the Compliance Registry tree.
    pub compliance_position: u64,
    /// The compliance leaf containing address, ACK, and asset_id.
    pub user_leaf: ComplianceLeaf,
    /// Whether this transaction is flagged (amount >= threshold).
    /// Circuit verifies this matches the threshold comparison.
    pub is_flagged: bool,
}

/// Circuit variable for asset policy (dk_pub and threshold).
pub struct AssetPolicyVar {
    /// Issuer's detection key public (for threshold-based flagging).
    pub dk_pub: ElementVar,
    /// Amount threshold (transfers >= threshold are flagged).
    pub threshold: FqVar,
}

/// Circuit variable for an Indexed Merkle Tree leaf.
///
/// Used for asset registry membership and non-membership proofs.
/// The policy fields are bound by the IMT membership proof.
pub struct IndexedLeafVar {
    /// The value stored in this leaf (asset_id for membership proofs).
    pub value: FqVar,
    /// Position in tree of the next-higher value leaf.
    pub next_index: FqVar,
    /// Value at next_index (for gap verification in non-membership proofs).
    pub next_value: FqVar,
    /// Asset policy containing dk_pub and threshold.
    pub policy: AssetPolicyVar,
}

impl IndexedLeafVar {
    /// Allocate as witness variables.
    pub fn new_witness(
        cs: ConstraintSystemRef<Fq>,
        leaf: &IndexedLeaf,
    ) -> Result<Self, SynthesisError> {
        let value = FqVar::new_witness(cs.clone(), || Ok(leaf.value))?;
        let next_index = FqVar::new_witness(cs.clone(), || Ok(Fq::from(leaf.next_index)))?;
        let next_value = FqVar::new_witness(cs.clone(), || Ok(leaf.next_value))?;
        let dk_pub = ElementVar::new_witness(cs.clone(), || Ok(leaf.policy.dk_pub))?;
        let threshold = FqVar::new_witness(cs, || {
            // Convert u128 threshold to Fq via its byte representation
            Ok(Fq::from_le_bytes_mod_order(
                &leaf.policy.threshold.to_le_bytes(),
            ))
        })?;
        Ok(Self {
            value,
            next_index,
            next_value,
            policy: AssetPolicyVar { dk_pub, threshold },
        })
    }

    /// Compute the leaf commitment in-circuit.
    ///
    /// Uses hash_5 to match native IndexedLeaf::commit():
    /// Poseidon_5(domain, (value, next_index, next_value, dk_pub_fq, threshold))
    pub fn commit(&self, cs: ConstraintSystemRef<Fq>) -> Result<FqVar, SynthesisError> {
        let domain_sep = FqVar::new_constant(cs.clone(), *IMT_LEAF_DOMAIN_SEP)?;
        // Compress dk_pub to a single field element (must match native vartime_compress_to_field)
        let dk_pub_fq = self.policy.dk_pub.compress_to_field()?;
        poseidon377::r1cs::hash_5(
            cs,
            &domain_sep,
            (
                self.value.clone(),
                self.next_index.clone(),
                self.next_value.clone(),
                dk_pub_fq,
                self.policy.threshold.clone(),
            ),
        )
    }
}

/// Circuit variable representation of compliance plaintext.
///
/// This struct wraps the circuit variables that represent the plaintext data
/// encrypted in the compliance ciphertext. It provides methods to serialize
/// the data to bits in a way that exactly matches the native byte serialization
/// used by the encryption function.
pub struct CompliancePlaintextVar {
    /// Amount being transacted (u128 value stored in FqVar).
    pub amount: FqVar,
    /// Asset ID being transacted.
    pub asset_id: FqVar,
    /// Self diversified generator (party who can decrypt this ciphertext).
    pub self_diversified_generator: ElementVar,
    /// Self transmission key.
    pub self_transmission_key: ElementVar,
    /// Counterparty diversified generator (the other party in the transaction).
    pub counterparty_diversified_generator: ElementVar,
    /// Counterparty transmission key.
    pub counterparty_transmission_key: ElementVar,
}

impl CompliancePlaintextVar {
    /// Get detection plaintext: asset_id (1 Fq element).
    ///
    /// This is encrypted directly as a single Fq - no chunking needed since
    /// asset_id is already an Fq field element.
    pub fn detection_plaintext(&self) -> FqVar {
        self.asset_id.clone()
    }

    /// Get core plaintext as Fq elements: amount + self address (3 Fq elements).
    ///
    /// Packs: amount (16 bytes) + self_div_gen (32 bytes) + self_trans_key (32 bytes)
    /// Total: 80 bytes → ceil(80/31) = 3 Fq elements using 31-byte chunks
    pub fn core_plaintext_fqs(&self) -> Result<Vec<FqVar>, SynthesisError> {
        use crate::structs::{AMOUNT_BYTES, GENERATOR_BYTES, KEY_BYTES};

        // Build core bytes: amount (16) + self_div_gen (32) + self_trans_key (32) = 80 bytes
        let total_bits = (AMOUNT_BYTES + GENERATOR_BYTES + KEY_BYTES) * 8; // 640 bits
        let mut bits = Vec::with_capacity(total_bits);

        // Amount: 128 bits (16 bytes)
        let mut amount_bits = self.amount.to_bits_le()?;
        amount_bits.truncate(AMOUNT_BYTES * 8);
        bits.append(&mut amount_bits);

        // Self Diversified Generator: 256 bits (32 bytes)
        let mut self_div_gen_bits = self
            .self_diversified_generator
            .compress_to_field()?
            .to_bits_le()?;
        self_div_gen_bits.resize(GENERATOR_BYTES * 8, Boolean::FALSE);
        bits.append(&mut self_div_gen_bits);

        // Self Transmission Key: 256 bits (32 bytes)
        let mut self_trans_key_bits = self
            .self_transmission_key
            .compress_to_field()?
            .to_bits_le()?;
        self_trans_key_bits.resize(KEY_BYTES * 8, Boolean::FALSE);
        bits.append(&mut self_trans_key_bits);

        // Pack into Fq elements using 31-byte (248-bit) chunks
        let chunk_size_bits = 31 * 8;
        let mut fqs = Vec::new();
        for chunk in bits.chunks(chunk_size_bits) {
            let mut padded_chunk = chunk.to_vec();
            padded_chunk.resize(256, Boolean::FALSE);
            let fq_var = Boolean::le_bits_to_fp_var(&padded_chunk)?;
            fqs.push(fq_var);
        }

        Ok(fqs)
    }

    /// Get extension plaintext as Fq elements: counterparty address (3 Fq elements).
    ///
    /// Packs: counterparty_div_gen (32 bytes) + counterparty_trans_key (32 bytes)
    /// Total: 64 bytes → ceil(64/31) = 3 Fq elements using 31-byte chunks
    pub fn extension_plaintext_fqs(&self) -> Result<Vec<FqVar>, SynthesisError> {
        use crate::structs::{GENERATOR_BYTES, KEY_BYTES};

        // Build extension bytes: counterparty_div_gen (32) + counterparty_trans_key (32) = 64 bytes
        let total_bits = (GENERATOR_BYTES + KEY_BYTES) * 8; // 512 bits
        let mut bits = Vec::with_capacity(total_bits);

        // Counterparty Diversified Generator: 256 bits (32 bytes)
        let mut counterparty_div_gen_bits = self
            .counterparty_diversified_generator
            .compress_to_field()?
            .to_bits_le()?;
        counterparty_div_gen_bits.resize(GENERATOR_BYTES * 8, Boolean::FALSE);
        bits.append(&mut counterparty_div_gen_bits);

        // Counterparty Transmission Key: 256 bits (32 bytes)
        let mut counterparty_trans_key_bits = self
            .counterparty_transmission_key
            .compress_to_field()?
            .to_bits_le()?;
        counterparty_trans_key_bits.resize(KEY_BYTES * 8, Boolean::FALSE);
        bits.append(&mut counterparty_trans_key_bits);

        // Pack into Fq elements using 31-byte (248-bit) chunks
        let chunk_size_bits = 31 * 8;
        let mut fqs = Vec::new();
        for chunk in bits.chunks(chunk_size_bits) {
            let mut padded_chunk = chunk.to_vec();
            padded_chunk.resize(256, Boolean::FALSE);
            let fq_var = Boolean::le_bits_to_fp_var(&padded_chunk)?;
            fqs.push(fq_var);
        }

        Ok(fqs)
    }
}

// ============================================================================
// Unified Compliance Integrity Verification
// ============================================================================

/// Verify compliance integrity for a transaction action.
///
/// Verifies asset registry inclusion, user registry inclusion, and ciphertext integrity.
/// Both regulated and unregulated paths use identical constraint counts.
///
/// For threshold-based flagging:
/// - If `threshold > 0` and `amount >= threshold`, the output is "flagged"
/// - Flagged outputs encrypt core+extension tiers to issuer's DK_pub
/// - The circuit verifies `is_flagged == (amount >= threshold)`
///
/// dk_pub and threshold are extracted from the asset_indexed_leaf (private witness).
/// The IMT membership proof binds the leaf to the public asset_anchor, so the
/// prover cannot lie about policy without breaking the Merkle proof.
pub fn verify_compliance_integrity(
    cs: ConstraintSystemRef<Fq>,
    // Public Inputs
    asset_anchor: FqVar,
    compliance_anchor: FqVar,
    target_date: FqVar,
    compliance_epk: ElementVar,
    compliance_epk_g: ElementVar,
    compliance_ciphertext: Vec<FqVar>,
    // Note data (to ensure binding)
    note_asset_id: FqVar,
    note_amount: FqVar,
    note_diversified_generator: ElementVar,
    note_transmission_key: ElementVar,
    counterparty_diversified_generator: ElementVar,
    counterparty_transmission_key: ElementVar,
    // Witness data
    compliance_ephemeral_secret: Fr,
    witness: ComplianceWitness,
) -> Result<(), SynthesisError> {
    tracing::debug!(
        constraints_before_alloc = cs.num_constraints(),
        "verify_compliance_integrity: start"
    );

    // Debug: log note_asset_id and note_amount values
    if let (Ok(asset_id), Ok(amount)) = (note_asset_id.value(), note_amount.value()) {
        tracing::debug!(
            note_asset_id = ?asset_id.to_bytes(),
            note_amount = ?amount.to_bytes(),
            "verify_compliance_integrity: note values"
        );
    }

    let (
        is_regulated,
        asset_indexed_leaf,
        asset_position,
        compliance_position,
        user_leaf_var,
        is_flagged,
    ) = allocate_compliance_witnesses(cs.clone(), &witness)?;

    tracing::debug!(
        constraints_after_alloc = cs.num_constraints(),
        "verify_compliance_integrity: after allocate_compliance_witnesses"
    );

    verify_asset_registry_imt(
        cs.clone(),
        note_asset_id.clone(),
        &is_regulated,
        &asset_indexed_leaf,
        &witness.asset_path,
        &asset_position,
        &asset_anchor,
    )?;

    tracing::debug!(
        constraints_after_asset_imt = cs.num_constraints(),
        "verify_compliance_integrity: after verify_asset_registry_imt"
    );

    verify_compliance_registry_path(
        cs.clone(),
        &user_leaf_var,
        &is_regulated,
        &witness.compliance_path,
        &compliance_position,
        &compliance_anchor,
    )?;

    tracing::debug!(
        constraints_after_compliance_path = cs.num_constraints(),
        "verify_compliance_integrity: after verify_compliance_registry_path"
    );

    // Extract dk_pub and threshold from the IndexedLeaf (bound by IMT proof)
    let dk_pub = asset_indexed_leaf.policy.dk_pub.clone();
    let threshold = asset_indexed_leaf.policy.threshold.clone();

    // Verify threshold flagging: is_flagged == (amount >= threshold)
    // This ensures the prover correctly computed the flag
    verify_threshold_flag(cs.clone(), &note_amount, &threshold, &is_flagged)?;

    tracing::debug!(
        constraints_after_threshold = cs.num_constraints(),
        "verify_compliance_integrity: after verify_threshold_flag"
    );

    // For regulated assets, use the user's ACK from the compliance leaf.
    // For unregulated assets, any value works since encryption verification is skipped.
    let target_ack = user_leaf_var.key.clone();

    // Derive daily public keys (detection key unused - detection uses issuer's dk_pub)
    let (_pk_detection, pk_core, pk_extension) = target_ack.derive_all_daily_public_keys(
        cs.clone(),
        &target_date,
        &note_diversified_generator,
    )?;

    tracing::debug!(
        constraints_after_derive_daily = cs.num_constraints(),
        "verify_compliance_integrity: after derive_all_daily_public_keys"
    );

    // Derive shared secrets with conditional encryption based on flagging:
    // - Detection tier: always to issuer (allows issuer to scan)
    // - Core+Extension tiers: to issuer if flagged, to user if not flagged
    let (ss_detection, ss_core, ss_extension) = derive_all_shared_secrets_with_flagging(
        cs.clone(),
        compliance_ephemeral_secret,
        &pk_core,
        &pk_extension,
        &dk_pub,
        &is_flagged,
        &note_diversified_generator,
        &compliance_epk,
        &compliance_epk_g,
    )?;

    tracing::debug!(
        constraints_after_shared_secrets = cs.num_constraints(),
        "verify_compliance_integrity: after derive_all_shared_secrets_with_flagging"
    );

    verify_tiered_poseidon_encryption(
        cs.clone(),
        &is_regulated,
        &is_flagged,
        &ss_detection,
        &ss_core,
        &ss_extension,
        &compliance_epk,
        note_amount,
        note_asset_id,
        note_diversified_generator,
        note_transmission_key,
        counterparty_diversified_generator,
        counterparty_transmission_key,
        &compliance_ciphertext,
    )?;

    tracing::debug!(
        constraints_after_encryption = cs.num_constraints(),
        "verify_compliance_integrity: after verify_tiered_poseidon_encryption"
    );

    Ok(())
}

/// Allocate witness variables for compliance verification.
fn allocate_compliance_witnesses(
    cs: ConstraintSystemRef<Fq>,
    witness: &ComplianceWitness,
) -> Result<
    (
        Boolean<Fq>,
        IndexedLeafVar,
        FqVar,
        FqVar,
        ComplianceLeafVar,
        Boolean<Fq>,
    ),
    SynthesisError,
> {
    let is_regulated = Boolean::new_witness(cs.clone(), || Ok(witness.is_regulated))?;
    let asset_indexed_leaf = IndexedLeafVar::new_witness(cs.clone(), &witness.asset_indexed_leaf)?;
    let asset_position = FqVar::new_witness(cs.clone(), || Ok(Fq::from(witness.asset_position)))?;
    let compliance_position =
        FqVar::new_witness(cs.clone(), || Ok(Fq::from(witness.compliance_position)))?;
    let is_flagged = Boolean::new_witness(cs.clone(), || Ok(witness.is_flagged))?;
    let user_leaf_var =
        ComplianceLeafVar::new_variable(cs, || Ok(&witness.user_leaf), AllocationMode::Witness)?;

    Ok((
        is_regulated,
        asset_indexed_leaf,
        asset_position,
        compliance_position,
        user_leaf_var,
        is_flagged,
    ))
}

/// Compare two FqVar values using bit decomposition.
/// Returns `Boolean<Fq>` that is true if `a < b`.
///
/// This works for ALL field elements, unlike `is_cmp` which only works
/// for values < (p-1)/2. Since asset IDs are 256-bit hashes, they can
/// exceed this limit, so we must use bit decomposition.
///
/// Algorithm: Decompose both values to bits, compare MSB to LSB.
/// Based on `U128x128Var::enforce_cmp` in fixpoint.rs.
fn fq_is_less_than(a: &FqVar, b: &FqVar) -> Result<Boolean<Fq>, SynthesisError> {
    use std::iter::zip;

    // Decompose to bits (little-endian), then reverse for MSB-first comparison
    let a_bits: Vec<Boolean<Fq>> = a.to_bits_le()?.into_iter().rev().collect();
    let b_bits: Vec<Boolean<Fq>> = b.to_bits_le()?.into_iter().rev().collect();

    // Track comparison state from MSB to LSB
    // gt = true if we've conclusively determined a > b
    // lt = true if we've conclusively determined a < b
    let mut gt: Boolean<Fq> = Boolean::constant(false);
    let mut lt: Boolean<Fq> = Boolean::constant(false);

    for (p, q) in zip(a_bits, b_bits) {
        // If we see a=1, b=0 and haven't determined lt yet, then a > b
        gt = gt.or(&lt.not().and(&p)?.and(&q.not())?)?;
        // If we see a=0, b=1 and haven't determined gt yet, then a < b
        lt = lt.or(&gt.not().and(&q)?.and(&p.not())?)?;
    }

    Ok(lt)
}

/// Verify that `is_flagged` witness matches the threshold comparison.
///
/// Enforces: `is_flagged == (amount >= threshold)`
/// This is equivalent to: `is_flagged == !(amount < threshold)`
fn verify_threshold_flag(
    _cs: ConstraintSystemRef<Fq>,
    amount: &FqVar,
    threshold: &FqVar,
    is_flagged: &Boolean<Fq>,
) -> Result<(), SynthesisError> {
    // Debug: log the values being compared
    if let (Ok(amt), Ok(thresh), Ok(flagged)) =
        (amount.value(), threshold.value(), is_flagged.value())
    {
        tracing::debug!(
            amount = ?amt.to_bytes(),
            threshold = ?thresh.to_bytes(),
            is_flagged_witness = flagged,
            "verify_threshold_flag: comparing amount vs threshold"
        );
    }

    // Compute: amount < threshold
    let amount_lt_threshold = fq_is_less_than(amount, threshold)?;

    // Debug: log computed comparison result
    if let Ok(lt) = amount_lt_threshold.value() {
        tracing::debug!(
            amount_lt_threshold = lt,
            computed_is_flagged = !lt,
            "verify_threshold_flag: computed comparison"
        );
    }

    // Compute: amount >= threshold = !(amount < threshold)
    let computed_is_flagged = amount_lt_threshold.not();
    // Enforce: witness is_flagged matches computed value
    is_flagged.enforce_equal(&computed_is_flagged)?;
    Ok(())
}

/// Derive shared secrets with conditional encryption based on flagging.
///
/// - Detection tier: always to issuer's DK_pub (allows issuer to scan for their asset)
/// - Core+Extension tiers: to issuer's DK_pub if flagged, to user's daily keys if not
fn derive_all_shared_secrets_with_flagging(
    cs: ConstraintSystemRef<Fq>,
    compliance_ephemeral_secret: Fr,
    pk_core: &ElementVar,
    pk_extension: &ElementVar,
    dk_pub: &ElementVar,
    is_flagged: &Boolean<Fq>,
    note_diversified_generator: &ElementVar,
    compliance_epk: &ElementVar,
    compliance_epk_g: &ElementVar,
) -> Result<(ElementVar, ElementVar, ElementVar), SynthesisError> {
    use ark_ff::{BigInteger, PrimeField};

    // Convert ephemeral secret to bits for scalar multiplication
    let esk_bigint = compliance_ephemeral_secret.into_bigint();
    let esk_bits: Vec<bool> = (0..Fr::MODULUS_BIT_SIZE)
        .map(|i| esk_bigint.get_bit(i as usize))
        .collect();
    let esk_bits_vars: Vec<Boolean<Fq>> = esk_bits
        .iter()
        .map(|bit| Boolean::new_witness(cs.clone(), || Ok(*bit)))
        .collect::<Result<Vec<_>, _>>()?;

    // Verify EPK = r * B_d
    let computed_epk = note_diversified_generator.scalar_mul_le(esk_bits_vars.iter())?;
    computed_epk.enforce_equal(compliance_epk)?;

    // Verify EPK_G = r * G
    let generator_var = ElementVar::new_constant(cs.clone(), decaf377::Element::GENERATOR)?;
    let computed_epk_g = generator_var.scalar_mul_le(esk_bits_vars.iter())?;
    computed_epk_g.enforce_equal(compliance_epk_g)?;

    // Compute user shared secrets: S_type = r * PK_type
    let ss_core_user = pk_core.scalar_mul_le(esk_bits_vars.iter())?;
    let ss_extension_user = pk_extension.scalar_mul_le(esk_bits_vars.iter())?;

    // Compute issuer shared secret: S_issuer = r * DK_pub
    let ss_issuer = dk_pub.scalar_mul_le(esk_bits_vars.iter())?;

    // Detection tier always uses issuer's key (allows issuer to scan for their asset)
    let ss_detection = ss_issuer.clone();

    // Core+Extension tiers: conditionally select based on is_flagged
    // If flagged: use issuer's shared secret
    // If not flagged: use user's shared secret
    let ss_core = ElementVar::conditionally_select(is_flagged, &ss_issuer, &ss_core_user)?;
    let ss_extension =
        ElementVar::conditionally_select(is_flagged, &ss_issuer, &ss_extension_user)?;

    Ok((ss_detection, ss_core, ss_extension))
}

/// Verify asset registry using Indexed Merkle Tree (IMT).
///
/// For **regulated** assets: membership proof - `indexed_leaf.value == asset_id`
/// For **unregulated** assets: non-membership proof - `indexed_leaf.value < asset_id < indexed_leaf.next_value`
///
/// Both proof types use identical constraint counts for circuit indistinguishability.
fn verify_asset_registry_imt(
    cs: ConstraintSystemRef<Fq>,
    note_asset_id: FqVar,
    is_regulated: &Boolean<Fq>,
    indexed_leaf: &IndexedLeafVar,
    asset_path: &MerklePath,
    asset_position: &FqVar,
    asset_anchor: &FqVar,
) -> Result<(), SynthesisError> {
    // 1. Compute leaf commitment
    let leaf_commitment = indexed_leaf.commit(cs.clone())?;

    // Debug: log circuit-computed leaf commitment value
    if let Ok(circuit_commit_value) = leaf_commitment.value() {
        tracing::debug!(
            circuit_leaf_commitment = ?circuit_commit_value.to_bytes(),
            "verify_asset_registry_imt: circuit-computed leaf commitment"
        );
    }

    // 2. Verify Merkle path (reuse existing verify_quad_path)
    let calculated_root = verify_quad_path(
        cs.clone(),
        leaf_commitment,
        asset_path,
        asset_position.clone(),
    )?;

    // Debug: log circuit-computed root vs anchor
    if let (Ok(calc_root), Ok(anchor)) = (calculated_root.value(), asset_anchor.value()) {
        let matches = calc_root == anchor;
        tracing::debug!(
            circuit_calculated_root = ?calc_root.to_bytes(),
            expected_anchor = ?anchor.to_bytes(),
            root_matches = matches,
            "verify_asset_registry_imt: circuit root vs anchor"
        );
    }

    calculated_root.enforce_equal(asset_anchor)?;

    // 3. Membership check: asset_id == leaf.value
    let is_exact_match = note_asset_id.is_eq(&indexed_leaf.value)?;

    // 4. Non-membership (gap) check: leaf.value < asset_id < leaf.next_value
    //
    // Uses bit decomposition comparison which works for ALL field elements,
    // unlike is_cmp which only works for values < (p-1)/2.
    let gt_low = fq_is_less_than(&indexed_leaf.value, &note_asset_id)?; // leaf.value < asset_id
    let lt_high = fq_is_less_than(&note_asset_id, &indexed_leaf.next_value)?; // asset_id < leaf.next_value
    let is_in_gap = gt_low.and(&lt_high)?;

    // 5. Select based on regulation status:
    // - Regulated (is_regulated=true): must have exact match (membership)
    // - Unregulated (is_regulated=false): must be in gap (non-membership)
    let valid_proof = is_regulated.select(&is_exact_match, &is_in_gap)?;
    valid_proof.enforce_equal(&Boolean::TRUE)?;

    Ok(())
}

/// Verify compliance registry Merkle path (user tree).
///
/// For regulated assets: enforces the user is registered in the compliance tree.
/// For unregulated assets: enforcement is skipped (any witness accepted).
fn verify_compliance_registry_path(
    cs: ConstraintSystemRef<Fq>,
    user_leaf_var: &ComplianceLeafVar,
    is_regulated: &Boolean<Fq>,
    compliance_path: &MerklePath,
    compliance_position: &FqVar,
    compliance_anchor: &FqVar,
) -> Result<(), SynthesisError> {
    let user_leaf_commitment = user_leaf_var.commit(cs.clone())?;

    let calculated_user_root = verify_quad_path(
        cs.clone(),
        user_leaf_commitment,
        compliance_path,
        compliance_position.clone(),
    )?;

    // Conditional enforcement: only if regulated
    // Note: Removed redundant anchor != 0 check - validators reject invalid anchors
    calculated_user_root.conditional_enforce_equal(compliance_anchor, is_regulated)?;

    Ok(())
}

/// Verify tiered Poseidon stream cipher encryption with 3 different keys.
///
/// Each ciphertext segment is encrypted with its own shared secret:
/// - detection_tag: encrypted with ss_detection (1 Fq)
/// - encrypted_core: encrypted with ss_core (3 Fqs)
/// - encrypted_extension: encrypted with ss_extension (3 Fqs)
///
/// Total: 7 Fq elements in compliance_ciphertext
///
/// Encryption verification is conditional on `is_regulated`:
/// - For regulated assets: encryption is verified (user must encrypt correctly)
/// - For unregulated assets: enforcement is skipped (IMT proof is sufficient)
fn verify_tiered_poseidon_encryption(
    cs: ConstraintSystemRef<Fq>,
    is_regulated: &Boolean<Fq>,
    is_flagged: &Boolean<Fq>,
    ss_detection: &ElementVar,
    ss_core: &ElementVar,
    ss_extension: &ElementVar,
    compliance_epk: &ElementVar,
    note_amount: FqVar,
    note_asset_id: FqVar,
    self_diversified_generator: ElementVar,
    self_transmission_key: ElementVar,
    counterparty_diversified_generator: ElementVar,
    counterparty_transmission_key: ElementVar,
    compliance_ciphertext: &[FqVar],
) -> Result<(), SynthesisError> {
    // Expected ciphertext layout: [detection: 1] [core: 3] [extension: 3] = 7 Fqs
    if compliance_ciphertext.len() != 7 {
        return Err(SynthesisError::Unsatisfiable);
    }

    let epk_fq = compliance_epk.compress_to_field()?;
    let domain_sep =
        FqVar::new_constant(cs.clone(), *crate::crypto::COMPLIANCE_STREAM_CIPHER_DOMAIN)?;
    let issuer_domain_sep =
        FqVar::new_constant(cs.clone(), *crate::crypto::ISSUER_DETECTION_DOMAIN)?;

    // Build plaintext structure
    let plaintext_var = CompliancePlaintextVar {
        amount: note_amount,
        asset_id: note_asset_id,
        self_diversified_generator,
        self_transmission_key,
        counterparty_diversified_generator,
        counterparty_transmission_key,
    };

    // === DETECTION: 1 Fq element (asset_id with flag) ===
    // Detection tier uses ISSUER_DETECTION_DOMAIN and ss_issuer (via ss_detection)
    let seed_detection = poseidon377::r1cs::hash_2(
        cs.clone(),
        &issuer_domain_sep,
        (ss_detection.compress_to_field()?, epk_fq.clone()),
    )?;

    // Detection plaintext = asset_id + (is_flagged ? 2^252 : 0)
    // This packs the flag into bit 252 like the native code does
    let flag_bit_value = {
        use ark_ff::{BigInteger, BigInteger256};
        let mut big = BigInteger256::from(1u64);
        for _ in 0..252 {
            big.mul2();
        }
        Fq::from(big)
    };
    let flag_bit_var = FqVar::new_constant(cs.clone(), flag_bit_value)?;
    let flag_contribution = FqVar::conditionally_select(is_flagged, &flag_bit_var, &FqVar::zero())?;
    let detection_plaintext = &plaintext_var.detection_plaintext() + &flag_contribution;

    let detection_counter = FqVar::new_constant(cs.clone(), Fq::from(0u64))?;
    let detection_keystream = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (detection_counter, seed_detection.clone()),
    )?;
    let computed_detection = &detection_plaintext + &detection_keystream;
    computed_detection.conditional_enforce_equal(&compliance_ciphertext[0], is_regulated)?;

    // === CORE: 3 Fq elements (amount + self address) ===
    let seed_core = poseidon377::r1cs::hash_2(
        cs.clone(),
        &domain_sep,
        (ss_core.compress_to_field()?, epk_fq.clone()),
    )?;

    let core_plaintexts = plaintext_var.core_plaintext_fqs()?;
    for (i, plain_var) in core_plaintexts.iter().enumerate() {
        let counter = FqVar::new_constant(cs.clone(), Fq::from(i as u64))?;
        let keystream =
            poseidon377::r1cs::hash_2(cs.clone(), &seed_core, (counter, seed_core.clone()))?;
        let computed_cipher = plain_var + &keystream;
        computed_cipher.conditional_enforce_equal(&compliance_ciphertext[1 + i], is_regulated)?;
    }

    // === EXTENSION: 3 Fq elements (counterparty address) ===
    let seed_extension = poseidon377::r1cs::hash_2(
        cs.clone(),
        &domain_sep,
        (ss_extension.compress_to_field()?, epk_fq),
    )?;

    let extension_plaintexts = plaintext_var.extension_plaintext_fqs()?;
    for (i, plain_var) in extension_plaintexts.iter().enumerate() {
        let counter = FqVar::new_constant(cs.clone(), Fq::from(i as u64))?;
        let keystream = poseidon377::r1cs::hash_2(
            cs.clone(),
            &seed_extension,
            (counter, seed_extension.clone()),
        )?;
        let computed_cipher = plain_var + &keystream;
        computed_cipher.conditional_enforce_equal(&compliance_ciphertext[4 + i], is_regulated)?;
    }

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexedMerkleTree;
    use ark_relations::r1cs::ConstraintSystem;
    use decaf377::{Element, Fr};
    use penumbra_sdk_asset::asset;
    use penumbra_sdk_keys::keys::{Diversifier, UserComplianceKey};
    use penumbra_sdk_keys::Address;
    use rand_core::OsRng;

    /// Test: AddressComplianceKeyVar allocation and daily key derivation
    #[test]
    fn test_wallet_compliance_key_var_derivation() {
        let mut rng = OsRng;
        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create a master key and derive a wallet key
        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);
        let diversifier = Diversifier([7u8; 16]);
        let ack = uck.derive_address_key(&diversifier);

        // Allocate ACK in the circuit
        let ack_var =
            AddressComplianceKeyVar::new_variable(cs.clone(), || Ok(&ack), AllocationMode::Witness)
                .expect("allocation should succeed");

        // Allocate date
        let date = 19000u64;
        let date_var = FqVar::new_witness(cs.clone(), || Ok(Fq::from(date)))
            .expect("date allocation should succeed");

        // Get diversified generator
        let div_gen = diversifier.diversified_generator();
        let div_gen_var = ElementVar::new_witness(cs.clone(), || Ok(div_gen))
            .expect("div gen allocation should succeed");

        // Derive daily public key in circuit using Core domain
        let pk_day_var = ack_var
            .derive_daily_public_key_with_domain(cs.clone(), &date_var, &div_gen_var, &CORE_DOMAIN)
            .expect("derivation should succeed");

        // Verify the circuit is satisfied
        assert!(cs.is_satisfied().unwrap(), "Circuit should be satisfied");

        // Compute the expected value natively using Core key type
        let pk_day_native =
            ack.derive_daily_public_key(penumbra_sdk_keys::keys::KeyType::Core, date, &diversifier);

        // Get the value from the circuit variable
        let pk_day_circuit = pk_day_var.value().expect("should have value");

        // They should match!
        assert_eq!(
            pk_day_circuit, pk_day_native,
            "Circuit and native derivation should match"
        );
    }

    /// Test: ComplianceLeafVar commitment matches native implementation
    #[test]
    fn test_compliance_leaf_var_commit() {
        let mut rng = OsRng;
        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create a compliance leaf
        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);
        let diversifier = Diversifier([1u8; 16]);
        let ack = uck.derive_address_key(&diversifier);

        // Create a test address
        let scalar = Fr::rand(&mut rng);
        let point = Element::GENERATOR * scalar;
        let pk_d = decaf377_ka::Public(point.vartime_compress().0);
        let mut ck_d_bytes = [0u8; 32];
        use rand_core::RngCore;
        rng.fill_bytes(&mut ck_d_bytes);
        let ck_d = decaf377_fmd::ClueKey(ck_d_bytes);
        let address = Address::from_components(diversifier, pk_d, ck_d).expect("valid address");

        let leaf = ComplianceLeaf {
            address: address,
            key: ack,
            asset_id: asset::Id(Fq::from(42u64)),
        };

        // Compute native commitment
        let native_commitment = leaf.commit();

        // Allocate leaf in circuit
        let leaf_var =
            ComplianceLeafVar::new_variable(cs.clone(), || Ok(&leaf), AllocationMode::Witness)
                .expect("leaf allocation should succeed");

        // Compute commitment in circuit
        let circuit_commitment = leaf_var
            .commit(cs.clone())
            .expect("commitment should succeed");

        // Verify circuit is satisfied
        assert!(cs.is_satisfied().unwrap(), "Circuit should be satisfied");

        // Get the value from the circuit
        let circuit_commitment_value = circuit_commitment.value().expect("should have value");

        // They should match!
        assert_eq!(
            circuit_commitment_value, native_commitment.0,
            "Circuit and native commitments should match"
        );
    }

    /// Test: verify_compliance_encryption gadget
    #[test]
    fn test_verify_compliance_encryption() {
        let mut rng = OsRng;
        let cs = ConstraintSystem::<Fq>::new_ref();

        // Setup: Create keys and derive daily public key
        let uck_scalar = Fr::rand(&mut rng);
        let uck = UserComplianceKey::new(uck_scalar);
        let diversifier = Diversifier([3u8; 16]);
        let ack = uck.derive_address_key(&diversifier);

        let date = 19000u64;
        let pk_day =
            ack.derive_daily_public_key(penumbra_sdk_keys::keys::KeyType::Core, date, &diversifier);

        // Generate ephemeral secret and compute ECDH
        let ephemeral_secret = Fr::rand(&mut rng);
        let div_gen = diversifier.diversified_generator();
        let epk = div_gen * ephemeral_secret; // R = r * B_d
        let shared_secret = pk_day * ephemeral_secret; // S = r * PK_day

        // Allocate everything in the circuit
        let ephemeral_secret_var = FqVar::new_witness(cs.clone(), || {
            Ok(Fq::from_le_bytes_mod_order(&ephemeral_secret.to_bytes()))
        })
        .expect("ephemeral secret allocation");

        let pk_day_var =
            ElementVar::new_input(cs.clone(), || Ok(pk_day)).expect("pk_day allocation");

        let div_gen_var =
            ElementVar::new_input(cs.clone(), || Ok(div_gen)).expect("div gen allocation");

        let epk_var = ElementVar::new_input(cs.clone(), || Ok(epk)).expect("epk allocation");

        let shared_secret_var = ElementVar::new_witness(cs.clone(), || Ok(shared_secret))
            .expect("shared secret allocation");

        // Run the verification gadget
        verify_compliance_encryption(
            cs.clone(),
            &ephemeral_secret_var,
            &pk_day_var,
            &div_gen_var,
            &epk_var,
            &shared_secret_var,
        )
        .expect("verification should succeed");

        // Verify the circuit is satisfied
        assert!(cs.is_satisfied().unwrap(), "Circuit should be satisfied");
    }

    /// Test: verify_quad_path alone with IMT hashing
    #[test]
    fn test_verify_quad_path_with_imt() {
        use crate::IndexedMerkleTree;

        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create an IMT (empty except sentinel)
        let tree = IndexedMerkleTree::new();

        // Get proof for sentinel at position 0
        let (position, indexed_leaf, auth_path) = tree
            .non_membership_proof(Fq::from(999u64))
            .expect("proof should succeed");

        // Verify native proof first
        let anchor = tree.root();
        assert!(
            IndexedMerkleTree::verify_auth_path(position, &indexed_leaf, &auth_path, anchor, 16),
            "Native auth path should verify"
        );

        // Compute leaf commitment natively
        let leaf_commitment = indexed_leaf.commit();

        // Allocate in circuit
        let leaf_var =
            FqVar::new_witness(cs.clone(), || Ok(leaf_commitment.0)).expect("leaf allocation");
        let position_var =
            FqVar::new_witness(cs.clone(), || Ok(Fq::from(position))).expect("position allocation");
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(anchor.0)).expect("anchor allocation");

        // Convert auth path to MerklePath
        let merkle_path = MerklePath::from_auth_path(auth_path);

        // Run verify_quad_path
        let computed_root = verify_quad_path(cs.clone(), leaf_var, &merkle_path, position_var)
            .expect("path verification should succeed");

        // Enforce computed root equals anchor
        computed_root.enforce_equal(&anchor_var).expect("enforce");

        assert!(
            cs.is_satisfied().unwrap(),
            "Circuit should be satisfied for quad path"
        );
    }

    /// Test: IndexedLeafVar commitment matches native IndexedLeaf::commit
    #[test]
    fn test_indexed_leaf_commit_native_vs_circuit() {
        use crate::indexed_tree::{IndexedLeaf, FQ_MAX};

        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create a test leaf (sentinel)
        let leaf = IndexedLeaf {
            value: Fq::from(0u64),
            next_index: 0,
            next_value: *FQ_MAX,
            policy: crate::AssetPolicy::default_unregulated(),
        };

        // Compute native commitment
        let native_commit = leaf.commit();

        // Allocate in circuit and compute
        let leaf_var = IndexedLeafVar::new_witness(cs.clone(), &leaf).unwrap();
        let circuit_commit = leaf_var.commit(cs.clone()).unwrap();

        // Compare values
        let circuit_value = circuit_commit.value().unwrap();

        assert_eq!(
            native_commit.0, circuit_value,
            "Native and circuit commitments must match"
        );

        assert!(cs.is_satisfied().unwrap(), "Circuit should be satisfied");
    }

    /// Test: verify_quad_path with circuit-computed leaf commitment
    #[test]
    fn test_verify_quad_path_with_circuit_computed_leaf() {
        use crate::IndexedMerkleTree;

        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create an IMT
        let tree = IndexedMerkleTree::new();

        // Get proof for sentinel at position 0
        let (position, indexed_leaf, auth_path) = tree
            .non_membership_proof(Fq::from(999u64))
            .expect("proof should succeed");

        let anchor = tree.root();

        // Allocate IndexedLeafVar and compute commitment IN-CIRCUIT
        let indexed_leaf_var =
            IndexedLeafVar::new_witness(cs.clone(), &indexed_leaf).expect("leaf allocation");
        let leaf_commitment = indexed_leaf_var.commit(cs.clone()).expect("commitment");

        let position_var =
            FqVar::new_witness(cs.clone(), || Ok(Fq::from(position))).expect("position allocation");
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(anchor.0)).expect("anchor allocation");

        // Convert auth path to MerklePath
        let merkle_path = MerklePath::from_auth_path(auth_path);

        // Run verify_quad_path with circuit-computed leaf
        let computed_root =
            verify_quad_path(cs.clone(), leaf_commitment, &merkle_path, position_var)
                .expect("path verification should succeed");

        // Enforce computed root equals anchor
        computed_root.enforce_equal(&anchor_var).expect("enforce");

        assert!(
            cs.is_satisfied().unwrap(),
            "Circuit should be satisfied for quad path with circuit-computed leaf"
        );
    }

    /// Test: verify_asset_registry_imt gadget with membership proof (regulated asset)
    #[test]
    fn test_verify_asset_registry_imt_membership() {
        use crate::IndexedMerkleTree;

        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create an IMT and insert a regulated asset
        let mut tree = IndexedMerkleTree::new();
        let asset_id = Fq::from(42u64);
        tree.insert(asset_id).expect("insert should succeed");

        // Get membership proof
        let (position, indexed_leaf, auth_path) = tree
            .membership_proof(asset_id)
            .expect("membership proof should succeed");
        let anchor = tree.root();

        // Allocate variables
        let asset_id_var =
            FqVar::new_witness(cs.clone(), || Ok(asset_id)).expect("asset_id allocation");
        let is_regulated = Boolean::new_witness(cs.clone(), || Ok(true)).expect("is_regulated");
        let indexed_leaf_var =
            IndexedLeafVar::new_witness(cs.clone(), &indexed_leaf).expect("leaf allocation");
        let position_var =
            FqVar::new_witness(cs.clone(), || Ok(Fq::from(position))).expect("position allocation");
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(anchor.0)).expect("anchor allocation");

        let merkle_path = MerklePath::from_auth_path(auth_path);

        verify_asset_registry_imt(
            cs.clone(),
            asset_id_var,
            &is_regulated,
            &indexed_leaf_var,
            &merkle_path,
            &position_var,
            &anchor_var,
        )
        .expect("verification should succeed");

        assert!(
            cs.is_satisfied().unwrap(),
            "Circuit should be satisfied for membership proof"
        );
    }

    /// Test: verify_asset_registry_imt gadget with non-membership proof (unregulated asset)
    #[test]
    fn test_verify_asset_registry_imt_non_membership() {
        use crate::IndexedMerkleTree;

        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create an IMT (empty except sentinel)
        let tree = IndexedMerkleTree::new();
        let asset_id = Fq::from(999u64); // Not in tree

        // Get non-membership proof (sentinel proves the gap)
        let (position, indexed_leaf, auth_path) = tree
            .non_membership_proof(asset_id)
            .expect("non-membership proof should succeed");
        let anchor = tree.root();

        // Verify the native proof is valid first
        assert!(
            IndexedMerkleTree::verify_auth_path(position, &indexed_leaf, &auth_path, anchor, 16),
            "Native auth path should verify"
        );

        // Allocate variables
        let asset_id_var =
            FqVar::new_witness(cs.clone(), || Ok(asset_id)).expect("asset_id allocation");
        let is_regulated = Boolean::new_witness(cs.clone(), || Ok(false)).expect("is_regulated");
        let indexed_leaf_var =
            IndexedLeafVar::new_witness(cs.clone(), &indexed_leaf).expect("leaf allocation");
        let position_var =
            FqVar::new_witness(cs.clone(), || Ok(Fq::from(position))).expect("position allocation");
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(anchor.0)).expect("anchor allocation");

        let merkle_path = MerklePath::from_auth_path(auth_path);

        verify_asset_registry_imt(
            cs.clone(),
            asset_id_var,
            &is_regulated,
            &indexed_leaf_var,
            &merkle_path,
            &position_var,
            &anchor_var,
        )
        .expect("verification should succeed");

        assert!(
            cs.is_satisfied().unwrap(),
            "Circuit should be satisfied for non-membership proof"
        );
    }

    /// Test: verify_asset_registry_imt gadget with REAL staking token asset ID
    /// This tests whether is_cmp works correctly with large field elements (256-bit hashes)
    #[test]
    fn test_verify_asset_registry_imt_non_membership_large_asset_id() {
        use crate::IndexedMerkleTree;
        use penumbra_sdk_asset::STAKING_TOKEN_ASSET_ID;

        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create an IMT (empty except sentinel)
        let tree = IndexedMerkleTree::new();
        let asset_id = STAKING_TOKEN_ASSET_ID.0; // Real 256-bit hash value

        eprintln!("Testing with STAKING_TOKEN_ASSET_ID: {:?}", asset_id);

        // Get non-membership proof (sentinel proves the gap)
        let (position, indexed_leaf, auth_path) = tree
            .non_membership_proof(asset_id)
            .expect("non-membership proof should succeed");
        let anchor = tree.root();

        eprintln!(
            "Proof: position={}, leaf.value={:?}, leaf.next_value={:?}",
            position, indexed_leaf.value, indexed_leaf.next_value
        );

        // Verify the native proof is valid first
        assert!(
            IndexedMerkleTree::verify_auth_path(position, &indexed_leaf, &auth_path, anchor, 16),
            "Native auth path should verify"
        );

        // Allocate variables
        let asset_id_var =
            FqVar::new_witness(cs.clone(), || Ok(asset_id)).expect("asset_id allocation");
        let is_regulated = Boolean::new_witness(cs.clone(), || Ok(false)).expect("is_regulated");
        let indexed_leaf_var =
            IndexedLeafVar::new_witness(cs.clone(), &indexed_leaf).expect("leaf allocation");
        let position_var =
            FqVar::new_witness(cs.clone(), || Ok(Fq::from(position))).expect("position allocation");
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(anchor.0)).expect("anchor allocation");

        let merkle_path = MerklePath::from_auth_path(auth_path);

        verify_asset_registry_imt(
            cs.clone(),
            asset_id_var,
            &is_regulated,
            &indexed_leaf_var,
            &merkle_path,
            &position_var,
            &anchor_var,
        )
        .expect("verification should succeed");

        let satisfied = cs.is_satisfied().unwrap();
        eprintln!("Circuit satisfied: {}", satisfied);
        if !satisfied {
            eprintln!("Unsatisfied constraints:");
            for (i, constraint) in cs.which_is_unsatisfied().unwrap().iter().enumerate() {
                eprintln!("  {}: {:?}", i, constraint);
            }
        }

        assert!(
            satisfied,
            "Circuit should be satisfied for non-membership proof with large asset_id"
        );
    }

    /// Test that invalid membership proof (wrong asset_id) fails circuit
    #[test]
    fn test_invalid_membership_fails_circuit() {
        let cs = ConstraintSystem::<Fq>::new_ref();

        let mut tree = IndexedMerkleTree::new();
        let real_asset = Fq::from(42u64);
        tree.insert(real_asset).unwrap();

        // Get valid proof for real_asset
        let (position, indexed_leaf, auth_path) =
            tree.membership_proof(real_asset).expect("membership proof");
        let merkle_path = MerklePath::from_auth_path(auth_path);

        // But claim it's for a DIFFERENT asset_id
        let fake_asset = Fq::from(999u64);
        let asset_id_var = FqVar::new_witness(cs.clone(), || Ok(fake_asset)).unwrap();
        let is_regulated = Boolean::new_witness(cs.clone(), || Ok(true)).unwrap();
        let indexed_leaf_var = IndexedLeafVar::new_witness(cs.clone(), &indexed_leaf).unwrap();
        let position_var = FqVar::new_witness(cs.clone(), || Ok(Fq::from(position))).unwrap();
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(tree.root().0)).unwrap();

        verify_asset_registry_imt(
            cs.clone(),
            asset_id_var,
            &is_regulated,
            &indexed_leaf_var,
            &merkle_path,
            &position_var,
            &anchor_var,
        )
        .unwrap();

        assert!(
            !cs.is_satisfied().unwrap(),
            "Circuit should FAIL when asset_id doesn't match leaf"
        );
    }

    /// Test that invalid non-membership proof (value not in gap) fails circuit
    #[test]
    fn test_invalid_non_membership_fails_circuit() {
        let cs = ConstraintSystem::<Fq>::new_ref();

        let mut tree = IndexedMerkleTree::new();
        tree.insert(Fq::from(100u64)).unwrap();
        tree.insert(Fq::from(300u64)).unwrap();

        // Get non-membership proof for value 200 (valid gap: 100 < 200 < 300)
        let (position, indexed_leaf, auth_path) = tree
            .non_membership_proof(Fq::from(200u64))
            .expect("non-membership proof");
        let merkle_path = MerklePath::from_auth_path(auth_path);

        // But claim it's for value 50 (NOT in this gap: 100 < 50 is false)
        let fake_value = Fq::from(50u64);
        let asset_id_var = FqVar::new_witness(cs.clone(), || Ok(fake_value)).unwrap();
        let is_regulated = Boolean::new_witness(cs.clone(), || Ok(false)).unwrap();
        let indexed_leaf_var = IndexedLeafVar::new_witness(cs.clone(), &indexed_leaf).unwrap();
        let position_var = FqVar::new_witness(cs.clone(), || Ok(Fq::from(position))).unwrap();
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(tree.root().0)).unwrap();

        verify_asset_registry_imt(
            cs.clone(),
            asset_id_var,
            &is_regulated,
            &indexed_leaf_var,
            &merkle_path,
            &position_var,
            &anchor_var,
        )
        .unwrap();

        assert!(
            !cs.is_satisfied().unwrap(),
            "Circuit should FAIL when value not in gap"
        );
    }

    /// Test that wrong anchor fails circuit
    #[test]
    fn test_wrong_anchor_fails_circuit() {
        let cs = ConstraintSystem::<Fq>::new_ref();

        let mut tree = IndexedMerkleTree::new();
        tree.insert(Fq::from(42u64)).unwrap();

        let (position, indexed_leaf, auth_path) = tree
            .membership_proof(Fq::from(42u64))
            .expect("membership proof");
        let merkle_path = MerklePath::from_auth_path(auth_path);

        let asset_id_var = FqVar::new_witness(cs.clone(), || Ok(Fq::from(42u64))).unwrap();
        let is_regulated = Boolean::new_witness(cs.clone(), || Ok(true)).unwrap();
        let indexed_leaf_var = IndexedLeafVar::new_witness(cs.clone(), &indexed_leaf).unwrap();
        let position_var = FqVar::new_witness(cs.clone(), || Ok(Fq::from(position))).unwrap();

        // Use WRONG anchor
        let wrong_anchor = Fq::from(12345u64);
        let anchor_var = FqVar::new_input(cs.clone(), || Ok(wrong_anchor)).unwrap();

        verify_asset_registry_imt(
            cs.clone(),
            asset_id_var,
            &is_regulated,
            &indexed_leaf_var,
            &merkle_path,
            &position_var,
            &anchor_var,
        )
        .unwrap();

        assert!(
            !cs.is_satisfied().unwrap(),
            "Circuit should FAIL with wrong anchor"
        );
    }
}
