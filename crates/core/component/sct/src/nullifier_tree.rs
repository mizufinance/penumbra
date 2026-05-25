use anyhow::{anyhow, ensure, Context, Result};
use borsh::BorshDeserialize;
use cnidarium::{StateRead, StateWrite};
use futures::StreamExt;
use jmt::{
    proof::SparseMerkleProof,
    storage::{LeafNode, Node, NodeKey, TreeReader},
    KeyHash, RootHash, Sha256Jmt, Version,
};
use penumbra_sdk_proto::DomainType as _;
use sha2::Sha256;
use std::{collections::BTreeMap, future::Future};

use crate::{state_key, NullificationInfo, Nullifier};

pub const VERSION: Version = 0;

#[derive(Clone, Debug)]
pub struct NullifierLookup {
    pub root: RootHash,
    pub info: Option<NullificationInfo>,
    pub proof: SparseMerkleProof<Sha256>,
}

struct EmptyReader;

impl TreeReader for EmptyReader {
    fn get_node_option(&self, _node_key: &NodeKey) -> Result<Option<Node>> {
        Ok(None)
    }

    fn get_value_option(
        &self,
        _max_version: Version,
        _key_hash: KeyHash,
    ) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }

    fn get_rightmost_leaf(&self) -> Result<Option<(NodeKey, LeafNode)>> {
        Ok(None)
    }
}

struct NvJmtReader<'a, S: ?Sized> {
    state: &'a S,
}

impl<'a, S: ?Sized> NvJmtReader<'a, S> {
    fn new(state: &'a S) -> Self {
        Self { state }
    }
}

fn key_hash(nullifier: &Nullifier) -> KeyHash {
    let bytes: [u8; 32] = (*nullifier).into();
    KeyHash::with::<Sha256>(bytes)
}

fn decode_node(bytes: Vec<u8>) -> Result<Node> {
    Node::try_from_slice(&bytes).context("decode nullifier JMT node")
}

fn encode_node(node: &Node) -> Result<Vec<u8>> {
    borsh::to_vec(node).context("encode nullifier JMT node")
}

fn decode_node_key(bytes: &[u8]) -> Result<NodeKey> {
    NodeKey::try_from_slice(bytes).context("decode nullifier JMT node key")
}

fn encode_node_key(node_key: &NodeKey) -> Result<Vec<u8>> {
    borsh::to_vec(node_key).context("encode nullifier JMT node key")
}

fn wait_state_read<F, T>(future: F) -> Result<T>
where
    F: Future<Output = Result<T>> + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name("nullifier-tree-state-read".to_string())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("build nullifier tree state-read runtime")?;
            runtime.block_on(future)
        })
        .context("spawn nullifier tree state-read thread")?
        .join()
        .map_err(|_| anyhow!("nullifier tree state-read thread panicked"))?
}

fn read_rightmost_leaf_sync<S: StateRead + ?Sized>(state: &S) -> Result<Option<(NodeKey, Node)>> {
    let Some(raw_key) = wait_state_read(
        state.nonverifiable_get_raw(state_key::nullifier_set::rightmost_leaf_node_key()),
    )?
    else {
        return Ok(None);
    };
    let Some(raw_node) = wait_state_read(
        state.nonverifiable_get_raw(state_key::nullifier_set::rightmost_leaf_node()),
    )?
    else {
        return Ok(None);
    };
    Ok(Some((decode_node_key(&raw_key)?, decode_node(raw_node)?)))
}

fn decode_root(bytes: Vec<u8>) -> Result<RootHash> {
    let bytes: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
        anyhow!("nullifier root must be 32 bytes, got {}", bytes.len())
    })?;
    Ok(RootHash(bytes))
}

pub async fn committed_root<S: StateRead + ?Sized>(state: &S) -> Result<Option<RootHash>> {
    state
        .get_raw(state_key::nullifier_set::root())
        .await?
        .map(decode_root)
        .transpose()
}

async fn read_rightmost_leaf<S: StateRead + ?Sized>(state: &S) -> Result<Option<(NodeKey, Node)>> {
    let Some(raw_key) = state
        .nonverifiable_get_raw(state_key::nullifier_set::rightmost_leaf_node_key())
        .await?
    else {
        return Ok(None);
    };
    let Some(raw_node) = state
        .nonverifiable_get_raw(state_key::nullifier_set::rightmost_leaf_node())
        .await?
    else {
        return Ok(None);
    };
    Ok(Some((decode_node_key(&raw_key)?, decode_node(raw_node)?)))
}

