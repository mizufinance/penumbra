pub mod config {
    pub fn sct_params() -> &'static str {
        "sct/config/sct_params"
    }
}

pub mod block_manager {
    pub fn block_height() -> &'static str {
        "sct/block_manager/block_height"
    }

    pub fn current_block_timestamp() -> &'static str {
        "sct/block_manager/block_timestamp"
    }

    pub fn block_timestamp(height: u64) -> String {
        format!("sct/block_manager/historical_block_timestamp/{}", height)
    }
}

pub mod epoch_manager {
    pub fn epoch_by_height(height: u64) -> String {
        format!("sct/epoch_manager/epoch_by_height/{}", height)
    }

    pub fn epoch_change_at_height(height: u64) -> String {
        format!("sct/epoch_manager/pending_epoch_changes/{}", height)
    }

    pub fn end_epoch_early() -> &'static str {
        "sct/epoch_manager/end_epoch_early"
    }
}

pub mod nullifier_set {
    use jmt::{storage::NodeKey, KeyHash};

    pub fn root() -> &'static str {
        "sct/nullifier_set/root"
    }

    pub fn tree_node_prefix() -> &'static [u8] {
        b"sct/nullifier_set/jmt/node/"
    }

    pub fn tree_node(node_key: &NodeKey) -> Vec<u8> {
        let mut key = tree_node_prefix().to_vec();
        key.extend(borsh::to_vec(node_key).expect("JMT node key serialization is infallible"));
        key
    }

    pub fn value_prefix() -> &'static [u8] {
        b"sct/nullifier_set/jmt/value/"
    }

    pub fn value(key_hash: KeyHash) -> Vec<u8> {
        let mut key = value_prefix().to_vec();
        key.extend_from_slice(&key_hash.0);
        key
    }

    pub fn rightmost_leaf_node_key() -> &'static [u8] {
        b"sct/nullifier_set/jmt/meta/rightmost_leaf_node_key"
    }

    pub fn rightmost_leaf_node() -> &'static [u8] {
        b"sct/nullifier_set/jmt/meta/rightmost_leaf_node"
    }

    pub fn pending_nullifiers() -> &'static str {
        "sct/nullifier_set/pending_nullifiers"
    }
}

pub mod tree {
    pub fn incremental_prefix() -> &'static str {
        "sct/tree/incremental/"
    }

    pub fn incremental_position() -> &'static str {
        "sct/tree/incremental/meta/position"
    }

    pub fn incremental_forgotten() -> &'static str {
        "sct/tree/incremental/meta/forgotten"
    }

    pub fn incremental_hash_prefix() -> &'static str {
        "sct/tree/incremental/hash/"
    }

    pub fn incremental_hash(position: penumbra_sdk_tct::Position, height: u8) -> String {
        format!(
            "{}{:020}/{:03}",
            incremental_hash_prefix(),
            u64::from(position),
            height
        )
    }

    pub fn incremental_commitment_prefix() -> &'static str {
        "sct/tree/incremental/commitment/"
    }

    pub fn incremental_commitment(position: penumbra_sdk_tct::Position) -> String {
        format!(
            "{}{:020}",
            incremental_commitment_prefix(),
            u64::from(position)
        )
    }

    pub fn anchor_by_height(height: u64) -> String {
        format!("sct/tree/anchor_by_height/{}", height)
    }

    pub fn anchor_lookup(anchor: penumbra_sdk_tct::Root) -> String {
        format!("sct/tree/anchor_lookup/{}", anchor)
    }

    pub fn note_source(note_commitment: &penumbra_sdk_tct::StateCommitment) -> String {
        format!("sct/tree/note_source/{}", note_commitment)
    }
}

pub mod cache {
    pub fn cached_state_commitment_tree() -> &'static str {
        "sct/cache/cached_state_commitment_tree"
    }
}

pub mod ambient {
    pub fn current_source() -> &'static str {
        "sct/ambient/current_source"
    }
}
