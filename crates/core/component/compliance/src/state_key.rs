pub mod config {
    pub fn compliance_params() -> &'static str {
        "compliance/config/params"
    }
}

/// State key for the user compliance tree root.
pub fn user_tree_root() -> &'static str {
    "compliance/user_tree_root"
}

/// State key for the asset IMT root.
pub fn asset_imt_root() -> &'static str {
    "compliance/asset_imt_root"
}

/// Nonverifiable tree node storage.
pub mod tree_storage {
    pub fn user_node_prefix() -> &'static str {
        "compliance/tree/user/node/"
    }

    pub fn user_node(level: u8, position: u64) -> String {
        format!("{}{:03}/{:020}", user_node_prefix(), level, position)
    }

    pub fn asset_node_prefix() -> &'static str {
        "compliance/tree/asset/node/"
    }

    pub fn asset_node(level: u8, position: u64) -> String {
        format!("{}{:03}/{:020}", asset_node_prefix(), level, position)
    }

    pub fn asset_leaf_prefix() -> &'static str {
        "compliance/tree/asset/leaf/"
    }

    pub fn asset_leaf(position: u64) -> String {
        format!("{}{:020}", asset_leaf_prefix(), position)
    }

    pub fn asset_value_desc_prefix() -> &'static str {
        "compliance/tree/asset/value_desc/"
    }

    pub fn asset_value_desc(descending_value_key: [u8; 32]) -> String {
        format!(
            "{}{}",
            asset_value_desc_prefix(),
            hex::encode(descending_value_key)
        )
    }
}

/// State key for the user count (number of registered users)
pub fn user_count() -> &'static str {
    "compliance/user_count"
}

/// State key for the asset count (number of registered assets)
pub fn asset_count() -> &'static str {
    "compliance/asset_count"
}

/// Prefix for compliance registrar verification keys.
pub fn compliance_registrar_vk_prefix() -> &'static str {
    "compliance/registrar/vk/"
}

/// State key for a compliance registrar verification key.
pub fn compliance_registrar_vk(
    vk: &decaf377_rdsa::VerificationKey<decaf377_rdsa::SpendAuth>,
) -> String {
    format!(
        "{}{}",
        compliance_registrar_vk_prefix(),
        hex::encode(vk.to_bytes())
    )
}

/// Object-store keys for compliance in-block caches.
pub mod cache {
    /// Cached asset policies for this state delta.
    pub fn cached_asset_policies() -> &'static str {
        "compliance/cache/asset_policies"
    }

    /// Cached deserialized user tree for this state delta.
    pub fn cached_user_tree() -> &'static str {
        "compliance/cache/user_tree"
    }

    /// Cached deserialized asset IMT for this state delta.
    pub fn cached_asset_imt() -> &'static str {
        "compliance/cache/asset_imt"
    }

    /// Dirty flag indicating whether either compliance tree was modified in this block.
    pub fn trees_modified() -> &'static str {
        "compliance/cache/trees_modified"
    }
}

/// State key for asset-specific compliance policy (dk_pub, threshold).
/// This stores issuer-defined policies for threshold-based flagging.
pub fn asset_policy(asset_id: &penumbra_sdk_asset::asset::Id) -> String {
    format!("compliance/asset_policy/{}", asset_id)
}

/// State key for regulated external IBC origin lookup keyed by base denom.
pub fn ibc_origin_asset(base_denom: &str) -> String {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"pen.ibc.origin")
        .hash(base_denom.as_bytes());
    format!("compliance/ibc_origin/{}", hex::encode(hash.as_bytes()))
}

/// State key for reverse lookup: (address, asset_id) -> position in user tree
/// This enables O(1) lookup of a user's leaf position for merkle path generation
pub fn user_leaf_position(
    address: &penumbra_sdk_keys::Address,
    asset_id: &penumbra_sdk_asset::asset::Id,
) -> String {
    format!("compliance/user_lookup/{}/{}", address, asset_id)
}

/// State key for storing the full ComplianceLeaf data for a user
/// This allows retrieving the complete leaf (including ACK) for proof generation
pub fn user_leaf_data(
    address: &penumbra_sdk_keys::Address,
    asset_id: &penumbra_sdk_asset::asset::Id,
) -> String {
    format!("compliance/user_leaf/{}/{}", address, asset_id)
}

/// State key for pending user registrations (buffered during block execution).
///
/// These are accumulated during transaction processing and drained when
/// building the CompactBlock, following the SCT pending_note_payloads pattern.
pub fn pending_user_registrations() -> &'static str {
    "compliance/pending_user_registrations"
}

/// State key for pending asset registrations (buffered during block execution).
///
/// These are accumulated during transaction processing and drained when
/// building the CompactBlock, following the SCT pending_note_payloads pattern.
pub fn pending_asset_registrations() -> &'static str {
    "compliance/pending_asset_registrations"
}

/// State key for IBC compliance metadata keyed by ICS-20 transfer identifiers.
/// Stores the compliance metadata from the sending chain for IBC-bridged regulated assets.
/// Keyed by (channel_id, packet_seq) which matches CommitmentSource::Ics20Transfer.
pub fn ibc_compliance_metadata(channel_id: &str, packet_seq: u64) -> String {
    format!("compliance/ibc/{}/{}", channel_id, packet_seq)
}

/// State keys for historical anchor storage (following SCT pattern).
///
/// Anchors are stored bidirectionally:
/// - anchor_by_height: height -> anchor (for querying current anchor)
/// - anchor_lookup: anchor -> height (for validating historical anchors)
pub mod anchor {
    use penumbra_sdk_tct::StateCommitment;

    /// State key for the greatest anchor height pruned from retention storage.
    pub fn pruned_through_height() -> &'static str {
        "compliance/anchor/pruned_through_height"
    }

    /// State key for user tree anchor at a specific block height.
    pub fn user_anchor_by_height(height: u64) -> String {
        format!("compliance/anchor/user/by_height/{}", height)
    }

    /// State key for reverse lookup: user tree anchor -> block height.
    /// Used to validate that a given anchor was valid at some historical point.
    pub fn user_anchor_lookup(anchor: &StateCommitment) -> String {
        format!("compliance/anchor/user/lookup/{}", anchor.0)
    }

    /// State key for asset IMT anchor at a specific block height.
    pub fn asset_anchor_by_height(height: u64) -> String {
        format!("compliance/anchor/asset/by_height/{}", height)
    }

    /// State key for reverse lookup: asset IMT anchor -> block height.
    /// Used to validate that a given anchor was valid at some historical point.
    pub fn asset_anchor_lookup(anchor: &StateCommitment) -> String {
        format!("compliance/anchor/asset/lookup/{}", anchor.0)
    }
}
