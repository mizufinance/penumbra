//! R1CS gadgets for compliance verification.
//!
//! Proves compliance properties in zero-knowledge: asset registry inclusion,
//! user registry inclusion, threshold flagging, leaf↔note binding, and ciphertext integrity.
//!
//! ACK = d × ring_pk where `d` is stored in the compliance leaf (SHA256-derived, matching Orbis).
//! Single ACK per party; tier isolation via distinct EPKs.

use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};
use decaf377::r1cs::{ElementVar, FqVar};
use decaf377::{Fq, Fr};

use crate::indexed_tree::{IndexedLeaf, IMT_LEAF_DOMAIN_SEP, PARAMS_DOMAIN_SEP, RING_DOMAIN_SEP};
use crate::structs::{ComplianceLeaf, MerklePath};
use crate::tree::DEFAULT_DEPTH;

/// 1 << 253 as an Fq constant, used as the flag sentinel bit in detection ciphertext.
fn flag_bit_fq() -> Fq {
    use ark_ff::{BigInteger, BigInteger256};
    let mut big = BigInteger256::from(1u64);
    for _ in 0..253 {
        big.mul2();
    }
    Fq::from(big)
}

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
        let is_index_0 = Boolean::kary_and(&[!bit_0.clone(), !bit_1.clone()])?; // !b0 && !b1
        let is_index_1 = Boolean::kary_and(&[bit_0.clone(), !bit_1.clone()])?; // b0 && !b1
        let is_index_2 = Boolean::kary_and(&[!bit_0.clone(), bit_1.clone()])?; // !b0 && b1
        let is_index_3 = Boolean::kary_and(&[bit_0.clone(), bit_1.clone()])?; // b0 && b1

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
// Compliance Leaf R1CS Gadgets
// ============================================================================

/// R1CS variable representing a Compliance Leaf.
///
/// Contains address, asset_id, slot metadata, and derivation scalar `d`.
/// ACK = d × ring_pk, computed in-circuit.
pub struct ComplianceLeafVar {
    pub address: penumbra_sdk_keys::AddressVar,
    pub asset_id: FqVar,
    pub slot_id: FqVar,
    pub slot_derivation: FqVar,
    pub d: FqVar,
}

impl ComplianceLeafVar {
    /// Compute the Poseidon commitment.
    ///
    /// Must match `structs::ComplianceLeaf::commit()` exactly.
    pub fn commit(&self, cs: ConstraintSystemRef<Fq>) -> Result<FqVar, SynthesisError> {
        let domain_sep = FqVar::new_constant(
            cs.clone(),
            Fq::from_le_bytes_mod_order(
                blake2b_simd::blake2b(b"penumbra.compliance.leaf").as_bytes(),
            ),
        )?;

        let div_gen_fq = self.address.diversified_generator().compress_to_field()?;
        let pk_d_fq = self.address.transmission_key().compress_to_field()?;

        poseidon377::r1cs::hash_6(
            cs,
            &domain_sep,
            (
                div_gen_fq,
                pk_d_fq,
                self.asset_id.clone(),
                self.slot_id.clone(),
                self.slot_derivation.clone(),
                self.d.clone(),
            ),
        )
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

        let address = penumbra_sdk_keys::AddressVar::new_variable(
            cs.clone(),
            || Ok(&leaf_ref.address),
            mode,
        )?;
        let asset_id_fq = leaf_ref.asset_id.0;
        let asset_id = FqVar::new_variable(cs.clone(), || Ok(asset_id_fq), mode)?;
        let slot_id = FqVar::new_variable(cs.clone(), || Ok(Fq::from(leaf_ref.slot_id)), mode)?;
        let slot_derivation =
            FqVar::new_variable(cs.clone(), || Ok(leaf_ref.slot_derivation), mode)?;
        let d = FqVar::new_variable(cs, || Ok(leaf_ref.d), mode)?;

        Ok(Self {
            address,
            asset_id,
            slot_id,
            slot_derivation,
            d,
        })
    }
}

// ============================================================================
// Compliance Witness Structure
// ============================================================================

/// Witness data for compliance verification.
#[derive(Clone)]
pub struct ComplianceWitness {
    pub is_regulated: bool,
    pub asset_indexed_leaf: IndexedLeaf,
    pub asset_path: MerklePath,
    pub asset_position: u64,
    pub compliance_path: MerklePath,
    pub compliance_position: u64,
    pub user_leaf: ComplianceLeaf,
    pub is_flagged: bool,
    /// Random salt for DLEQ metadata hash, encrypted in detection tier.
    pub salt: Fq,
}

/// Circuit variable for an Indexed Merkle Tree leaf.
///
/// All policy fields are private witnesses bound by the IMT Merkle proof.
/// Sub-structured Poseidon commitment matches native IndexedLeaf::commit().
pub struct IndexedLeafVar {
    // Structural fields
    pub value: FqVar,
    pub next_index: FqVar,
    pub next_value: FqVar,
    // Penumbra-decided policy (bound by IMT proof)
    pub dk_pub: ElementVar,
    pub threshold: FqVar,
    pub slot_count: FqVar,
    pub route_policy_hash: FqVar,
    // Orbis-decided policy (bound by IMT proof)
    pub ring_pk: ElementVar,
    pub ring_id_hash: FqVar,
    pub policy_id_hash: FqVar,
    pub permission_hash: FqVar,
    pub resource_hash: FqVar,
}

impl IndexedLeafVar {
    /// Allocate all fields as witness variables.
    pub fn new_witness(
        cs: ConstraintSystemRef<Fq>,
        leaf: &IndexedLeaf,
    ) -> Result<Self, SynthesisError> {
        let value = FqVar::new_witness(cs.clone(), || Ok(leaf.value))?;
        let next_index = FqVar::new_witness(cs.clone(), || Ok(Fq::from(leaf.next_index)))?;
        let next_value = FqVar::new_witness(cs.clone(), || Ok(leaf.next_value))?;
        let dk_pub = ElementVar::new_witness(cs.clone(), || Ok(leaf.params.dk_pub))?;
        let threshold = FqVar::new_witness(cs.clone(), || Ok(Fq::from(leaf.params.threshold)))?;
        let slot_count = FqVar::new_witness(cs.clone(), || Ok(Fq::from(leaf.params.slot_count)))?;
        let route_policy_hash =
            FqVar::new_witness(cs.clone(), || Ok(leaf.params.route_policy_hash))?;
        let ring_pk = ElementVar::new_witness(cs.clone(), || Ok(leaf.ring.ring_pk))?;
        let ring_id_hash = FqVar::new_witness(cs.clone(), || Ok(leaf.ring.ring_id_hash))?;
        let policy_id_hash = FqVar::new_witness(cs.clone(), || Ok(leaf.ring.policy_id_hash))?;
        let permission_hash = FqVar::new_witness(cs.clone(), || Ok(leaf.ring.permission_hash))?;
        let resource_hash = FqVar::new_witness(cs, || Ok(leaf.ring.resource_hash))?;
        Ok(Self {
            value,
            next_index,
            next_value,
            dk_pub,
            threshold,
            slot_count,
            route_policy_hash,
            ring_pk,
            ring_id_hash,
            policy_id_hash,
            permission_hash,
            resource_hash,
        })
    }

