use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
};

use anyhow::{ensure, Context, Result};
use decaf377::Fq;
use sha2::{Digest, Sha256};

use crate::{
    indexed_nullifier_tree::{
        hash_children, FqOrdKey, IndexedNullifierLeaf, IndexedNullifierWitness, CAPACITY, DEPTH,
        ZERO_HASHES,
    },
    nullifier_generation::{
        ArchivedNullifierProof, NullifierGenerationArchived, NullifierGenerationPackReceipt,
        PROTOCOL_VERSION,
    },
    Nullifier,
};

const MAGIC: &[u8; 8] = b"SHNFPACK";
const HEADER_BYTES: usize = 76;
const CHECKSUM_BYTES: usize = 32;
const MIN_PACK_BYTES: usize = HEADER_BYTES + CHECKSUM_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenerationPackMetadata {
    pub protocol_version: u32,
    pub generation_index: u64,
    pub generation_root: [u8; 32],
    pub generation_start_position: u64,
    pub generation_end_position: u64,
    /// Includes the implicit lower sentinel at position zero.
    pub leaf_count: u64,
}

impl GenerationPackMetadata {
    pub fn from_archived(archived: NullifierGenerationArchived, leaf_count: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            generation_index: archived.generation_index,
            generation_root: archived.generation_root,
            generation_start_position: archived.generation_start_position,
            generation_end_position: archived.generation_end_position,
            leaf_count,
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.protocol_version == PROTOCOL_VERSION,
            "unsupported nullifier generation pack protocol"
        );
        ensure!(
            self.generation_start_position <= self.generation_end_position,
            "generation pack position range is invalid"
        );
        ensure!(
            (1..=CAPACITY).contains(&self.leaf_count),
            "generation pack leaf count is invalid"
        );
        Fq::from_bytes_checked(&self.generation_root)
            .map_err(|_| anyhow::anyhow!("generation pack root is not canonical"))?;
        Ok(())
    }

    pub fn matches_archived(&self, archived: NullifierGenerationArchived) -> bool {
        self.protocol_version == PROTOCOL_VERSION
            && self.generation_index == archived.generation_index
            && self.generation_root == archived.generation_root
            && self.generation_start_position == archived.generation_start_position
            && self.generation_end_position == archived.generation_end_position
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let metadata = Self {
            protocol_version: read_u32(bytes, 8)?,
            generation_index: read_u64(bytes, 12)?,
            generation_root: read_array::<32>(bytes, 20)?,
            generation_start_position: read_u64(bytes, 52)?,
            generation_end_position: read_u64(bytes, 60)?,
            leaf_count: read_u64(bytes, 68)?,
        };
        metadata.validate()?;
        Ok(metadata)
    }

    fn byte_length(&self) -> Result<usize> {
        let ordinary_count = self
            .leaf_count
            .checked_sub(1)
            .context("generation pack omits its sentinel leaf")?;
        let ordinary_count = usize::try_from(ordinary_count)
            .context("generation pack is too large for this platform")?;
        MIN_PACK_BYTES
            .checked_add(
                ordinary_count
                    .checked_mul(32)
                    .context("generation pack byte length overflow")?,
            )
            .context("generation pack byte length overflow")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NullifierGenerationPack {
    pub metadata: GenerationPackMetadata,
    /// Ordinary leaves in insertion order. Position zero is implicit.
    pub nullifiers: Vec<Nullifier>,
}

impl NullifierGenerationPack {
    pub fn new(archived: NullifierGenerationArchived, nullifiers: Vec<Nullifier>) -> Result<Self> {
        let leaf_count = u64::try_from(nullifiers.len())?
            .checked_add(1)
            .context("generation pack leaf count overflow")?;
        let pack = Self {
            metadata: GenerationPackMetadata::from_archived(archived, leaf_count),
            nullifiers,
        };
        pack.validate()?;
        Ok(pack)
    }

    pub fn validate(&self) -> Result<()> {
        self.metadata.validate()?;
        ensure!(
            self.metadata.leaf_count == u64::try_from(self.nullifiers.len())? + 1,
            "generation pack leaf count does not match its nullifier stream"
        );
        let mut values = BTreeSet::new();
        for nullifier in &self.nullifiers {
            ensure!(
                values.insert(FqOrdKey::from(nullifier.0)),
                "generation pack contains a duplicate nullifier"
            );
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let nullifier_bytes = self
            .nullifiers
            .len()
            .checked_mul(32)
            .context("generation pack byte length overflow")?;
        let body_len = HEADER_BYTES
            .checked_add(nullifier_bytes)
            .context("generation pack byte length overflow")?;
        let mut bytes = Vec::with_capacity(
            body_len
                .checked_add(CHECKSUM_BYTES)
                .context("generation pack byte length overflow")?,
        );
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&self.metadata.protocol_version.to_be_bytes());
        bytes.extend_from_slice(&self.metadata.generation_index.to_be_bytes());
        bytes.extend_from_slice(&self.metadata.generation_root);
        bytes.extend_from_slice(&self.metadata.generation_start_position.to_be_bytes());
        bytes.extend_from_slice(&self.metadata.generation_end_position.to_be_bytes());
        bytes.extend_from_slice(&self.metadata.leaf_count.to_be_bytes());
        for nullifier in &self.nullifiers {
            bytes.extend_from_slice(&nullifier.to_bytes());
        }
        debug_assert_eq!(bytes.len(), body_len);
        let checksum = Sha256::digest(&bytes);
        bytes.extend_from_slice(&checksum);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let metadata = inspect_bytes(bytes)?.metadata;
        let body_len = bytes.len() - CHECKSUM_BYTES;
        let ordinary_count = usize::try_from(metadata.leaf_count - 1)?;

        let mut nullifiers = Vec::with_capacity(ordinary_count);
        for chunk in bytes[HEADER_BYTES..body_len].chunks_exact(32) {
            nullifiers.push(Nullifier::try_from(chunk)?);
        }
        let pack = Self {
            metadata,
            nullifiers,
        };
        pack.validate()?;
        Ok(pack)
    }

    pub fn reconstruct(&self) -> Result<ReconstructedGeneration> {
        ReconstructedGeneration::new(self)
    }

    pub fn receipt(&self, bytes: &[u8]) -> Result<NullifierGenerationPackReceipt> {
        ensure!(self.encode()? == bytes, "pack bytes are not canonical");
        let receipt = NullifierGenerationPackReceipt {
            protocol_version: self.metadata.protocol_version,
            generation_index: self.metadata.generation_index,
            generation_root: self.metadata.generation_root,
            generation_start_position: self.metadata.generation_start_position,
            generation_end_position: self.metadata.generation_end_position,
            leaf_count: self.metadata.leaf_count,
            byte_length: u64::try_from(bytes.len())?,
            pack_sha256: Sha256::digest(bytes).into(),
        };
        receipt.validate()?;
        Ok(receipt)
    }
}

struct PackInspection {
    metadata: GenerationPackMetadata,
}

fn inspect_bytes(bytes: &[u8]) -> Result<PackInspection> {
    ensure!(
        bytes.len() >= MIN_PACK_BYTES,
        "generation pack is shorter than its fixed header and checksum"
    );
    ensure!(
        &bytes[..MAGIC.len()] == MAGIC,
        "invalid generation pack magic"
    );
    let metadata = GenerationPackMetadata::decode(bytes)?;
    ensure!(
        bytes.len() == metadata.byte_length()?,
        "generation pack length does not match its leaf count"
    );
    let body_len = bytes.len() - CHECKSUM_BYTES;
    let checksum: [u8; 32] = Sha256::digest(&bytes[..body_len]).into();
    ensure!(
        checksum.as_slice() == &bytes[body_len..],
        "generation pack checksum mismatch"
    );
    Ok(PackInspection { metadata })
}

#[derive(Debug)]
pub struct ReconstructedGeneration {
    metadata: GenerationPackMetadata,
    leaves: Vec<IndexedNullifierLeaf>,
    ordered: Vec<(FqOrdKey, u64)>,
    levels: Vec<Vec<Fq>>,
}

impl ReconstructedGeneration {
    fn new(pack: &NullifierGenerationPack) -> Result<Self> {
        pack.validate()?;
        let mut ordered = pack
            .nullifiers
            .iter()
            .copied()
            .enumerate()
            .map(|(index, nullifier)| {
                Ok((
                    FqOrdKey::from(nullifier.0),
                    u64::try_from(index)?
                        .checked_add(1)
                        .context("leaf position overflow")?,
                    nullifier,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        ordered.sort_unstable_by_key(|(key, _, _)| *key);

        let leaf_count = usize::try_from(pack.metadata.leaf_count)?;
        let mut leaves = vec![IndexedNullifierLeaf::lower_sentinel(); leaf_count];
        if let Some((_, first_position, first)) = ordered.first().copied() {
            leaves[0] = IndexedNullifierLeaf {
                next_index: first_position,
                next_value: first.to_bytes(),
                is_terminal: false,
                ..IndexedNullifierLeaf::lower_sentinel()
            };
        }
        for (index, (_, position, value)) in ordered.iter().copied().enumerate() {
            let successor = ordered.get(index + 1).copied();
            let leaf = match successor {
                Some((_, next_position, next_value)) => IndexedNullifierLeaf::ordinary(
                    value,
                    next_position,
                    next_value.to_bytes(),
                    false,
                ),
                None => IndexedNullifierLeaf::ordinary(value, 0, Fq::from(0u64).to_bytes(), true),
            };
            leaves[usize::try_from(position)?] = leaf;
        }

        let mut levels = Vec::with_capacity(DEPTH as usize + 1);
        levels.push(
            leaves
                .iter()
                .map(IndexedNullifierLeaf::commitment)
                .collect::<Result<Vec<_>>>()?,
        );
        for level in 0..DEPTH as usize {
            let children = &levels[level];
            let parent_count = children.len().div_ceil(4);
            let mut parents = Vec::with_capacity(parent_count);
            for parent in 0..parent_count {
                let base = parent * 4;
                let child = |offset: usize| {
                    children
                        .get(base + offset)
                        .copied()
                        .unwrap_or(ZERO_HASHES[level])
                };
                parents.push(hash_children([child(0), child(1), child(2), child(3)]));
            }
            levels.push(parents);
        }
        let root = levels
            .get(DEPTH as usize)
            .and_then(|level| level.first())
            .context("reconstructed generation has no root")?
            .to_bytes();
        ensure!(
            root == pack.metadata.generation_root,
            "reconstructed generation root does not match pack metadata"
        );

        Ok(Self {
            metadata: pack.metadata,
            leaves,
            ordered: ordered
                .into_iter()
                .map(|(key, position, _)| (key, position))
                .collect(),
            levels,
        })
    }

    pub fn root(&self) -> [u8; 32] {
        self.metadata.generation_root
    }

    pub fn nonmembership_proof(&self, nullifier: Nullifier) -> Result<Box<ArchivedNullifierProof>> {
        let target = FqOrdKey::from(nullifier.0);
        let predecessor_position = match self.ordered.binary_search_by_key(&target, |(key, _)| *key)
        {
            Ok(_) => anyhow::bail!("nullifier was spent in archived generation"),
            Err(0) => 0,
            Err(index) => self.ordered[index - 1].1,
        };
        let leaf = *self
            .leaves
            .get(usize::try_from(predecessor_position)?)
            .context("predecessor leaf is missing")?;
        let mut position = predecessor_position;
        let mut auth_path = Vec::with_capacity(DEPTH as usize);
        for level in 0..DEPTH as usize {
            let child_index = usize::try_from(position % 4)?;
            let base = usize::try_from(position / 4)? * 4;
            let siblings = (0..4)
                .filter(|index| *index != child_index)
                .map(|index| {
                    self.levels[level]
                        .get(base + index)
                        .copied()
                        .unwrap_or(ZERO_HASHES[level])
                        .to_bytes()
                })
                .collect::<Vec<_>>();
            auth_path.push(
                siblings
                    .try_into()
                    .expect("one of four children was omitted"),
            );
            position /= 4;
        }
        let proof = Box::new(ArchivedNullifierProof {
            generation_index: self.metadata.generation_index,
            generation_root: self.metadata.generation_root,
            generation_start_position: self.metadata.generation_start_position,
            generation_end_position: self.metadata.generation_end_position,
            witness: IndexedNullifierWitness {
                leaf_position: predecessor_position,
                leaf,
                auth_path,
            },
        });
        proof.verify_for(nullifier)?;
        Ok(proof)
    }
}

#[derive(Debug)]
struct PackCache {
    generations: BTreeMap<u64, Arc<ReconstructedGeneration>>,
    order: VecDeque<u64>,
    loading: BTreeSet<u64>,
}

#[derive(Clone, Debug)]
pub struct GenerationPackRepository {
    directory: Arc<PathBuf>,
    max_cached_generations: usize,
    cache: Arc<Mutex<PackCache>>,
    cache_ready: Arc<Condvar>,
}

impl GenerationPackRepository {
    pub fn new(directory: PathBuf, max_cached_generations: usize) -> Result<Self> {
        ensure!(
            max_cached_generations > 0,
            "generation pack cache must retain at least one generation"
        );
        Ok(Self {
            directory: Arc::new(directory),
            max_cached_generations,
            cache: Arc::new(Mutex::new(PackCache {
                generations: BTreeMap::new(),
                order: VecDeque::new(),
                loading: BTreeSet::new(),
            })),
            cache_ready: Arc::new(Condvar::new()),
        })
    }

    pub fn directory(&self) -> &Path {
        self.directory.as_path()
    }

    pub fn path(&self, generation_index: u64) -> PathBuf {
        self.directory
            .join(format!("generation-{generation_index:020}.ngp"))
    }

    pub fn contains(&self, generation_index: u64) -> bool {
        self.path(generation_index).is_file()
    }

    pub fn quarantine(&self, generation_index: u64) -> Result<Option<PathBuf>> {
        let path = self.path(generation_index);
        if !path.exists() {
            return Ok(None);
        }
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis();
        let quarantine = path.with_extension(format!("ngp.corrupt-{suffix}"));
        fs::rename(&path, &quarantine)?;
        File::open(self.directory())?.sync_all()?;
        self.cache
            .lock()
            .map_err(|_| anyhow::anyhow!("generation pack cache lock is poisoned"))?
            .generations
            .remove(&generation_index);
        Ok(Some(quarantine))
    }

    fn read_bytes(&self, generation_index: u64) -> Result<Vec<u8>> {
        let path = self.path(generation_index);
        let mut file = File::open(&path)
            .with_context(|| format!("open generation pack {}", path.display()))?;
        let length = usize::try_from(file.metadata()?.len())
            .context("generation pack is too large for this platform")?;
        ensure!(
            length >= MIN_PACK_BYTES,
            "generation pack file is truncated"
        );
        let mut bytes = Vec::with_capacity(length);
        file.read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == length,
            "generation pack changed while being read"
        );
        Ok(bytes)
    }

    fn read_pack(
        &self,
        archived: NullifierGenerationArchived,
    ) -> Result<(NullifierGenerationPack, Vec<u8>)> {
        let bytes = self.read_bytes(archived.generation_index)?;
        let pack = NullifierGenerationPack::decode(&bytes)?;
        ensure!(
            pack.metadata.matches_archived(archived),
            "generation pack metadata does not match retired generation"
        );
        Ok((pack, bytes))
    }

    fn load_pack(
        &self,
        archived: NullifierGenerationArchived,
    ) -> Result<(NullifierGenerationPack, Vec<u8>, ReconstructedGeneration)> {
        let (pack, bytes) = self.read_pack(archived)?;
        let reconstructed = pack.reconstruct()?;
        Ok((pack, bytes, reconstructed))
    }

    /// Validate the canonical bytes and retired-generation metadata without rebuilding the tree.
    pub fn inspect(
        &self,
        archived: NullifierGenerationArchived,
    ) -> Result<NullifierGenerationPackReceipt> {
        let bytes = self.read_bytes(archived.generation_index)?;
        let inspection = inspect_bytes(&bytes)?;
        ensure!(
            inspection.metadata.matches_archived(archived),
            "generation pack metadata does not match retired generation"
        );
        let receipt = NullifierGenerationPackReceipt {
            protocol_version: inspection.metadata.protocol_version,
            generation_index: inspection.metadata.generation_index,
            generation_root: inspection.metadata.generation_root,
            generation_start_position: inspection.metadata.generation_start_position,
            generation_end_position: inspection.metadata.generation_end_position,
            leaf_count: inspection.metadata.leaf_count,
            byte_length: u64::try_from(bytes.len())?,
            pack_sha256: Sha256::digest(&bytes).into(),
        };
        receipt.validate()?;
        Ok(receipt)
    }

    pub fn verify(
        &self,
        archived: NullifierGenerationArchived,
    ) -> Result<NullifierGenerationPackReceipt> {
        let (pack, bytes, _) = self.load_pack(archived)?;
        pack.receipt(&bytes)
    }

    pub fn write(&self, pack: &NullifierGenerationPack) -> Result<NullifierGenerationPackReceipt> {
        fs::create_dir_all(self.directory())?;
        let bytes = pack.encode()?;
        pack.reconstruct()?;
        let final_path = self.path(pack.metadata.generation_index);
        if final_path.exists() {
            let archived = archived_from_metadata(pack.metadata);
            let (existing, existing_bytes, _) = self.load_pack(archived)?;
            ensure!(
                existing == *pack,
                "existing generation pack has different contents"
            );
            return existing.receipt(&existing_bytes);
        }

        let temporary = final_path.with_extension("ngp.partial");
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .with_context(|| format!("create temporary pack {}", temporary.display()))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);

        let durable = fs::read(&temporary)?;
        let decoded = NullifierGenerationPack::decode(&durable)?;
        ensure!(
            decoded == *pack,
            "durable generation pack changed before rename"
        );
        decoded.reconstruct()?;
        fs::rename(&temporary, &final_path)?;
        File::open(self.directory())?.sync_all()?;
        decoded.receipt(&durable)
    }

    pub fn nonmembership_proof(
        &self,
        archived: NullifierGenerationArchived,
        nullifier: Nullifier,
    ) -> Result<Box<ArchivedNullifierProof>> {
        let generation = archived.generation_index;
        loop {
            let mut cache = self
                .cache
                .lock()
                .map_err(|_| anyhow::anyhow!("generation pack cache lock is poisoned"))?;
            if let Some(reconstructed) = cache.generations.get(&generation).cloned() {
                ensure!(
                    reconstructed.metadata.matches_archived(archived),
                    "cached generation metadata does not match retired generation"
                );
                touch(&mut cache.order, generation);
                drop(cache);
                return reconstructed.nonmembership_proof(nullifier);
            }
            if cache.loading.insert(generation) {
                break;
            }
            drop(
                self.cache_ready
                    .wait(cache)
                    .map_err(|_| anyhow::anyhow!("generation pack cache lock is poisoned"))?,
            );
        }

        let loaded = self
            .load_pack(archived)
            .map(|(_, _, reconstructed)| Arc::new(reconstructed));
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("generation pack cache lock is poisoned"))?;
        cache.loading.remove(&generation);
        self.cache_ready.notify_all();
        let reconstructed = loaded?;
        while cache.generations.len() >= self.max_cached_generations {
            let oldest = cache
                .order
                .pop_front()
                .context("generation pack cache order is empty")?;
            cache.generations.remove(&oldest);
        }
        cache.generations.insert(generation, reconstructed.clone());
        touch(&mut cache.order, generation);
        drop(cache);
        reconstructed.nonmembership_proof(nullifier)
    }
}

fn touch(order: &mut VecDeque<u64>, generation: u64) {
    if let Some(index) = order.iter().position(|entry| *entry == generation) {
        order.remove(index);
    }
    order.push_back(generation);
}

fn archived_from_metadata(metadata: GenerationPackMetadata) -> NullifierGenerationArchived {
    NullifierGenerationArchived {
        generation_index: metadata.generation_index,
        generation_start_position: metadata.generation_start_position,
        generation_end_position: metadata.generation_end_position,
        generation_root: metadata.generation_root,
        archived_generation_count: 0,
        archived_history_head: [0; 32],
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(read_array(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_be_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N]> {
    let end = offset
        .checked_add(N)
        .context("pack field offset overflow")?;
    bytes
        .get(offset..end)
        .context("generation pack header is truncated")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("generation pack field has an invalid length"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn archived(root: [u8; 32]) -> NullifierGenerationArchived {
        NullifierGenerationArchived {
            generation_index: 7,
            generation_start_position: 11,
            generation_end_position: 19,
            generation_root: root,
            archived_generation_count: 8,
            archived_history_head: Fq::from(9u64).to_bytes(),
        }
    }

    fn pack(values: &[u64]) -> Result<NullifierGenerationPack> {
        let placeholder = archived(Fq::from(0u64).to_bytes());
        let mut pack = NullifierGenerationPack::new(
            placeholder,
            values
                .iter()
                .map(|value| Nullifier(Fq::from(*value)))
                .collect(),
        )?;
        let root = {
            // Construct once without the final metadata check to obtain the expected root.
            let mut metadata = pack.metadata;
            metadata.generation_root = ZERO_HASHES[DEPTH as usize].to_bytes();
            let mut leaves_only = pack.clone();
            leaves_only.metadata = metadata;
            reconstructed_root_without_check(&leaves_only)?
        };
        pack.metadata.generation_root = root;
        Ok(pack)
    }

    fn reconstructed_root_without_check(pack: &NullifierGenerationPack) -> Result<[u8; 32]> {
        let mut ordered = pack
            .nullifiers
            .iter()
            .copied()
            .enumerate()
            .map(|(index, value)| (FqOrdKey::from(value.0), index as u64 + 1, value))
            .collect::<Vec<_>>();
        ordered.sort_unstable_by_key(|entry| entry.0);
        let mut leaves = vec![IndexedNullifierLeaf::lower_sentinel(); pack.nullifiers.len() + 1];
        if let Some((_, position, value)) = ordered.first().copied() {
            leaves[0] = IndexedNullifierLeaf {
                next_index: position,
                next_value: value.to_bytes(),
                is_terminal: false,
                ..IndexedNullifierLeaf::lower_sentinel()
            };
        }
        for (index, (_, position, value)) in ordered.iter().copied().enumerate() {
            leaves[position as usize] =
                if let Some((_, next_position, next)) = ordered.get(index + 1).copied() {
                    IndexedNullifierLeaf::ordinary(value, next_position, next.to_bytes(), false)
                } else {
                    IndexedNullifierLeaf::ordinary(value, 0, Fq::from(0u64).to_bytes(), true)
                };
        }
        let mut current = leaves
            .iter()
            .map(IndexedNullifierLeaf::commitment)
            .collect::<Result<Vec<_>>>()?;
        for level in 0..DEPTH as usize {
            current = current
                .chunks(4)
                .map(|chunk| {
                    let child = |index| chunk.get(index).copied().unwrap_or(ZERO_HASHES[level]);
                    hash_children([child(0), child(1), child(2), child(3)])
                })
                .collect();
        }
        Ok(current[0].to_bytes())
    }

    #[test]
    fn codec_is_canonical_and_rejects_corruption() -> Result<()> {
        let pack = pack(&[7, 1, 12])?;
        let bytes = pack.encode()?;
        assert_eq!(NullifierGenerationPack::decode(&bytes)?, pack);
        let mut corrupt = bytes.clone();
        corrupt[HEADER_BYTES] ^= 1;
        assert!(NullifierGenerationPack::decode(&corrupt).is_err());
        assert!(NullifierGenerationPack::decode(&bytes[..bytes.len() - 1]).is_err());
        Ok(())
    }

    #[test]
    fn reconstruction_proves_every_gap_and_rejects_membership() -> Result<()> {
        let pack = pack(&[7, 1, 12])?;
        let tree = pack.reconstruct()?;
        for value in [0u64, 2, 8, 13] {
            tree.nonmembership_proof(Nullifier(Fq::from(value)))?;
        }
        for value in [1u64, 7, 12] {
            assert!(tree
                .nonmembership_proof(Nullifier(Fq::from(value)))
                .is_err());
        }
        Ok(())
    }

    #[test]
    fn repository_reloads_verified_pack_and_quarantines_damage() -> Result<()> {
        let directory = TempDir::new()?;
        let repository = GenerationPackRepository::new(directory.path().to_path_buf(), 1)?;
        let pack = pack(&[7, 1, 12])?;
        let archived = archived(pack.metadata.generation_root);
        let receipt = repository.write(&pack)?;
        assert_eq!(repository.verify(archived)?, receipt);
        repository
            .nonmembership_proof(archived, Nullifier(Fq::from(8u64)))?
            .verify_for(Nullifier(Fq::from(8u64)))?;

        let path = repository.path(archived.generation_index);
        let mut damaged = fs::read(&path)?;
        damaged[HEADER_BYTES] ^= 1;
        fs::write(&path, damaged)?;
        assert!(repository.verify(archived).is_err());
        let quarantine = repository
            .quarantine(archived.generation_index)?
            .context("damaged pack was not quarantined")?;
        assert!(!path.exists());
        assert!(quarantine.exists());
        Ok(())
    }

    #[test]
    fn inspection_is_cheap_but_full_verification_still_checks_the_root() -> Result<()> {
        let directory = TempDir::new()?;
        let repository = GenerationPackRepository::new(directory.path().to_path_buf(), 1)?;
        let original = pack(&[7, 1, 12])?;
        let archived = archived(original.metadata.generation_root);
        let mut wrong_contents = original.clone();
        wrong_contents.nullifiers[0] = Nullifier(Fq::from(8u64));
        fs::create_dir_all(repository.directory())?;
        fs::write(
            repository.path(archived.generation_index),
            wrong_contents.encode()?,
        )?;

        repository.inspect(archived)?;
        assert!(repository.verify(archived).is_err());
        Ok(())
    }
}