fn max_leaf(left: Option<(NodeKey, Node)>, right: (NodeKey, Node)) -> Option<(NodeKey, Node)> {
    let Node::Leaf(right_leaf) = &right.1 else {
        return left;
    };
    match left {
        Some(existing) => {
            let Node::Leaf(existing_leaf) = &existing.1 else {
                return Some(right);
            };
            if right_leaf.key_hash() > existing_leaf.key_hash() {
                Some(right)
            } else {
                Some(existing)
            }
        }
        None => Some(right),
    }
}

async fn write_update_batch<S: StateWrite + ?Sized>(
    state: &mut S,
    batch: jmt::storage::TreeUpdateBatch,
) -> Result<()> {
    let mut rightmost = read_rightmost_leaf(state).await?;

    for (node_key, node) in batch.node_batch.nodes() {
        state.nonverifiable_put_raw(
            state_key::nullifier_set::tree_node(node_key),
            encode_node(node)?,
        );
        if matches!(node, Node::Leaf(_)) {
            rightmost = max_leaf(rightmost, (node_key.clone(), node.clone()));
        }
    }

    for ((_version, key_hash), value) in batch.node_batch.values() {
        let key = state_key::nullifier_set::value(*key_hash);
        match value {
            Some(value) => state.nonverifiable_put_raw(key, value.clone()),
            None => state.nonverifiable_delete(key),
        }
    }

    if let Some((node_key, node)) = rightmost {
        state.nonverifiable_put_raw(
            state_key::nullifier_set::rightmost_leaf_node_key().to_vec(),
            encode_node_key(&node_key)?,
        );
        state.nonverifiable_put_raw(
            state_key::nullifier_set::rightmost_leaf_node().to_vec(),
            encode_node(&node)?,
        );
    }

    Ok(())
}

pub async fn initialize<S: StateWrite + ?Sized>(state: &mut S) -> Result<()> {
    if committed_root(state).await?.is_some() {
        return Ok(());
    }

    let reader = EmptyReader;
    let tree = Sha256Jmt::new(&reader);
    let (root, batch) =
        tree.put_value_set(std::iter::empty::<(KeyHash, Option<Vec<u8>>)>(), VERSION)?;
    write_update_batch(state, batch).await?;
    state.put_raw(
        state_key::nullifier_set::root().to_string(),
        root.0.to_vec(),
    );
    Ok(())
}

pub async fn verify_committed_root<S: StateRead + ?Sized>(state: &S) -> Result<()> {
    let Some(committed) = committed_root(state).await? else {
        return Ok(());
    };
    let reader = NvJmtReader::new(state);
    let tree = Sha256Jmt::new(&reader);
    let actual = tree
        .get_root_hash_option(VERSION)
        .context("read nullifier tree root from NV storage")?
        .ok_or_else(|| anyhow!("nullifier tree root node missing from NV storage"))?;
    ensure!(
        actual == committed,
        "nullifier tree root mismatch: committed {:?}, NV {:?}",
        committed,
        actual
    );
    Ok(())
}

pub async fn lookup_with_proof<S: StateRead + ?Sized>(
    state: &S,
    nullifier: Nullifier,
) -> Result<NullifierLookup> {
    let root = committed_root(state)
        .await?
        .ok_or_else(|| anyhow!("nullifier tree root missing from state"))?;
    let reader = NvJmtReader::new(state);
    let tree = Sha256Jmt::new(&reader);
    let key = key_hash(&nullifier);
    let (value, proof) = tree
        .get_with_proof(key, VERSION)
        .context("read nullifier JMT proof")?;
    let info = value
        .map(|bytes| NullificationInfo::decode(bytes.as_slice()))
        .transpose()
        .context("decode nullification info")?;
    Ok(NullifierLookup { root, info, proof })
}

pub async fn spend_info<S: StateRead + ?Sized>(
    state: &S,
    nullifier: Nullifier,
) -> Result<Option<NullificationInfo>> {
    lookup_with_proof(state, nullifier)
        .await
        .map(|lookup| lookup.info)
}