    /// Compute the leaf commitment matching native IndexedLeaf::commit().
    ///
    /// params_hash = hash_4(PARAMS_DOMAIN, dk_pub_fq, threshold, slot_count, route_policy_hash)
    /// ring_hash   = hash_5(RING_DOMAIN, ring_pk_fq, ring_id_hash, policy_id_hash, permission_hash, resource_hash)
    /// leaf_commit = hash_5(LEAF_DOMAIN, value, next_index, next_value, params_hash, ring_hash)
    pub fn commit(&self, cs: ConstraintSystemRef<Fq>) -> Result<FqVar, SynthesisError> {
        let leaf_domain = FqVar::new_constant(cs.clone(), *IMT_LEAF_DOMAIN_SEP)?;
        let params_domain = FqVar::new_constant(cs.clone(), *PARAMS_DOMAIN_SEP)?;
        let ring_domain = FqVar::new_constant(cs.clone(), *RING_DOMAIN_SEP)?;

        // Sub-hash 1: Penumbra-decided params
        let dk_pub_fq = self.dk_pub.compress_to_field()?;
        let params_hash = poseidon377::r1cs::hash_4(
            cs.clone(),
            &params_domain,
            (
                dk_pub_fq,
                self.threshold.clone(),
                self.slot_count.clone(),
                self.route_policy_hash.clone(),
            ),
        )?;

        // Sub-hash 2: Orbis-decided ring
        let ring_pk_fq = self.ring_pk.compress_to_field()?;
        let ring_hash = poseidon377::r1cs::hash_5(
            cs.clone(),
            &ring_domain,
            (
                ring_pk_fq,
                self.ring_id_hash.clone(),
                self.policy_id_hash.clone(),
                self.permission_hash.clone(),
                self.resource_hash.clone(),
            ),
        )?;

        // Final leaf commitment
        poseidon377::r1cs::hash_5(
            cs,
            &leaf_domain,
            (
                self.value.clone(),
                self.next_index.clone(),
                self.next_value.clone(),
                params_hash,
                ring_hash,
            ),
        )
    }
}

/// Derive ACK from the leaf's `d` scalar and ring_pk: `ACK = d × ring_pk`.
fn derive_ack_from_leaf_d(ring_pk: &ElementVar, d: &FqVar) -> Result<ElementVar, SynthesisError> {
    let d_bits = d.to_bits_le()?;
    ring_pk.scalar_mul_le(d_bits.iter())
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
            let fq_var = Boolean::le_bits_to_fp(chunk)?;
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
            let fq_var = Boolean::le_bits_to_fp(chunk)?;
            fqs.push(fq_var);
        }

        Ok(fqs)
    }
}

// ============================================================================
// Unified Compliance Integrity Verification
// ============================================================================

