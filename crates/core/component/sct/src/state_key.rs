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

pub mod nullifier_generations {
    use crate::nullifier_generation::NullifierTreeId;

    pub fn storage_prefix() -> &'static str {
        "sct/nullifier_generations/"
    }

    pub fn state() -> &'static str {
        "sct/nullifier_generations/state"
    }

    pub fn pending_block() -> &'static str {
        "sct/nullifier_generations/pending_block"
    }

    pub fn retired_record(tree: NullifierTreeId) -> Vec<u8> {
        format!(
            "sct/nullifier_generations/archive/{}/retired_record",
            tree.storage_segment()
        )
        .into_bytes()
    }

    pub fn local_pack_receipt(tree: NullifierTreeId) -> Vec<u8> {
        format!(
            "sct/nullifier_generations/archive/{}/local_pack_receipt",
            tree.storage_segment()
        )
        .into_bytes()
    }

    fn tree_base(tree: NullifierTreeId) -> String {
        format!("sct/nullifier_generations/tree/{}", tree.storage_segment())
    }

    pub fn schema(tree: NullifierTreeId) -> String {
        format!("{}/schema", tree_base(tree))
    }

    pub fn root(tree: NullifierTreeId) -> String {
        format!("{}/root", tree_base(tree))
    }

    pub fn tree_node_prefix(tree: NullifierTreeId) -> Vec<u8> {
        format!("{}/imt/node/", tree_base(tree)).into_bytes()
    }

    pub fn tree_node(tree: NullifierTreeId, level: u8, position: u64) -> Vec<u8> {
        let mut key = tree_node_prefix(tree);
        key.push(level);
        key.extend_from_slice(&position.to_be_bytes());
        key
    }

    pub fn leaf_prefix(tree: NullifierTreeId) -> Vec<u8> {
        format!("{}/imt/leaf/", tree_base(tree)).into_bytes()
    }

    pub fn leaf(tree: NullifierTreeId, position: u64) -> Vec<u8> {
        let mut key = leaf_prefix(tree);
        key.extend_from_slice(&position.to_be_bytes());
        key
    }

    pub fn value_prefix(tree: NullifierTreeId) -> Vec<u8> {
        format!("{}/imt/value/", tree_base(tree)).into_bytes()
    }

    pub fn value(tree: NullifierTreeId, nullifier: [u8; 32]) -> Vec<u8> {
        let mut key = value_prefix(tree);
        key.extend_from_slice(&nullifier);
        key
    }

    pub fn value_desc_prefix(tree: NullifierTreeId) -> Vec<u8> {
        format!("{}/imt/value_desc/", tree_base(tree)).into_bytes()
    }

    pub fn value_desc(tree: NullifierTreeId, descending_key: [u8; 32]) -> Vec<u8> {
        let mut key = value_desc_prefix(tree);
        key.extend_from_slice(&descending_key);
        key
    }

    pub fn leaf_count(tree: NullifierTreeId) -> String {
        format!("{}/leaf_count", tree_base(tree))
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

    pub fn incremental_hash(position: shieldd_sdk_tct::Position, height: u8) -> String {
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

    pub fn incremental_commitment(position: shieldd_sdk_tct::Position) -> String {
        format!(
            "{}{:020}",
            incremental_commitment_prefix(),
            u64::from(position)
        )
    }

    pub fn anchor_by_height(height: u64) -> String {
        format!("sct/tree/anchor_by_height/{}", height)
    }

    pub fn anchor_lookup(anchor: shieldd_sdk_tct::Root) -> String {
        format!("sct/tree/anchor_lookup/{}", anchor)
    }

    pub fn note_source(note_commitment: &shieldd_sdk_tct::StateCommitment) -> String {
        format!("sct/tree/note_source/{}", note_commitment)
    }
}

pub mod cache {
    pub fn cached_state_commitment_tree() -> &'static str {
        "sct/cache/cached_state_commitment_tree"
    }

    pub fn block_materialization() -> &'static str {
        "sct/cache/block_materialization"
    }
}

pub mod ambient {
    pub fn current_source() -> &'static str {
        "sct/ambient/current_source"
    }
}