pub async fn insert_batch<S: StateWrite + ?Sized>(
    state: &mut S,
    entries: impl IntoIterator<Item = (Nullifier, NullificationInfo)>,
) -> Result<()> {
    initialize(state).await?;

    let mut values = BTreeMap::<KeyHash, Vec<u8>>::new();
    for (nullifier, info) in entries {
        let key = key_hash(&nullifier);
        let value = info.encode_to_vec();
        ensure!(
            values.insert(key, value).is_none(),
            "duplicate nullifier {} in nullifier batch",
            nullifier
        );
    }

    if values.is_empty() {
        return Ok(());
    }

    let reader = NvJmtReader::new(state);
    let tree = Sha256Jmt::new(&reader);
    for key_hash in values.keys().copied() {
        if tree
            .get_with_proof(key_hash, VERSION)
            .context("checking nullifier absence")?
            .0
            .is_some()
        {
            anyhow::bail!("nullifier key {:?} was already spent", key_hash);
        }
    }

    let value_set = values
        .into_iter()
        .map(|(key, value)| (key, Some(value)))
        .collect::<Vec<_>>();

    let has_leaf = reader.get_rightmost_leaf()?.is_some();
    let (root, batch) = if has_leaf {
        tree.append_value_set(value_set, VERSION)?
    } else {
        tree.put_value_set(value_set, VERSION)?
    };

    write_update_batch(state, batch).await?;
    state.put_raw(
        state_key::nullifier_set::root().to_string(),
        root.0.to_vec(),
    );
    Ok(())
}