/// Verify compliance integrity for an output action.
///
/// Three independent ephemeral scalars (r_1, r_2, r_3) with EPKs on standard generator G.
/// Core/ext tiers use receiver's ACK, sext tier uses sender's ACK (counterparty disclosure).
/// Policy fields (dk_pub, threshold, ring_pk) are private witnesses bound by the IMT proof.
pub fn verify_compliance_integrity(
    cs: ConstraintSystemRef<Fq>,
    // Public Inputs
    asset_anchor: FqVar,
    compliance_anchor: FqVar,
    epk_1: ElementVar,
    epk_2: ElementVar,
    epk_3: ElementVar,
    c2_core: FqVar,
    c2_ext: FqVar,
    c2_sext: FqVar,
    compliance_ciphertext: Vec<FqVar>, // 13 Fqs: detection(4) + core(3) + ext(3) + sext(3)
    // DLEQ public inputs (3 tiers × 2 = 6 Fq values, stored as Fq but canonical Fr)
    target_timestamp: FqVar,
    dleq_c_1: FqVar,
    dleq_s_1: FqVar,
    dleq_c_2: FqVar,
    dleq_s_2: FqVar,
    dleq_c_3: FqVar,
    dleq_s_3: FqVar,
    // Note data
    note_asset_id: FqVar,
    note_amount: FqVar,
    note_diversified_generator: ElementVar,
    note_transmission_key: ElementVar,
    counterparty_diversified_generator: ElementVar,
    counterparty_transmission_key: ElementVar,
    // Witness scalars
    r_1: Fr,
    r_2: Fr,
    r_3: Fr,
    // Counterparty's d scalar (already allocated from counterparty_leaf_var)
    counterparty_d: FqVar,
    counterparty_slot_id: FqVar,
    witness: ComplianceWitness,
) -> Result<Vec<Boolean<Fq>>, SynthesisError> {
    let (
        is_regulated,
        asset_indexed_leaf,
        asset_position,
        compliance_position,
        user_leaf_var,
        is_flagged,
    ) = allocate_compliance_witnesses(cs.clone(), &witness)?;

    verify_asset_registry_imt(
        cs.clone(),
        note_asset_id.clone(),
        &is_regulated,
        &asset_indexed_leaf,
        &witness.asset_path,
        &asset_position,
        &asset_anchor,
    )?;

    verify_path(
        cs.clone(),
        &user_leaf_var,
        &is_regulated,
        &witness.compliance_path,
        &compliance_position,
        &compliance_anchor,
    )?;

    // Leaf↔note binding: the Merkle-proven leaf must match the note's address and asset.
    let leaf_g_d = user_leaf_var.address.diversified_generator();
    let leaf_pk_d = user_leaf_var.address.transmission_key();
    leaf_g_d.enforce_equal(&note_diversified_generator)?;
    leaf_pk_d.enforce_equal(&note_transmission_key)?;
    user_leaf_var.asset_id.enforce_equal(&note_asset_id)?;

    // Policy fields extracted from IndexedLeafVar (bound by IMT proof)
    verify_threshold_flag_simple(
        cs.clone(),
        &note_amount,
        &asset_indexed_leaf.threshold,
        &is_flagged,
    )?;

    // ACK per party from leaf's `d` scalar: ACK = d × ring_pk
    let ack_receiver = derive_ack_from_leaf_d(&asset_indexed_leaf.ring_pk, &user_leaf_var.d)?;
    let ack_sender = derive_ack_from_leaf_d(&asset_indexed_leaf.ring_pk, &counterparty_d)?;

    // Derive shared secrets with 3 independent ephemeral scalars
    let (ss_detection, ss_core, ss_ext, ss_sext, r1_bits, r2_bits, r3_bits) =
        derive_shared_secrets_output(
            cs.clone(),
            r_1,
            r_2,
            r_3,
            &ack_receiver,
            &ack_sender,
            &asset_indexed_leaf.dk_pub,
            &is_flagged,
            &epk_1,
            &epk_2,
            &epk_3,
        )?;

    // Salt witness for detection tier
    let salt_var = FqVar::new_witness(cs.clone(), || Ok(witness.salt))?;

    verify_poseidon_encryption(
        cs.clone(),
        &is_regulated,
        &is_flagged,
        &ss_detection,
        &ss_core,
        &ss_ext,
        &ss_sext,
        &c2_core,
        &c2_ext,
        &c2_sext,
        &epk_1,
        &salt_var,
        &counterparty_slot_id,
        &user_leaf_var.slot_id,
        note_amount,
        note_asset_id,
        note_diversified_generator,
        note_transmission_key,
        counterparty_diversified_generator,
        counterparty_transmission_key,
        &compliance_ciphertext,
    )?;

    // DLEQ verification: metadata hash + 3 tier proofs
    // M = Poseidon_6(policy_id_hash, resource_hash, permission_hash, tier, target_timestamp, salt)
    // All 3 tiers share the same metadata hash (same policy fields, same target_timestamp, same salt)
    // but with different tier constants.

    let tier_core = FqVar::new_constant(cs.clone(), Fq::from(1u64))?;
    let tier_ext = FqVar::new_constant(cs.clone(), Fq::from(2u64))?;
    let tier_sext = FqVar::new_constant(cs.clone(), Fq::from(3u64))?;

    let m_core = compute_metadata_hash_r1cs(
        cs.clone(),
        &asset_indexed_leaf.policy_id_hash,
        &asset_indexed_leaf.resource_hash,
        &asset_indexed_leaf.permission_hash,
        &tier_core,
        &target_timestamp,
        &salt_var,
    )?;
    let m_ext = compute_metadata_hash_r1cs(
        cs.clone(),
        &asset_indexed_leaf.policy_id_hash,
        &asset_indexed_leaf.resource_hash,
        &asset_indexed_leaf.permission_hash,
        &tier_ext,
        &target_timestamp,
        &salt_var,
    )?;
    let m_sext = compute_metadata_hash_r1cs(
        cs.clone(),
        &asset_indexed_leaf.policy_id_hash,
        &asset_indexed_leaf.resource_hash,
        &asset_indexed_leaf.permission_hash,
        &tier_sext,
        &target_timestamp,
        &salt_var,
    )?;

    // Core DLEQ (r_1, ACK_receiver)
    verify_dleq_r1cs(
        cs.clone(),
        &r1_bits,
        &ack_receiver,
        &epk_1,
        &m_core,
        &dleq_c_1,
        &dleq_s_1,
        &is_regulated,
    )?;

    // Ext DLEQ (r_2, ACK_receiver)
    verify_dleq_r1cs(
        cs.clone(),
        &r2_bits,
        &ack_receiver,
        &epk_2,
        &m_ext,
        &dleq_c_2,
        &dleq_s_2,
        &is_regulated,
    )?;

    // Sext DLEQ (r_3, ACK_sender — counterparty disclosure tier)
    verify_dleq_r1cs(
        cs,
        &r3_bits,
        &ack_sender,
        &epk_3,
        &m_sext,
        &dleq_c_3,
        &dleq_s_3,
        &is_regulated,
    )?;

    Ok(r1_bits)
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

/// Convert an Fr scalar to circuit bit variables and verify EPK = r × G.
fn alloc_esk_and_verify_epk(
    cs: ConstraintSystemRef<Fq>,
    esk: Fr,
    published_epk: &ElementVar,
) -> Result<Vec<Boolean<Fq>>, SynthesisError> {
    use ark_ff::{BigInteger, PrimeField};

    let esk_bigint = esk.into_bigint();
    let esk_bits: Vec<bool> = (0..Fr::MODULUS_BIT_SIZE)
        .map(|i| esk_bigint.get_bit(i as usize))
        .collect();
    let esk_bits_vars: Vec<Boolean<Fq>> = esk_bits
        .iter()
        .map(|bit| Boolean::new_witness(cs.clone(), || Ok(*bit)))
        .collect::<Result<Vec<_>, _>>()?;

    let generator_var = ElementVar::new_constant(cs, decaf377::Element::GENERATOR)?;
    let computed_epk = generator_var.scalar_mul_le(esk_bits_vars.iter())?;
    computed_epk.enforce_equal(published_epk)?;

    Ok(esk_bits_vars)
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
        gt = Boolean::kary_or(&[
            gt,
            Boolean::kary_and(&[!lt.clone(), p.clone(), !q.clone()])?,
        ])?;
        // If we see a=0, b=1 and haven't determined gt yet, then a < b
        lt = Boolean::kary_or(&[
            lt,
            Boolean::kary_and(&[!gt.clone(), q.clone(), !p.clone()])?,
        ])?;
    }

    Ok(lt)
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
    let is_in_gap = Boolean::kary_and(&[gt_low, lt_high])?;

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
fn verify_path(
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

    calculated_user_root.conditional_enforce_equal(compliance_anchor, is_regulated)?;

    Ok(())
}

/// Verify 4-tier Poseidon stream cipher encryption with hybrid KEM/DEM.
///
/// Ciphertext layout: [detection:4] [core:3] [ext:3] [sext:3] = 13 Fqs.
/// Detection slots: asset_id+flag, salt, sender_slot_id, receiver_slot_id.
///
/// Detection uses epk_1. Core/ext/sext use seeds from C2 ElGamal envelopes.
/// Conditional on `is_regulated` — unregulated assets skip enforcement.
fn verify_poseidon_encryption(
    cs: ConstraintSystemRef<Fq>,
    is_regulated: &Boolean<Fq>,
    is_flagged: &Boolean<Fq>,
    ss_detection: &ElementVar,
    ss_core: &ElementVar,
    ss_ext: &ElementVar,
    ss_sext: &ElementVar,
    c2_core: &FqVar,
    c2_ext: &FqVar,
    c2_sext: &FqVar,
    epk_1: &ElementVar,
    salt: &FqVar,
    sender_slot_id: &FqVar,
    receiver_slot_id: &FqVar,
    note_amount: FqVar,
    note_asset_id: FqVar,
    self_diversified_generator: ElementVar,
    self_transmission_key: ElementVar,
    counterparty_diversified_generator: ElementVar,
    counterparty_transmission_key: ElementVar,
    compliance_ciphertext: &[FqVar],
) -> Result<(), SynthesisError> {
    if compliance_ciphertext.len() != 13 {
        return Err(SynthesisError::Unsatisfiable);
    }

    let epk_1_fq = epk_1.compress_to_field()?;
    let issuer_domain_sep =
        FqVar::new_constant(cs.clone(), *crate::crypto::ISSUER_DETECTION_DOMAIN)?;

    let plaintext_var = CompliancePlaintextVar {
        amount: note_amount,
        asset_id: note_asset_id,
        self_diversified_generator,
        self_transmission_key,
        counterparty_diversified_generator,
        counterparty_transmission_key,
    };

    // === DETECTION: 4 Fq elements ===
    let seed_detection = poseidon377::r1cs::hash_2(
        cs.clone(),
        &issuer_domain_sep,
        (ss_detection.compress_to_field()?, epk_1_fq),
    )?;

    // Slot 0: asset_id + flag
    let flag_bit_var = FqVar::new_constant(cs.clone(), flag_bit_fq())?;
    let flag_contribution = FqVar::conditionally_select(is_flagged, &flag_bit_var, &FqVar::zero())?;
    let detection_plaintext = &plaintext_var.detection_plaintext() + &flag_contribution;

    let counter_0 = FqVar::new_constant(cs.clone(), Fq::from(0u64))?;
    let keystream_0 = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (counter_0, seed_detection.clone()),
    )?;
    let computed_detection_0 = &detection_plaintext + &keystream_0;
    computed_detection_0.conditional_enforce_equal(&compliance_ciphertext[0], is_regulated)?;

    // Slot 1: salt
    let counter_1 = FqVar::new_constant(cs.clone(), Fq::from(1u64))?;
    let keystream_1 = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (counter_1, seed_detection.clone()),
    )?;
    let computed_detection_1 = salt + &keystream_1;
    computed_detection_1.conditional_enforce_equal(&compliance_ciphertext[1], is_regulated)?;

    let counter_2 = FqVar::new_constant(cs.clone(), Fq::from(2u64))?;
    let keystream_2 = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (counter_2, seed_detection.clone()),
    )?;
    let computed_detection_2 = sender_slot_id + &keystream_2;
    computed_detection_2.conditional_enforce_equal(&compliance_ciphertext[2], is_regulated)?;

    let counter_3 = FqVar::new_constant(cs.clone(), Fq::from(3u64))?;
    let keystream_3 = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (counter_3, seed_detection.clone()),
    )?;
    let computed_detection_3 = receiver_slot_id + &keystream_3;
    computed_detection_3.conditional_enforce_equal(&compliance_ciphertext[3], is_regulated)?;

    // === CORE: 3 Fq elements (amount + self address), starting at index 4 ===
    let ss_core_fq = ss_core.compress_to_field()?;
    let seed_core = c2_core - &ss_core_fq;

    let core_plaintexts = plaintext_var.core_plaintext_fqs()?;
    for (i, plain_var) in core_plaintexts.iter().enumerate() {
        let counter = FqVar::new_constant(cs.clone(), Fq::from(i as u64))?;
        let keystream =
            poseidon377::r1cs::hash_2(cs.clone(), &seed_core, (counter, seed_core.clone()))?;
        let computed_cipher = plain_var + &keystream;
        computed_cipher.conditional_enforce_equal(&compliance_ciphertext[4 + i], is_regulated)?;
    }

    // === EXT: 3 Fq elements (counterparty address), starting at index 7 ===
    let ss_ext_fq = ss_ext.compress_to_field()?;
    let seed_ext = c2_ext - &ss_ext_fq;

    let extension_plaintexts = plaintext_var.extension_plaintext_fqs()?;
    for (i, plain_var) in extension_plaintexts.iter().enumerate() {
        let counter = FqVar::new_constant(cs.clone(), Fq::from(i as u64))?;
        let keystream =
            poseidon377::r1cs::hash_2(cs.clone(), &seed_ext, (counter, seed_ext.clone()))?;
        let computed_cipher = plain_var + &keystream;
        computed_cipher.conditional_enforce_equal(&compliance_ciphertext[7 + i], is_regulated)?;
    }

    // === SEXT: 3 Fq elements (same plaintext as core), starting at index 10 ===
    let ss_sext_fq = ss_sext.compress_to_field()?;
    let seed_sext = c2_sext - &ss_sext_fq;

    for (i, plain_var) in core_plaintexts.iter().enumerate() {
        let counter = FqVar::new_constant(cs.clone(), Fq::from(i as u64))?;
        let keystream =
            poseidon377::r1cs::hash_2(cs.clone(), &seed_sext, (counter, seed_sext.clone()))?;
        let computed_cipher = plain_var + &keystream;
        computed_cipher.conditional_enforce_equal(&compliance_ciphertext[10 + i], is_regulated)?;
    }

    Ok(())
}

