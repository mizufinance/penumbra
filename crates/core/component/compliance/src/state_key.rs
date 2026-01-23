/// State key for the asset Indexed Merkle Tree (IMT)
pub fn asset_imt() -> &'static str {
    "compliance/asset_imt"
}

/// State key for the user compliance tree
pub fn user_tree() -> &'static str {
    "compliance/user_tree"
}

/// State key for the user count (number of registered users)
pub fn user_count() -> &'static str {
    "compliance/user_count"
}

/// State key for the asset count (number of registered assets)
pub fn asset_count() -> &'static str {
    "compliance/asset_count"
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

/// State keys for historical anchor storage (following SCT pattern).
///
/// Anchors are stored bidirectionally:
/// - anchor_by_height: height -> anchor (for querying current anchor)
/// - anchor_lookup: anchor -> height (for validating historical anchors)
pub mod anchor {
    use penumbra_sdk_tct::StateCommitment;

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