impl<S: StateRead + ?Sized> TreeReader for NvJmtReader<'_, S> {
    fn get_node_option(&self, node_key: &NodeKey) -> Result<Option<Node>> {
        wait_state_read(
            self.state
                .nonverifiable_get_raw(&state_key::nullifier_set::tree_node(node_key)),
        )?
        .map(decode_node)
        .transpose()
    }

    fn get_value_option(
        &self,
        _max_version: Version,
        key_hash: KeyHash,
    ) -> Result<Option<Vec<u8>>> {
        wait_state_read(
            self.state
                .nonverifiable_get_raw(&state_key::nullifier_set::value(key_hash)),
        )
    }

    fn get_rightmost_leaf(&self) -> Result<Option<(NodeKey, LeafNode)>> {
        if let Some((node_key, Node::Leaf(leaf))) = read_rightmost_leaf_sync(self.state)? {
            return Ok(Some((node_key, leaf)));
        }

        // Defensive recovery path for a missing/corrupt rightmost-leaf cache.
        // Operators should investigate if this appears outside repair/replay.
        tracing::warn!(
            prefix = ?state_key::nullifier_set::tree_node_prefix(),
            "nullifier tree rightmost leaf cache missing; scanning NV tree nodes"
        );
        let stream = self.state.nonverifiable_range_raw(
            Some(state_key::nullifier_set::tree_node_prefix()),
            Vec::new()..,
        )?;
        wait_state_read(async move {
            futures::pin_mut!(stream);
            let mut rightmost: Option<(NodeKey, LeafNode)> = None;
            while let Some(item) = stream.next().await {
                let (key, bytes) = item?;
                let Some(suffix) = key.strip_prefix(state_key::nullifier_set::tree_node_prefix())
                else {
                    continue;
                };
                let node_key = decode_node_key(suffix)?;
                if let Node::Leaf(leaf) = decode_node(bytes)? {
                    rightmost = match rightmost {
                        Some((existing_key, existing_leaf))
                            if existing_leaf.key_hash() >= leaf.key_hash() =>
                        {
                            Some((existing_key, existing_leaf))
                        }
                        _ => Some((node_key, leaf)),
                    }
                }
            }
            Ok(rightmost)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cnidarium::TempStorage;
    use decaf377::Fq;

    fn nullifier(value: u64) -> Nullifier {
        Nullifier(Fq::from(value))
    }

    fn info(value: u8, spend_height: u64) -> NullificationInfo {
        NullificationInfo {
            id: [value; 32],
            spend_height,
        }
    }

    async fn tree_node_keys<S: StateRead>(state: &S) -> Result<Vec<Vec<u8>>> {
        let stream = state.nonverifiable_prefix_raw(state_key::nullifier_set::tree_node_prefix());
        futures::pin_mut!(stream);
        let mut keys = Vec::new();
        while let Some(item) = stream.next().await {
            let (key, _) = item?;
            keys.push(key);
        }
        Ok(keys)
    }

    #[tokio::test]
    async fn empty_tree_initializes_with_nonmembership_proof() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());

        initialize(&mut state).await?;
        let lookup = lookup_with_proof(&state, nullifier(1)).await?;

        assert!(lookup.info.is_none());
        lookup
            .proof
            .verify_nonexistence(lookup.root, key_hash(&nullifier(1)))?;
        verify_committed_root(&state).await?;

        Ok(())
    }

    #[tokio::test]
    async fn single_insert_lookup_and_membership_proof() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        let nf = nullifier(7);
        let spent = info(9, 42);

        insert_batch(&mut state, [(nf, spent)]).await?;
        let lookup = lookup_with_proof(&state, nf).await?;

        assert_eq!(lookup.info, Some(spent));
        lookup
            .proof
            .verify_existence(lookup.root, key_hash(&nf), spent.encode_to_vec())?;
        verify_committed_root(&state).await?;

        Ok(())
    }

    #[tokio::test]
    async fn duplicate_batch_insert_is_rejected_before_mutation() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        let nf = nullifier(11);

        let err = insert_batch(&mut state, [(nf, info(1, 1)), (nf, info(2, 1))])
            .await
            .expect_err("duplicate nullifier should be rejected");

        assert!(err.to_string().contains("duplicate nullifier"));
        assert_eq!(spend_info(&state, nf).await?, None);

        Ok(())
    }

    #[tokio::test]
    async fn already_spent_nullifier_is_rejected() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());
        let nf = nullifier(13);

        insert_batch(&mut state, [(nf, info(1, 1))]).await?;
        let root = committed_root(&state).await?.expect("root initialized");
        let err = insert_batch(&mut state, [(nf, info(2, 2))])
            .await
            .expect_err("already spent nullifier should be rejected");

        assert!(err.to_string().contains("already spent"));
        assert_eq!(committed_root(&state).await?, Some(root));
        assert_eq!(spend_info(&state, nf).await?, Some(info(1, 1)));

        Ok(())
    }

    #[tokio::test]
    async fn batch_root_matches_repeated_insert_root() -> Result<()> {
        let storage = TempStorage::new().await?;
        let entries = vec![
            (nullifier(21), info(1, 10)),
            (nullifier(22), info(2, 10)),
            (nullifier(23), info(3, 10)),
            (nullifier(24), info(4, 10)),
        ];

        let mut repeated = cnidarium::StateDelta::new(storage.latest_snapshot());
        for entry in entries.iter().copied() {
            insert_batch(&mut repeated, [entry]).await?;
        }

        let mut batched = cnidarium::StateDelta::new(storage.latest_snapshot());
        insert_batch(&mut batched, entries).await?;

        assert_eq!(
            committed_root(&repeated).await?,
            committed_root(&batched).await?
        );

        Ok(())
    }

    #[tokio::test]
    async fn batch_insertion_order_is_root_stable() -> Result<()> {
        let storage = TempStorage::new().await?;
        let forward = vec![
            (nullifier(31), info(1, 20)),
            (nullifier(32), info(2, 20)),
            (nullifier(33), info(3, 20)),
            (nullifier(34), info(4, 20)),
        ];
        let mut reverse = forward.clone();
        reverse.reverse();

        let mut first = cnidarium::StateDelta::new(storage.latest_snapshot());
        insert_batch(&mut first, forward).await?;

        let mut second = cnidarium::StateDelta::new(storage.latest_snapshot());
        insert_batch(&mut second, reverse).await?;

        assert_eq!(
            committed_root(&first).await?,
            committed_root(&second).await?
        );

        Ok(())
    }

    #[tokio::test]
    async fn missing_nv_node_fails_closed() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());

        insert_batch(&mut state, [(nullifier(41), info(1, 30))]).await?;
        let keys = tree_node_keys(&state).await?;
        assert!(!keys.is_empty(), "insert should materialize NV tree nodes");
        for key in keys {
            state.nonverifiable_delete(key);
        }

        let err = verify_committed_root(&state)
            .await
            .expect_err("missing NV nodes should fail the startup check");
        assert!(
            err.to_string().contains("root node missing")
                || err.to_string().contains("decode nullifier JMT node")
                || err.to_string().contains("Missing node in DB")
                || err.to_string().contains("missing")
        );

        Ok(())
    }

    #[tokio::test]
    async fn old_jmt_nullifier_keys_are_not_written() -> Result<()> {
        let storage = TempStorage::new().await?;
        let mut state = cnidarium::StateDelta::new(storage.latest_snapshot());

        insert_batch(&mut state, [(nullifier(51), info(1, 40))]).await?;

        let old_prefix = "sct/nullifier_set/spent_nullifier_lookup/";
        let mut stream = state.prefix_raw(old_prefix);
        assert!(
            stream.next().await.is_none(),
            "dedicated nullifier tree must not write old app-JMT nullifier entries"
        );

        Ok(())
    }
}