/// Compute the salted metadata hash in-circuit (mirrors native `compute_metadata_hash`).
///
/// M = Poseidon_6(DLEQ_METADATA_DOMAIN, (policy_id_hash, resource_hash, permission_hash, tier, target_timestamp, salt))
pub fn compute_metadata_hash_r1cs(
    cs: ConstraintSystemRef<Fq>,
    policy_id_hash: &FqVar,
    resource_hash: &FqVar,
    permission_hash: &FqVar,
    tier: &FqVar,
    target_timestamp: &FqVar,
    salt: &FqVar,
) -> Result<FqVar, SynthesisError> {
    let domain = FqVar::new_constant(cs.clone(), *crate::crypto::DLEQ_METADATA_DOMAIN)?;
    poseidon377::r1cs::hash_6(
        cs,
        &domain,
        (
            policy_id_hash.clone(),
            resource_hash.clone(),
            permission_hash.clone(),
            tier.clone(),
            target_timestamp.clone(),
            salt.clone(),
        ),
    )
}

/// Verify a DLEQ proof in-circuit (pure verifier — no k witness).
///
/// Reconstructs R and R' from published (c, s), recomputes the Fiat-Shamir challenge
/// via Orbis-compatible hash_7, and compares the truncated (252-bit) challenge.
///
/// Conditional on `is_regulated` — unregulated assets skip enforcement.
pub fn verify_dleq_r1cs(
    cs: ConstraintSystemRef<Fq>,
    r_bits: &[Boolean<Fq>],
    ack: &ElementVar,
    epk: &ElementVar,
    metadata_hash: &FqVar,
    published_c: &FqVar,
    published_s: &FqVar,
    is_regulated: &Boolean<Fq>,
) -> Result<(), SynthesisError> {
    // S = r × ACK (reuses r_bits already allocated for EPK verification)
    let s_point = ack.scalar_mul_le(r_bits.iter())?;

    // Reconstruct R and R' from verification equations:
    // R_rec  = s × G - c × EPK
    // R'_rec = s × ACK - c × S

    // Decompose published_s to bits for scalar mul
    let s_bits = published_s.to_bits_le()?;
    // Decompose published_c to bits for scalar mul
    let c_bits = published_c.to_bits_le()?;

    // s × G (fixed-base)
    let generator = ElementVar::new_constant(cs.clone(), decaf377::Element::GENERATOR)?;
    let s_times_g = generator.scalar_mul_le(s_bits.iter())?;
    // c × EPK (variable-base)
    let c_times_epk = epk.scalar_mul_le(c_bits.iter())?;
    // R_rec = s × G - c × EPK
    let r_rec = s_times_g - &c_times_epk;

    // s × ACK (variable-base)
    let s_times_ack = ack.scalar_mul_le(s_bits.iter())?;
    // c × S (variable-base)
    let c_times_s = s_point.scalar_mul_le(c_bits.iter())?;
    // R'_rec = s × ACK - c × S
    let rp_rec = s_times_ack - &c_times_s;

    // Compress all points for the challenge hash (Orbis-compatible ordering: M, G, ACK, EPK, S, R, R')
    // G is a known constant — pre-compute its Fq value to avoid compress_to_field on constant ElementVar
    let g_fq = FqVar::new_constant(
        cs.clone(),
        decaf377::Element::GENERATOR.vartime_compress_to_field(),
    )?;
    let ack_fq = ack.compress_to_field()?;
    let epk_fq = epk.compress_to_field()?;
    let s_fq = s_point.compress_to_field()?;
    let r_fq = r_rec.compress_to_field()?;
    let rp_fq = rp_rec.compress_to_field()?;

    // Recompute challenge: c = hash_7(ENCRYPT_PROOF_DOMAIN, (M, G, ACK, EPK, S, R, R'))
    let challenge_domain = FqVar::new_constant(
        cs.clone(),
        Fq::from_le_bytes_mod_order(crate::crypto::ENCRYPT_PROOF_DOMAIN),
    )?;
    let c_computed = poseidon377::r1cs::hash_7(
        cs.clone(),
        &challenge_domain,
        (
            metadata_hash.clone(),
            g_fq,
            ack_fq,
            epk_fq,
            s_fq,
            r_fq,
            rp_fq,
        ),
    )?;

    // Truncation: compare only the low Fr::MODULUS_BIT_SIZE-1 (=250) bits,
    // matching fq_to_challenge_scalar. The hash output is a full Fq; the published c
    // stores the truncated value (high bits zero). We enforce published c's high bits
    // are zero to prevent malleability.
    // Reuse c_bits (from published_c.to_bits_le() above) to avoid double decomposition.
    let keep_bits = (Fr::MODULUS_BIT_SIZE as usize) - 1;
    let c_computed_bits = c_computed.to_bits_le()?;

    // Enforce published c high bits are zero (prevents malleability)
    for bit in c_bits.iter().skip(keep_bits) {
        bit.conditional_enforce_equal(&Boolean::FALSE, is_regulated)?;
    }

    // Compare only the low keep_bits between hash output and published c
    for i in 0..keep_bits {
        c_computed_bits[i].conditional_enforce_equal(&c_bits[i], is_regulated)?;
    }

    Ok(())
}

/// Threshold flag verification: enforces `is_flagged == (amount >= threshold)`.
pub fn verify_threshold_flag_simple(
    _cs: ConstraintSystemRef<Fq>,
    amount: &FqVar,
    threshold: &FqVar,
    is_flagged: &Boolean<Fq>,
) -> Result<(), SynthesisError> {
    let amount_lt_threshold = fq_is_less_than(amount, threshold)?;
    let amount_gte_threshold = !amount_lt_threshold;
    is_flagged.enforce_equal(&amount_gte_threshold)?;
    Ok(())
}

/// Derive shared secrets for the single-EPK reduced ciphertext (detection + core).
///
/// Single r_s: EPK = r_s × G. Detection: r_s × dk_pub. Core: r_s × ack_core (or dk_pub if flagged).
/// Returns `(ss_detection, ss_core, r_s_bits)`.
pub fn derive_shared_secrets_spend(
    cs: ConstraintSystemRef<Fq>,
    r_s: Fr,
    ack_core: &ElementVar,
    dk_pub: &ElementVar,
    is_flagged: &Boolean<Fq>,
    published_epk: &ElementVar,
) -> Result<(ElementVar, ElementVar, Vec<Boolean<Fq>>), SynthesisError> {
    let r_s_bits = alloc_esk_and_verify_epk(cs.clone(), r_s, published_epk)?;

    // ss_core_user = r_s × ACK_core
    let ss_core_user = ack_core.scalar_mul_le(r_s_bits.iter())?;
    // ss_issuer = r_s × dk_pub (used for detection always, core if flagged)
    let ss_issuer = dk_pub.scalar_mul_le(r_s_bits.iter())?;

    let ss_detection = ss_issuer.clone();
    let ss_core = ElementVar::conditionally_select(is_flagged, &ss_issuer, &ss_core_user)?;

    Ok((ss_detection, ss_core, r_s_bits))
}

/// Derive shared secrets for the multi-tier ciphertext with 3 independent ephemeral scalars.
///
/// r_1 → detection + core, r_2 → ext, r_3 → sext. All EPKs on G.
/// Core/ext use `ack_receiver`, sext uses `ack_sender` for counterparty disclosure.
/// Returns `(ss_detection, ss_core, ss_ext, ss_sext, r1_bits, r2_bits, r3_bits)`.
pub fn derive_shared_secrets_output(
    cs: ConstraintSystemRef<Fq>,
    r_1: Fr,
    r_2: Fr,
    r_3: Fr,
    ack_receiver: &ElementVar,
    ack_sender: &ElementVar,
    dk_pub: &ElementVar,
    is_flagged: &Boolean<Fq>,
    epk_1: &ElementVar,
    epk_2: &ElementVar,
    epk_3: &ElementVar,
) -> Result<
    (
        ElementVar,
        ElementVar,
        ElementVar,
        ElementVar,
        Vec<Boolean<Fq>>,
        Vec<Boolean<Fq>>,
        Vec<Boolean<Fq>>,
    ),
    SynthesisError,
> {
    // Alloc and verify EPK_i = r_i × G for each scalar
    let r1_bits = alloc_esk_and_verify_epk(cs.clone(), r_1, epk_1)?;
    let r2_bits = alloc_esk_and_verify_epk(cs.clone(), r_2, epk_2)?;
    let r3_bits = alloc_esk_and_verify_epk(cs.clone(), r_3, epk_3)?;

    // User shared secrets: core/ext use ack_receiver, sext uses ack_sender
    let ss_core_user = ack_receiver.scalar_mul_le(r1_bits.iter())?;
    let ss_ext_user = ack_receiver.scalar_mul_le(r2_bits.iter())?;
    let ss_sext_user = ack_sender.scalar_mul_le(r3_bits.iter())?;

    // Issuer shared secrets: r_i × dk_pub
    let ss_issuer_1 = dk_pub.scalar_mul_le(r1_bits.iter())?;
    let ss_issuer_2 = dk_pub.scalar_mul_le(r2_bits.iter())?;
    let ss_issuer_3 = dk_pub.scalar_mul_le(r3_bits.iter())?;

    // Detection always uses issuer (r_1 × dk_pub)
    let ss_detection = ss_issuer_1.clone();

    // Conditional selection based on flagging
    let ss_core = ElementVar::conditionally_select(is_flagged, &ss_issuer_1, &ss_core_user)?;
    let ss_ext = ElementVar::conditionally_select(is_flagged, &ss_issuer_2, &ss_ext_user)?;
    let ss_sext = ElementVar::conditionally_select(is_flagged, &ss_issuer_3, &ss_sext_user)?;

    Ok((
        ss_detection,
        ss_core,
        ss_ext,
        ss_sext,
        r1_bits,
        r2_bits,
        r3_bits,
    ))
}

/// Verify Poseidon encryption for the reduced ciphertext (detection + core only).
///
/// Expected ciphertext layout: [detection: 4] [core: 3] = 7 Fq elements.
/// Detection slots: asset_id+flag, salt, sender_slot_id, receiver_slot_id.
fn verify_poseidon_encryption_spend(
    cs: ConstraintSystemRef<Fq>,
    is_regulated: &Boolean<Fq>,
    is_flagged: &Boolean<Fq>,
    ss_detection: &ElementVar,
    ss_core: &ElementVar,
    c2_core: &FqVar,
    epk: &ElementVar,
    salt: &FqVar,
    note_amount: FqVar,
    note_asset_id: FqVar,
    self_diversified_generator: ElementVar,
    self_transmission_key: ElementVar,
    compliance_ciphertext: &[FqVar],
) -> Result<(), SynthesisError> {
    if compliance_ciphertext.len() != 7 {
        return Err(SynthesisError::Unsatisfiable);
    }

    let epk_fq = epk.compress_to_field()?;
    let issuer_domain_sep =
        FqVar::new_constant(cs.clone(), *crate::crypto::ISSUER_DETECTION_DOMAIN)?;

    // Counterparty fields unused in spend — set to self
    let plaintext_var = CompliancePlaintextVar {
        amount: note_amount,
        asset_id: note_asset_id,
        self_diversified_generator: self_diversified_generator.clone(),
        self_transmission_key: self_transmission_key.clone(),
        counterparty_diversified_generator: self_diversified_generator,
        counterparty_transmission_key: self_transmission_key,
    };

    // Detection seed from issuer shared secret + epk_1 compressed
    let seed_detection = poseidon377::r1cs::hash_2(
        cs.clone(),
        &issuer_domain_sep,
        (ss_detection.compress_to_field()?, epk_fq),
    )?;

    // Detection slot 0: asset_id + flag
    let flag_bit_var = FqVar::new_constant(cs.clone(), flag_bit_fq())?;
    let flag_contribution = FqVar::conditionally_select(is_flagged, &flag_bit_var, &FqVar::zero())?;
    let detection_plaintext = &plaintext_var.detection_plaintext() + &flag_contribution;

    let counter_0 = FqVar::new_constant(cs.clone(), Fq::from(0u64))?;
    let keystream_0 = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (counter_0, seed_detection.clone()),
    )?;
    let computed_detection_0 = &detection_plaintext + &keystream_0;
    computed_detection_0.conditional_enforce_equal(&compliance_ciphertext[0], is_regulated)?;

    // Detection slot 1: salt
    let counter_1 = FqVar::new_constant(cs.clone(), Fq::from(1u64))?;
    let keystream_1 = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (counter_1, seed_detection.clone()),
    )?;
    let computed_detection_1 = salt + &keystream_1;
    computed_detection_1.conditional_enforce_equal(&compliance_ciphertext[1], is_regulated)?;

    let counter_2 = FqVar::new_constant(cs.clone(), Fq::from(2u64))?;
    let keystream_2 = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (counter_2, seed_detection.clone()),
    )?;
    let computed_detection_2 = FqVar::zero() + &keystream_2;
    computed_detection_2.conditional_enforce_equal(&compliance_ciphertext[2], is_regulated)?;

    let counter_3 = FqVar::new_constant(cs.clone(), Fq::from(3u64))?;
    let keystream_3 = poseidon377::r1cs::hash_2(
        cs.clone(),
        &seed_detection,
        (counter_3, seed_detection.clone()),
    )?;
    let computed_detection_3 = FqVar::zero() + &keystream_3;
    computed_detection_3.conditional_enforce_equal(&compliance_ciphertext[3], is_regulated)?;

    // Core: seed from C2 envelope
    let ss_core_fq = ss_core.compress_to_field()?;
    let seed_core = c2_core - &ss_core_fq;

    let core_plaintexts = plaintext_var.core_plaintext_fqs()?;
    for (i, plain_var) in core_plaintexts.iter().enumerate() {
        let counter = FqVar::new_constant(cs.clone(), Fq::from(i as u64))?;
        let keystream =
            poseidon377::r1cs::hash_2(cs.clone(), &seed_core, (counter, seed_core.clone()))?;
        let computed_cipher = plain_var + &keystream;
        computed_cipher.conditional_enforce_equal(&compliance_ciphertext[4 + i], is_regulated)?;
    }

    Ok(())
}

/// Transfer-input-only compliance verification (detection + core, no extension).
///
/// Single r_s: EPK = r_s × G. ACK_core derived in-circuit from ring_pk.
/// Policy fields (dk_pub, threshold, ring_pk) are private witnesses bound by IMT proof.
pub fn verify_compliance_spend(
    cs: ConstraintSystemRef<Fq>,
    // Public Inputs
    asset_anchor: FqVar,
    compliance_anchor: FqVar,
    epk: ElementVar,
    c2_core: FqVar,
    compliance_ciphertext: Vec<FqVar>, // 7 Fqs: detection(4) + core(3)
    // DLEQ public inputs (1 tier × 2 = 2 Fq values)
    target_timestamp: FqVar,
    dleq_c: FqVar,
    dleq_s: FqVar,
    // Note data
    note_asset_id: FqVar,
    note_amount: FqVar,
    note_diversified_generator: ElementVar,
    note_transmission_key: ElementVar,
    // Witness
    r_s: Fr,
    witness: ComplianceWitness,
) -> Result<(), SynthesisError> {
    let (
        is_regulated,
        asset_indexed_leaf,
        asset_position,
        compliance_position,
        user_leaf_var,
        is_flagged,
    ) = allocate_compliance_witnesses(cs.clone(), &witness)?;

    verify_asset_registry_imt(
        cs.clone(),
        note_asset_id.clone(),
        &is_regulated,
        &asset_indexed_leaf,
        &witness.asset_path,
        &asset_position,
        &asset_anchor,
    )?;

    verify_path(
        cs.clone(),
        &user_leaf_var,
        &is_regulated,
        &witness.compliance_path,
        &compliance_position,
        &compliance_anchor,
    )?;

    // Leaf↔note binding: the Merkle-proven leaf must match the note's address and asset.
    let leaf_g_d = user_leaf_var.address.diversified_generator();
    let leaf_pk_d = user_leaf_var.address.transmission_key();
    leaf_g_d.enforce_equal(&note_diversified_generator)?;
    leaf_pk_d.enforce_equal(&note_transmission_key)?;
    user_leaf_var.asset_id.enforce_equal(&note_asset_id)?;

    // Policy fields extracted from IndexedLeafVar (bound by IMT proof)
    verify_threshold_flag_simple(
        cs.clone(),
        &note_amount,
        &asset_indexed_leaf.threshold,
        &is_flagged,
    )?;

    // Single ACK from leaf's `d` scalar: ACK = d × ring_pk
    let ack = derive_ack_from_leaf_d(&asset_indexed_leaf.ring_pk, &user_leaf_var.d)?;

    let (ss_detection, ss_core, r_s_bits) = derive_shared_secrets_spend(
        cs.clone(),
        r_s,
        &ack,
        &asset_indexed_leaf.dk_pub,
        &is_flagged,
        &epk,
    )?;

    // Salt witness for detection tier
    let salt_var = FqVar::new_witness(cs.clone(), || Ok(witness.salt))?;

    verify_poseidon_encryption_spend(
        cs.clone(),
        &is_regulated,
        &is_flagged,
        &ss_detection,
        &ss_core,
        &c2_core,
        &epk,
        &salt_var,
        note_amount,
        note_asset_id,
        note_diversified_generator,
        note_transmission_key,
        &compliance_ciphertext,
    )?;

    // DLEQ verification: metadata hash + 1 tier proof (core, tier=1)
    let tier_core = FqVar::new_constant(cs.clone(), Fq::from(1u64))?;
    let m_core = compute_metadata_hash_r1cs(
        cs.clone(),
        &asset_indexed_leaf.policy_id_hash,
        &asset_indexed_leaf.resource_hash,
        &asset_indexed_leaf.permission_hash,
        &tier_core,
        &target_timestamp,
        &salt_var,
    )?;

    verify_dleq_r1cs(
        cs,
        &r_s_bits,
        &ack,
        &epk,
        &m_core,
        &dleq_c,
        &dleq_s,
        &is_regulated,
    )?;

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structs::AssetPolicy;
    use crate::IndexedMerkleTree;
    use ark_relations::r1cs::ConstraintSystem;
    use decaf377::{Element, Fr};
    use penumbra_sdk_asset::asset;
    use penumbra_sdk_keys::keys::Diversifier;
    use penumbra_sdk_keys::Address;
    use rand_core::OsRng;

    fn test_policy() -> AssetPolicy {
        AssetPolicy::default_unregulated()
    }

    /// Test: derive_ack_from_leaf_d matches native d × ring_pk
    #[test]
    fn test_ack_derivation_circuit_vs_native() {
        let mut rng = OsRng;
        let cs = ConstraintSystem::<Fq>::new_ref();

        let sk_ring = Fr::rand(&mut rng);
        let ring_pk = Element::GENERATOR * sk_ring;
        let d = Fq::from(42u64);

        // Native: ACK = d × ring_pk (d interpreted as scalar via from_le_bytes_mod_order)
        let d_fr = Fr::from_le_bytes_mod_order(&d.to_bytes());
        let ack_native = ring_pk * d_fr;

        // Circuit derivation
        let ring_pk_var = ElementVar::new_witness(cs.clone(), || Ok(ring_pk)).unwrap();
        let d_var = FqVar::new_witness(cs.clone(), || Ok(d)).unwrap();
        let ack_circuit = derive_ack_from_leaf_d(&ring_pk_var, &d_var).unwrap();

        assert!(cs.is_satisfied().unwrap());
        assert_eq!(ack_circuit.value().unwrap(), ack_native);
    }

    /// Test: ComplianceLeafVar commitment matches native implementation
    #[test]
    fn test_compliance_leaf_var_commit() {
        let mut rng = OsRng;
        let cs = ConstraintSystem::<Fq>::new_ref();

        let scalar = Fr::rand(&mut rng);
        let point = Element::GENERATOR * scalar;
        let pk_d = decaf377_ka::Public(point.vartime_compress().0);
        let mut ck_d_bytes = [0u8; 32];
        use rand_core::RngCore;
        rng.fill_bytes(&mut ck_d_bytes);
        let ck_d = decaf377_fmd::ClueKey(ck_d_bytes);
        let diversifier = Diversifier([1u8; 16]);
        let address = Address::from_components(diversifier, pk_d, ck_d).expect("valid address");

        let leaf =
            ComplianceLeaf::with_slot(address, asset::Id(Fq::from(42u64)), 0, Fq::from(123u64));

        let native_commitment = leaf.commit();

        let leaf_var =
            ComplianceLeafVar::new_variable(cs.clone(), || Ok(&leaf), AllocationMode::Witness)
                .expect("leaf allocation should succeed");

        let circuit_commitment = leaf_var
            .commit(cs.clone())
            .expect("commitment should succeed");

        assert!(cs.is_satisfied().unwrap());
        assert_eq!(circuit_commitment.value().unwrap(), native_commitment.0,);
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

        // Create a test leaf (sentinel with default policy)
        let leaf = IndexedLeaf::with_default_policy(Fq::from(0u64), 0, *FQ_MAX);

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
        tree.insert(asset_id, &test_policy())
            .expect("insert should succeed");

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

    /// Test: verify_asset_registry_imt gadget with the real base asset ID
    /// This tests whether is_cmp works correctly with large field elements (256-bit hashes)
    #[test]
    fn test_verify_asset_registry_imt_non_membership_large_asset_id() {
        use crate::IndexedMerkleTree;
        use penumbra_sdk_asset::BASE_ASSET_ID;

        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create an IMT (empty except sentinel)
        let tree = IndexedMerkleTree::new();
        let asset_id = BASE_ASSET_ID.0; // Real 256-bit hash value

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
            "Circuit should be satisfied for non-membership proof with large asset_id"
        );
    }

    /// Test that invalid membership proof (wrong asset_id) fails circuit
    #[test]
    fn test_invalid_membership_fails_circuit() {
        let cs = ConstraintSystem::<Fq>::new_ref();

        let mut tree = IndexedMerkleTree::new();
        let real_asset = Fq::from(42u64);
        tree.insert(real_asset, &test_policy()).unwrap();

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
        tree.insert(Fq::from(100u64), &test_policy()).unwrap();
        tree.insert(Fq::from(300u64), &test_policy()).unwrap();

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
        tree.insert(Fq::from(42u64), &test_policy()).unwrap();

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

    #[test]
    fn test_dleq_circuit_satisfied() {
        use crate::crypto::{compute_dleq_native, compute_metadata_hash};
        use ark_ff::{BigInteger, PrimeField};

        // Run multiple iterations with different derived scalars
        for i in 1u64..20 {
            let cs = ConstraintSystem::<Fq>::new_ref();

            // Deterministic scalars derived from loop index
            let r = Fr::from(100 + i);
            let ring_sk = Fr::from(200 + i);
            let ring_pk = decaf377::Element::GENERATOR * ring_sk;

            let d_fr = Fr::from(300 + i);
            let ack = ring_pk * d_fr;
            let epk = decaf377::Element::GENERATOR * r;

            let k = Fr::from(400 + i);
            let salt = Fq::from(500 + i);
            let target_timestamp = Fq::from(1_700_000_000u64);

            let metadata_hash = compute_metadata_hash(
                Fq::from(1u64),
                Fq::from(2u64),
                Fq::from(3u64),
                Fq::from(1u64),
                target_timestamp,
                salt,
            );

            let dleq = compute_dleq_native(r, k, &ack, &epk, metadata_hash);

            let c_fq = dleq.c;
            let s_fq = Fq::from_le_bytes_mod_order(&dleq.s.to_bytes());

            let ack_var = ElementVar::new_witness(cs.clone(), || Ok(ack)).unwrap();
            let epk_var = ElementVar::new_witness(cs.clone(), || Ok(epk)).unwrap();
            let metadata_var = FqVar::new_witness(cs.clone(), || Ok(metadata_hash)).unwrap();
            let published_c_var = FqVar::new_input(cs.clone(), || Ok(c_fq)).unwrap();
            let published_s_var = FqVar::new_input(cs.clone(), || Ok(s_fq)).unwrap();
            let is_regulated = Boolean::new_witness(cs.clone(), || Ok(true)).unwrap();

            let r_bigint = r.into_bigint();
            let r_bits: Vec<Boolean<Fq>> = (0..Fr::MODULUS_BIT_SIZE)
                .map(|j| {
                    Boolean::new_witness(cs.clone(), || Ok(r_bigint.get_bit(j as usize))).unwrap()
                })
                .collect();

            verify_dleq_r1cs(
                cs.clone(),
                &r_bits,
                &ack_var,
                &epk_var,
                &metadata_var,
                &published_c_var,
                &published_s_var,
                &is_regulated,
            )
            .unwrap();

            assert!(
                cs.is_satisfied().unwrap(),
                "DLEQ circuit should be satisfied with valid proof (i={i})"
            );
        }
    }

    #[test]
    fn test_dleq_wrong_metadata_fails() {
        use crate::crypto::{compute_dleq_native, compute_metadata_hash};
        use ark_ff::{BigInteger, PrimeField};

        let mut rng = rand::thread_rng();
        let cs = ConstraintSystem::<Fq>::new_ref();

        let r = Fr::rand(&mut rng);
        let ring_sk = Fr::rand(&mut rng);
        let ring_pk = decaf377::Element::GENERATOR * ring_sk;
        let d_fr = Fr::rand(&mut rng);
        let ack = ring_pk * d_fr;
        let epk = decaf377::Element::GENERATOR * r;
        let k = Fr::rand(&mut rng);

        let metadata_hash = compute_metadata_hash(
            Fq::from(1u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );

        let dleq = compute_dleq_native(r, k, &ack, &epk, metadata_hash);
        let c_fq = dleq.c;
        let s_fq = Fq::from_le_bytes_mod_order(&dleq.s.to_bytes());

        // Use WRONG metadata in circuit
        let wrong_metadata = compute_metadata_hash(
            Fq::from(99u64),
            Fq::from(2u64),
            Fq::from(3u64),
            Fq::from(1u64),
            Fq::from(1_700_000_000u64),
            Fq::rand(&mut rng),
        );

        let ack_var = ElementVar::new_witness(cs.clone(), || Ok(ack)).unwrap();
        let epk_var = ElementVar::new_witness(cs.clone(), || Ok(epk)).unwrap();
        let metadata_var = FqVar::new_witness(cs.clone(), || Ok(wrong_metadata)).unwrap();
        let published_c_var = FqVar::new_input(cs.clone(), || Ok(c_fq)).unwrap();
        let published_s_var = FqVar::new_input(cs.clone(), || Ok(s_fq)).unwrap();
        let is_regulated = Boolean::new_witness(cs.clone(), || Ok(true)).unwrap();

        let r_bigint = r.into_bigint();
        let r_bits: Vec<Boolean<Fq>> = (0..Fr::MODULUS_BIT_SIZE)
            .map(|i| Boolean::new_witness(cs.clone(), || Ok(r_bigint.get_bit(i as usize))).unwrap())
            .collect();

        verify_dleq_r1cs(
            cs.clone(),
            &r_bits,
            &ack_var,
            &epk_var,
            &metadata_var,
            &published_c_var,
            &published_s_var,
            &is_regulated,
        )
        .unwrap();

        assert!(
            !cs.is_satisfied().unwrap(),
            "DLEQ circuit should FAIL with wrong metadata"
        );
    }

    /// Minimal test: hash_7 in R1CS matches native hash_7
    #[test]
    fn test_hash7_r1cs_matches_native() {
        let cs = ConstraintSystem::<Fq>::new_ref();

        // Known inputs
        let domain = Fq::from_le_bytes_mod_order(crate::crypto::ENCRYPT_PROOF_DOMAIN);
        let a = Fq::from(1u64);
        let b = Fq::from(2u64);
        let c = Fq::from(3u64);
        let d = Fq::from(4u64);
        let e = Fq::from(5u64);
        let f = Fq::from(6u64);
        let g = Fq::from(7u64);

        // Native hash
        let expected = poseidon377::hash_7(&domain, (a, b, c, d, e, f, g));

        // Circuit hash
        let domain_var = FqVar::new_constant(cs.clone(), domain).unwrap();
        let a_var = FqVar::new_witness(cs.clone(), || Ok(a)).unwrap();
        let b_var = FqVar::new_witness(cs.clone(), || Ok(b)).unwrap();
        let c_var = FqVar::new_witness(cs.clone(), || Ok(c)).unwrap();
        let d_var = FqVar::new_witness(cs.clone(), || Ok(d)).unwrap();
        let e_var = FqVar::new_witness(cs.clone(), || Ok(e)).unwrap();
        let f_var = FqVar::new_witness(cs.clone(), || Ok(f)).unwrap();
        let g_var = FqVar::new_witness(cs.clone(), || Ok(g)).unwrap();

        let result_var = poseidon377::r1cs::hash_7(
            cs.clone(),
            &domain_var,
            (a_var, b_var, c_var, d_var, e_var, f_var, g_var),
        )
        .unwrap();

        // Compare output
        let expected_var = FqVar::new_input(cs.clone(), || Ok(expected)).unwrap();
        result_var.enforce_equal(&expected_var).unwrap();

        assert!(
            cs.is_satisfied().unwrap(),
            "hash_7 R1CS should match native"
        );
    }

    /// Minimal test: scalar_mul_le with truncated Fq value
    #[test]
    fn test_scalar_mul_with_truncated_fq() {
        let mut rng = rand::thread_rng();
        let cs = ConstraintSystem::<Fq>::new_ref();

        // Create a truncated c value (< 2^252)
        let c_fr = Fr::rand(&mut rng);
        let c_truncated =
            crate::crypto::fq_to_challenge_scalar(Fq::from_le_bytes_mod_order(&c_fr.to_bytes()));
        let c_fq = Fq::from_le_bytes_mod_order(&c_truncated.to_bytes());

        let point = decaf377::Element::GENERATOR * Fr::rand(&mut rng);

        // Native: c_truncated × point
        let expected = point * c_truncated;

        // Circuit: scalar_mul_le with Fq bits
        let c_var = FqVar::new_input(cs.clone(), || Ok(c_fq)).unwrap();
        let c_bits = c_var.to_bits_le().unwrap();
        let point_var = ElementVar::new_witness(cs.clone(), || Ok(point)).unwrap();
        let result_var = point_var.scalar_mul_le(c_bits.iter()).unwrap();

        let expected_var = ElementVar::new_witness(cs.clone(), || Ok(expected)).unwrap();
        result_var.enforce_equal(&expected_var).unwrap();

        assert!(
            cs.is_satisfied().unwrap(),
            "scalar_mul with truncated Fq should work"
        );
    }
}
