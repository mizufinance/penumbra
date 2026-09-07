use anyhow::{ensure, Context};
use decaf377::Fq;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shieldd_sdk_proto::{shieldd::core::component::sct::v1 as pb, DomainType};

use crate::{indexed_nullifier_tree::IndexedNullifierWitness, Nullifier};

pub const PROTOCOL_VERSION: u32 = 2;
pub const GENERATION_EPOCHS: u64 = 30;
pub const CHUNK_WIDTH: u64 = 10;
pub const BLS12_377_PROOF_BYTES: usize = 192;
pub const BW6_761_PROOF_BYTES: usize = 480;

const MAX_SCT_POSITION: u64 = (1u64 << 48) - 1;

static EMPTY_HISTORY_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(
        blake2b_simd::blake2b(b"shieldd.nullifier.history.empty").as_bytes(),
    )
});
static HISTORY_NODE_DOMAIN: Lazy<Fq> = Lazy::new(|| {
    Fq::from_le_bytes_mod_order(blake2b_simd::blake2b(b"shieldd.nullifier.history.node").as_bytes())
});

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArchivedNullifierProof {
    pub generation_index: u64,
    pub generation_root: [u8; 32],
    pub generation_start_position: u64,
    pub generation_end_position: u64,
    pub witness: IndexedNullifierWitness,
}

impl From<ArchivedNullifierProof> for pb::ArchivedNullifierProofResponse {
    fn from(value: ArchivedNullifierProof) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            generation_index: value.generation_index,
            generation_root: value.generation_root.to_vec(),
            generation_start_position: value.generation_start_position,
            generation_end_position: value.generation_end_position,
            witness: Some(value.witness.into()),
        }
    }
}

impl TryFrom<pb::ArchivedNullifierProofResponse> for ArchivedNullifierProof {
    type Error = anyhow::Error;

    fn try_from(value: pb::ArchivedNullifierProofResponse) -> Result<Self, Self::Error> {
        ensure!(
            value.protocol_version == PROTOCOL_VERSION,
            "unsupported archived nullifier proof protocol version"
        );
        let proof = Self {
            generation_index: value.generation_index,
            generation_root: decode32(value.generation_root, "archived generation root")?,
            generation_start_position: value.generation_start_position,
            generation_end_position: value.generation_end_position,
            witness: value
                .witness
                .context("archived indexed nullifier witness is missing")?
                .try_into()?,
        };
        ensure!(
            proof.generation_start_position <= proof.generation_end_position
                && proof.generation_end_position <= MAX_SCT_POSITION,
            "archived generation position range is invalid"
        );
        canonical_fq(proof.generation_root, "archived generation root")?;
        Ok(proof)
    }
}

impl ArchivedNullifierProof {
    /// Verify the exact indexed-tree witness against the retired root.
    pub fn verify_for(&self, nullifier: Nullifier) -> anyhow::Result<()> {
        self.witness
            .verify_nonmembership(nullifier, self.generation_root)
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum NullifierTreeId {
    Generation(u64),
}

impl NullifierTreeId {
    pub fn storage_segment(self) -> String {
        let Self::Generation(index) = self;
        format!("generation/{index:020}")
    }
}

impl DomainType for NullifierTreeId {
    type Proto = pb::NullifierTreeId;
}

impl From<NullifierTreeId> for pb::NullifierTreeId {
    fn from(value: NullifierTreeId) -> Self {
        let NullifierTreeId::Generation(generation) = value;
        Self { generation }
    }
}

impl TryFrom<pb::NullifierTreeId> for NullifierTreeId {
    type Error = anyhow::Error;

    fn try_from(value: pb::NullifierTreeId) -> Result<Self, Self::Error> {
        Ok(Self::Generation(value.generation))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NullifierWindow {
    pub protocol_version: u32,
    pub current_generation: u64,
    pub recent_position_floor: u64,
    pub archived_generation_count: u64,
    pub archived_history_head: [u8; 32],
}

impl DomainType for NullifierWindow {
    type Proto = pb::NullifierWindow;
}

impl From<NullifierWindow> for pb::NullifierWindow {
    fn from(value: NullifierWindow) -> Self {
        Self {
            protocol_version: value.protocol_version,
            current_generation: value.current_generation,
            recent_position_floor: value.recent_position_floor,
            archived_generation_count: value.archived_generation_count,
            archived_history_head: value.archived_history_head.to_vec(),
        }
    }
}

impl TryFrom<pb::NullifierWindow> for NullifierWindow {
    type Error = anyhow::Error;

    fn try_from(value: pb::NullifierWindow) -> Result<Self, Self::Error> {
        let window = Self {
            protocol_version: value.protocol_version,
            current_generation: value.current_generation,
            recent_position_floor: value.recent_position_floor,
            archived_generation_count: value.archived_generation_count,
            archived_history_head: decode32(value.archived_history_head, "archived history head")?,
        };
        window.validate()?;
        Ok(window)
    }
}

impl NullifierWindow {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.protocol_version == PROTOCOL_VERSION,
            "unsupported nullifier protocol version {}",
            self.protocol_version
        );
        ensure!(
            self.archived_generation_count == self.current_generation.saturating_sub(1),
            "archived generation count does not match current generation"
        );
        ensure!(
            self.recent_position_floor <= MAX_SCT_POSITION,
            "recent position floor exceeds the 48-bit SCT position range"
        );
        canonical_fq(self.archived_history_head, "archived history head")?;
        if self.archived_generation_count == 0 {
            ensure!(
                self.archived_history_head == empty_history_head(),
                "empty retired history has a nonempty head"
            );
        }
        Ok(())
    }

    pub fn bundle_shape(self) -> HistoricalBundleShape {
        HistoricalBundleShape {
            completed_chunks: self.archived_generation_count / CHUNK_WIDTH,
            tail_proofs: self.archived_generation_count % CHUNK_WIDTH,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoricalBundleShape {
    pub completed_chunks: u64,
    pub tail_proofs: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::GenerationNonmembershipProof",
    into = "pb::GenerationNonmembershipProof"
)]
pub struct GenerationNonmembershipProof {
    pub generation_index: u64,
    pub generation_root: [u8; 32],
    pub generation_start_position: u64,
    pub generation_end_position: u64,
    pub groth16_proof: Vec<u8>,
}

impl DomainType for GenerationNonmembershipProof {
    type Proto = pb::GenerationNonmembershipProof;
}

impl GenerationNonmembershipProof {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.generation_start_position <= self.generation_end_position
                && self.generation_end_position <= MAX_SCT_POSITION,
            "generation position range is invalid"
        );
        ensure!(
            self.groth16_proof.len() == BLS12_377_PROOF_BYTES,
            "BLS12-377 proof must be exactly {BLS12_377_PROOF_BYTES} bytes"
        );
        canonical_fq(self.generation_root, "generation root")?;
        Ok(())
    }
}

impl From<GenerationNonmembershipProof> for pb::GenerationNonmembershipProof {
    fn from(value: GenerationNonmembershipProof) -> Self {
        Self {
            generation_index: value.generation_index,
            generation_root: value.generation_root.to_vec(),
            generation_start_position: value.generation_start_position,
            generation_end_position: value.generation_end_position,
            groth16_proof: value.groth16_proof,
        }
    }
}

impl TryFrom<pb::GenerationNonmembershipProof> for GenerationNonmembershipProof {
    type Error = anyhow::Error;

    fn try_from(value: pb::GenerationNonmembershipProof) -> Result<Self, Self::Error> {
        let proof = Self {
            generation_index: value.generation_index,
            generation_root: decode32(value.generation_root, "generation root")?,
            generation_start_position: value.generation_start_position,
            generation_end_position: value.generation_end_position,
            groth16_proof: value.groth16_proof,
        };
        proof.validate()?;
        Ok(proof)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::HistoricalChunkProof",
    into = "pb::HistoricalChunkProof"
)]
pub struct HistoricalChunkProof {
    pub chunk_index: u64,
    pub end_history_head: [u8; 32],
    pub groth16_proof: Vec<u8>,
}

impl DomainType for HistoricalChunkProof {
    type Proto = pb::HistoricalChunkProof;
}

impl HistoricalChunkProof {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.groth16_proof.len() == BW6_761_PROOF_BYTES,
            "BW6-761 proof must be exactly {BW6_761_PROOF_BYTES} bytes"
        );
        canonical_fq(self.end_history_head, "chunk end history head")?;
        Ok(())
    }
}

impl From<HistoricalChunkProof> for pb::HistoricalChunkProof {
    fn from(value: HistoricalChunkProof) -> Self {
        Self {
            chunk_index: value.chunk_index,
            end_history_head: value.end_history_head.to_vec(),
            groth16_proof: value.groth16_proof,
        }
    }
}

impl TryFrom<pb::HistoricalChunkProof> for HistoricalChunkProof {
    type Error = anyhow::Error;

    fn try_from(value: pb::HistoricalChunkProof) -> Result<Self, Self::Error> {
        let proof = Self {
            chunk_index: value.chunk_index,
            end_history_head: decode32(value.end_history_head, "chunk end history head")?,
            groth16_proof: value.groth16_proof,
        };
        proof.validate()?;
        Ok(proof)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "pb::HistoricalNullifierProof",
    into = "pb::HistoricalNullifierProof"
)]
pub struct HistoricalNullifierProof {
    pub nullifier: Nullifier,
    pub completed_chunks: Vec<HistoricalChunkProof>,
    pub tail: Vec<GenerationNonmembershipProof>,
}

impl DomainType for HistoricalNullifierProof {
    type Proto = pb::HistoricalNullifierProof;
}

impl HistoricalNullifierProof {
    /// Validate the only legal public bundle shape and reconstruct its terminal history head.
    /// Cryptographic proof verification is deliberately performed by the consensus verifier.
    pub fn validate_structure(&self, window: NullifierWindow) -> anyhow::Result<()> {
        window.validate()?;
        ensure!(
            window.archived_generation_count > 0,
            "historical bundles are forbidden for an empty retired prefix"
        );
        let expected = window.bundle_shape();
        ensure!(
            u64::try_from(self.completed_chunks.len()).ok() == Some(expected.completed_chunks),
            "historical chunk count does not match the current window"
        );
        ensure!(
            u64::try_from(self.tail.len()).ok() == Some(expected.tail_proofs),
            "historical tail count does not match the current window"
        );

        let mut expected_head = empty_history_head();
        for (index, chunk) in self.completed_chunks.iter().enumerate() {
            chunk.validate()?;
            ensure!(
                chunk.chunk_index == index as u64,
                "historical chunks are not in canonical order"
            );
            expected_head = chunk.end_history_head;
        }

        let tail_start = expected
            .completed_chunks
            .checked_mul(CHUNK_WIDTH)
            .context("historical tail index overflow")?;
        for (offset, proof) in self.tail.iter().enumerate() {
            proof.validate()?;
            let expected_generation = tail_start
                .checked_add(offset as u64)
                .context("historical generation index overflow")?;
            ensure!(
                proof.generation_index == expected_generation,
                "historical tail is not in canonical order"
            );
            expected_head = append_history(
                expected_head,
                expected_generation,
                proof.generation_root,
                proof.generation_start_position,
                proof.generation_end_position,
            )?;
        }
        ensure!(
            expected_head == window.archived_history_head,
            "historical bundle does not end at the current history head"
        );
        Ok(())
    }
}

impl From<HistoricalNullifierProof> for pb::HistoricalNullifierProof {
    fn from(value: HistoricalNullifierProof) -> Self {
        Self {
            nullifier: Some(value.nullifier.into()),
            completed_chunks: value.completed_chunks.into_iter().map(Into::into).collect(),
            tail: value.tail.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<pb::HistoricalNullifierProof> for HistoricalNullifierProof {
    type Error = anyhow::Error;

    fn try_from(value: pb::HistoricalNullifierProof) -> Result<Self, Self::Error> {
        Ok(Self {
            nullifier: value
                .nullifier
                .context("historical proof nullifier is missing")?
                .try_into()?,
            completed_chunks: value
                .completed_chunks
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            tail: value
                .tail
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NullifierGenerationState {
    pub protocol_version: u32,
    pub activation_epoch: u64,
    pub current_generation: u64,
    pub current_tree: NullifierTreeId,
    pub current_root: [u8; 32],
    pub current_start_position: u64,
    pub previous_tree: Option<NullifierTreeId>,
    pub previous_root: Option<[u8; 32]>,
    pub recent_position_floor: u64,
    pub archived_generation_count: u64,
    pub archived_history_head: [u8; 32],
}

impl DomainType for NullifierGenerationState {
    type Proto = pb::NullifierGenerationState;
}

impl From<NullifierGenerationState> for pb::NullifierGenerationState {
    fn from(value: NullifierGenerationState) -> Self {
        Self {
            protocol_version: value.protocol_version,
            activation_epoch: value.activation_epoch,
            current_generation: value.current_generation,
            current_tree: Some(value.current_tree.into()),
            current_root: value.current_root.to_vec(),
            current_start_position: value.current_start_position,
            previous_tree: value.previous_tree.map(Into::into),
            previous_root: value
                .previous_root
                .map_or_else(Vec::new, |root| root.to_vec()),
            recent_position_floor: value.recent_position_floor,
            archived_generation_count: value.archived_generation_count,
            archived_history_head: value.archived_history_head.to_vec(),
        }
    }
}

impl TryFrom<pb::NullifierGenerationState> for NullifierGenerationState {
    type Error = anyhow::Error;

    fn try_from(value: pb::NullifierGenerationState) -> Result<Self, Self::Error> {
        let previous_root = if value.previous_root.is_empty() {
            None
        } else {
            Some(decode32(value.previous_root, "previous nullifier root")?)
        };
        let state = Self {
            protocol_version: value.protocol_version,
            activation_epoch: value.activation_epoch,
            current_generation: value.current_generation,
            current_tree: value
                .current_tree
                .context("current nullifier tree is missing")?
                .try_into()?,
            current_root: decode32(value.current_root, "current nullifier root")?,
            current_start_position: value.current_start_position,
            previous_tree: value.previous_tree.map(TryInto::try_into).transpose()?,
            previous_root,
            recent_position_floor: value.recent_position_floor,
            archived_generation_count: value.archived_generation_count,
            archived_history_head: decode32(value.archived_history_head, "archived history head")?,
        };
        state.validate()?;
        Ok(state)
    }
}

impl NullifierGenerationState {
    pub fn at_activation(
        activation_epoch: u64,
        activation_position: u64,
        current_root: [u8; 32],
    ) -> anyhow::Result<Self> {
        ensure!(
            activation_position <= MAX_SCT_POSITION,
            "activation position exceeds the 48-bit SCT position range"
        );
        let state = Self {
            protocol_version: PROTOCOL_VERSION,
            activation_epoch,
            current_generation: 0,
            current_tree: NullifierTreeId::Generation(0),
            current_root,
            current_start_position: activation_position,
            previous_tree: None,
            previous_root: None,
            recent_position_floor: activation_position,
            archived_generation_count: 0,
            archived_history_head: empty_history_head(),
        };
        state.validate()?;
        Ok(state)
    }

    pub fn window(&self) -> NullifierWindow {
        NullifierWindow {
            protocol_version: self.protocol_version,
            current_generation: self.current_generation,
            recent_position_floor: self.recent_position_floor,
            archived_generation_count: self.archived_generation_count,
            archived_history_head: self.archived_history_head,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        self.window().validate()?;
        ensure!(
            self.current_tree == NullifierTreeId::Generation(self.current_generation),
            "current tree id does not match current generation"
        );
        if self.current_generation == 0 {
            ensure!(
                self.previous_tree.is_none() && self.previous_root.is_none(),
                "activation generation cannot have a previous tree"
            );
        } else {
            let expected_previous = NullifierTreeId::Generation(self.current_generation - 1);
            ensure!(
                self.previous_tree == Some(expected_previous),
                "previous tree id does not match current generation"
            );
            ensure!(
                self.previous_root.is_some(),
                "previous nullifier root is missing"
            );
        }
        ensure!(
            self.current_start_position <= MAX_SCT_POSITION,
            "current start position exceeds the 48-bit SCT position range"
        );
        ensure!(
            self.recent_position_floor <= self.current_start_position,
            "recent floor is after the current generation start"
        );
        canonical_fq(self.current_root, "current generation root")?;
        if let Some(previous_root) = self.previous_root {
            canonical_fq(previous_root, "previous generation root")?;
        }
        Ok(())
    }

    pub fn should_rollover(&self, next_epoch: u64) -> anyhow::Result<bool> {
        let elapsed = next_epoch
            .checked_sub(self.activation_epoch)
            .context("next epoch precedes nullifier activation")?;
        Ok(elapsed > 0 && elapsed % GENERATION_EPOCHS == 0)
    }

    pub fn rollover(
        &self,
        next_epoch: u64,
        next_position: u64,
        new_empty_root: [u8; 32],
    ) -> anyhow::Result<NullifierGenerationTransition> {
        ensure!(
            self.should_rollover(next_epoch)?,
            "epoch is not a generation boundary"
        );
        ensure!(
            next_position <= MAX_SCT_POSITION && next_position >= self.current_start_position,
            "invalid next generation SCT position"
        );
        let next_generation = (next_epoch - self.activation_epoch) / GENERATION_EPOCHS;
        ensure!(
            next_generation == self.current_generation + 1,
            "generation boundary skipped or repeated"
        );

        let archived = match (self.previous_tree, self.previous_root) {
            (Some(NullifierTreeId::Generation(generation_index)), Some(generation_root)) => {
                let archived_history_head = append_history(
                    self.archived_history_head,
                    generation_index,
                    generation_root,
                    self.recent_position_floor,
                    self.current_start_position,
                )?;
                let archived_generation_count = self
                    .archived_generation_count
                    .checked_add(1)
                    .context("archived generation count overflow")?;
                Some(NullifierGenerationArchived {
                    generation_index,
                    generation_start_position: self.recent_position_floor,
                    generation_end_position: self.current_start_position,
                    generation_root,
                    archived_generation_count,
                    archived_history_head,
                })
            }
            (None, None) => None,
            _ => anyhow::bail!("previous nullifier tree and root disagree"),
        };
        let archived_generation_count = archived
            .map_or(self.archived_generation_count, |archived| {
                archived.archived_generation_count
            });
        let archived_history_head = archived.map_or(self.archived_history_head, |archived| {
            archived.archived_history_head
        });

        let next = Self {
            protocol_version: self.protocol_version,
            activation_epoch: self.activation_epoch,
            current_generation: next_generation,
            current_tree: NullifierTreeId::Generation(next_generation),
            current_root: new_empty_root,
            current_start_position: next_position,
            previous_tree: Some(self.current_tree),
            previous_root: Some(self.current_root),
            recent_position_floor: self.current_start_position,
            archived_generation_count,
            archived_history_head,
        };
        next.validate()?;

        let application_epoch_start =
            self.activation_epoch + self.current_generation * GENERATION_EPOCHS;
        Ok(NullifierGenerationTransition {
            next,
            frozen: NullifierGenerationFrozen {
                generation_index: self.current_generation,
                application_epoch_start,
                application_epoch_end: next_epoch - 1,
                generation_start_position: self.current_start_position,
                generation_end_position: next_position,
                generation_root: self.current_root,
            },
            archived,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NullifierGenerationTransition {
    pub next: NullifierGenerationState,
    pub frozen: NullifierGenerationFrozen,
    pub archived: Option<NullifierGenerationArchived>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NullifierGenerationFrozen {
    pub generation_index: u64,
    pub application_epoch_start: u64,
    pub application_epoch_end: u64,
    pub generation_start_position: u64,
    pub generation_end_position: u64,
    pub generation_root: [u8; 32],
}

impl NullifierGenerationFrozen {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.application_epoch_start <= self.application_epoch_end,
            "frozen generation epoch range is invalid"
        );
        ensure!(
            self.generation_start_position <= self.generation_end_position
                && self.generation_end_position <= MAX_SCT_POSITION,
            "frozen generation position range is invalid"
        );
        canonical_fq(self.generation_root, "frozen generation root")?;
        Ok(())
    }
}

impl DomainType for NullifierGenerationFrozen {
    type Proto = pb::EventNullifierGenerationFrozen;
}

impl From<NullifierGenerationFrozen> for pb::EventNullifierGenerationFrozen {
    fn from(value: NullifierGenerationFrozen) -> Self {
        Self {
            generation_index: value.generation_index,
            application_epoch_start: value.application_epoch_start,
            application_epoch_end: value.application_epoch_end,
            generation_start_position: value.generation_start_position,
            generation_end_position: value.generation_end_position,
            generation_root: value.generation_root.to_vec(),
        }
    }
}

impl TryFrom<pb::EventNullifierGenerationFrozen> for NullifierGenerationFrozen {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventNullifierGenerationFrozen) -> Result<Self, Self::Error> {
        let frozen = Self {
            generation_index: value.generation_index,
            application_epoch_start: value.application_epoch_start,
            application_epoch_end: value.application_epoch_end,
            generation_start_position: value.generation_start_position,
            generation_end_position: value.generation_end_position,
            generation_root: decode32(value.generation_root, "frozen generation root")?,
        };
        frozen.validate()?;
        Ok(frozen)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NullifierGenerationArchived {
    pub generation_index: u64,
    pub generation_start_position: u64,
    pub generation_end_position: u64,
    pub generation_root: [u8; 32],
    pub archived_generation_count: u64,
    pub archived_history_head: [u8; 32],
}

impl NullifierGenerationArchived {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.generation_start_position <= self.generation_end_position
                && self.generation_end_position <= MAX_SCT_POSITION,
            "archived generation position range is invalid"
        );
        let expected_count = self
            .generation_index
            .checked_add(1)
            .context("archived generation index overflow")?;
        ensure!(
            self.archived_generation_count == expected_count,
            "archived generation count does not match generation index"
        );
        canonical_fq(self.generation_root, "archived generation root")?;
        canonical_fq(
            self.archived_history_head,
            "archived generation history head",
        )?;
        Ok(())
    }
}

/// Node-local evidence that a generation pack was verified before expanded-tree pruning.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NullifierGenerationPackReceipt {
    pub protocol_version: u32,
    pub generation_index: u64,
    pub generation_root: [u8; 32],
    pub generation_start_position: u64,
    pub generation_end_position: u64,
    /// Includes the implicit lower sentinel.
    pub leaf_count: u64,
    pub byte_length: u64,
    pub pack_sha256: [u8; 32],
}

impl NullifierGenerationPackReceipt {
    pub fn validate(&self) -> anyhow::Result<()> {
        ensure!(
            self.protocol_version == PROTOCOL_VERSION,
            "unsupported nullifier generation pack protocol"
        );
        ensure!(
            self.generation_start_position <= self.generation_end_position
                && self.generation_end_position <= MAX_SCT_POSITION,
            "packed generation position range is invalid"
        );
        ensure!(
            (1..=(1u64 << 40)).contains(&self.leaf_count),
            "packed generation leaf count is invalid"
        );
        ensure!(
            self.byte_length
                == 108u64
                    .checked_add(
                        self.leaf_count
                            .checked_sub(1)
                            .context("generation pack omits its sentinel")?
                            .checked_mul(32)
                            .context("generation pack byte length overflow")?,
                    )
                    .context("generation pack byte length overflow")?,
            "generation pack byte length does not match its leaf count"
        );
        Ok(())
    }
}

impl DomainType for NullifierGenerationArchived {
    type Proto = pb::EventNullifierGenerationArchived;
}

impl From<NullifierGenerationArchived> for pb::EventNullifierGenerationArchived {
    fn from(value: NullifierGenerationArchived) -> Self {
        Self {
            generation_index: value.generation_index,
            generation_start_position: value.generation_start_position,
            generation_end_position: value.generation_end_position,
            generation_root: value.generation_root.to_vec(),
            archived_generation_count: value.archived_generation_count,
            archived_history_head: value.archived_history_head.to_vec(),
        }
    }
}

impl TryFrom<pb::EventNullifierGenerationArchived> for NullifierGenerationArchived {
    type Error = anyhow::Error;

    fn try_from(value: pb::EventNullifierGenerationArchived) -> Result<Self, Self::Error> {
        let archived = Self {
            generation_index: value.generation_index,
            generation_start_position: value.generation_start_position,
            generation_end_position: value.generation_end_position,
            generation_root: decode32(value.generation_root, "archived generation root")?,
            archived_generation_count: value.archived_generation_count,
            archived_history_head: decode32(
                value.archived_history_head,
                "archived generation history head",
            )?,
        };
        archived.validate()?;
        Ok(archived)
    }
}

pub fn empty_history_head() -> [u8; 32] {
    poseidon377::hash_1(&EMPTY_HISTORY_DOMAIN, Fq::from(0u64)).to_bytes()
}

pub fn append_history(
    previous: [u8; 32],
    generation: u64,
    root: [u8; 32],
    generation_start_position: u64,
    generation_end_position: u64,
) -> anyhow::Result<[u8; 32]> {
    ensure!(
        generation_start_position <= generation_end_position
            && generation_end_position <= MAX_SCT_POSITION,
        "generation position range is invalid"
    );
    let previous = canonical_fq(previous, "previous history head")?;
    let root = canonical_fq(root, "generation root")?;
    Ok(poseidon377::hash_5(
        &HISTORY_NODE_DOMAIN,
        (
            previous,
            Fq::from(generation),
            root,
            Fq::from(generation_start_position),
            Fq::from(generation_end_position),
        ),
    )
    .to_bytes())
}

pub fn is_old(note_position: u64, recent_position_floor: u64) -> anyhow::Result<bool> {
    ensure!(
        note_position <= MAX_SCT_POSITION && recent_position_floor <= MAX_SCT_POSITION,
        "position exceeds the 48-bit SCT position range"
    );
    Ok(note_position < recent_position_floor)
}

fn decode32(bytes: Vec<u8>, label: &str) -> anyhow::Result<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| anyhow::anyhow!("{label} must be 32 bytes, got {}", bytes.len()))
}

fn canonical_fq(bytes: [u8; 32], label: &str) -> anyhow::Result<Fq> {
    Fq::from_bytes_checked(&bytes).map_err(|_| anyhow::anyhow!("{label} is not a canonical Fq"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::Fq;

    fn root(value: u64) -> [u8; 32] {
        Fq::from(value).to_bytes()
    }

    #[test]
    fn activation_and_rollover_preserve_generation_arithmetic() -> anyhow::Result<()> {
        let state = NullifierGenerationState::at_activation(100, 9 << 32, root(1))?;
        assert!(!state.should_rollover(129)?);
        assert!(state.should_rollover(130)?);

        let transition = state.rollover(130, 39 << 32, root(3))?;
        assert_eq!(transition.next.current_generation, 1);
        assert_eq!(transition.next.current_tree, NullifierTreeId::Generation(1));
        assert_eq!(
            transition.next.previous_tree,
            Some(NullifierTreeId::Generation(0))
        );
        assert_eq!(transition.next.recent_position_floor, 9 << 32);
        assert_eq!(transition.next.archived_generation_count, 0);
        assert!(transition.archived.is_none());
        assert_eq!(transition.frozen.generation_index, 0);
        assert_eq!(transition.frozen.application_epoch_start, 100);
        assert_eq!(transition.frozen.application_epoch_end, 129);
        Ok(())
    }

    #[test]
    fn history_binds_index_root_and_end_position() -> anyhow::Result<()> {
        let empty = empty_history_head();
        assert_ne!(append_history(empty, 0, root(1), 0, 10)?, empty);
        assert_ne!(
            append_history(empty, 0, root(1), 0, 10)?,
            append_history(empty, 1, root(1), 0, 10)?
        );
        assert_ne!(
            append_history(empty, 0, root(1), 0, 10)?,
            append_history(empty, 0, root(2), 0, 10)?
        );
        assert_ne!(
            append_history(empty, 0, root(1), 0, 10)?,
            append_history(empty, 0, root(1), 1, 10)?
        );
        assert_ne!(
            append_history(empty, 0, root(1), 0, 10)?,
            append_history(empty, 0, root(1), 0, 11)?
        );
        Ok(())
    }

    #[test]
    fn bundle_shape_depends_only_on_global_history_count() -> anyhow::Result<()> {
        let mut state = NullifierGenerationState::at_activation(0, 0, root(1))?;
        for generation in 1..=51 {
            let transition = state.rollover(
                generation * GENERATION_EPOCHS,
                generation << 32,
                root(generation + 1),
            )?;
            state = transition.next;
        }
        assert_eq!(
            state.window().bundle_shape(),
            HistoricalBundleShape {
                completed_chunks: 5,
                tail_proofs: 0,
            }
        );
        Ok(())
    }

    #[test]
    fn old_boundary_is_strict() -> anyhow::Result<()> {
        assert!(is_old(41, 42)?);
        assert!(!is_old(42, 42)?);
        assert!(!is_old(43, 42)?);
        Ok(())
    }

    #[test]
    fn protobuf_round_trip_is_exact() -> anyhow::Result<()> {
        let state = NullifierGenerationState::at_activation(7, 11 << 32, root(3))?;
        let decoded: NullifierGenerationState =
            pb::NullifierGenerationState::from(state.clone()).try_into()?;
        assert_eq!(decoded, state);
        Ok(())
    }

    fn historical_bundle(
        count: u64,
    ) -> anyhow::Result<(NullifierWindow, HistoricalNullifierProof)> {
        let mut heads = vec![empty_history_head()];
        for generation in 0..count {
            heads.push(append_history(
                *heads.last().expect("history starts nonempty"),
                generation,
                root(generation + 1),
                generation << 32,
                (generation + 1) << 32,
            )?);
        }
        let completed_chunks = (0..count / CHUNK_WIDTH)
            .map(|chunk_index| HistoricalChunkProof {
                chunk_index,
                end_history_head: heads[((chunk_index + 1) * CHUNK_WIDTH) as usize],
                groth16_proof: vec![7; BW6_761_PROOF_BYTES],
            })
            .collect();
        let tail = (count / CHUNK_WIDTH * CHUNK_WIDTH..count)
            .map(|generation_index| GenerationNonmembershipProof {
                generation_index,
                generation_root: root(generation_index + 1),
                generation_start_position: generation_index << 32,
                generation_end_position: (generation_index + 1) << 32,
                groth16_proof: vec![9; BLS12_377_PROOF_BYTES],
            })
            .collect();
        Ok((
            NullifierWindow {
                protocol_version: PROTOCOL_VERSION,
                current_generation: count + 1,
                recent_position_floor: 0,
                archived_generation_count: count,
                archived_history_head: heads[count as usize],
            },
            HistoricalNullifierProof {
                nullifier: Nullifier(Fq::from(17u64)),
                completed_chunks,
                tail,
            },
        ))
    }

    #[test]
    fn canonical_historical_bundles_cover_the_complete_prefix() -> anyhow::Result<()> {
        for count in [1, 9, 10, 11, 49, 50, 51, 120] {
            let (window, proof) = historical_bundle(count)?;
            proof.validate_structure(window)?;
            let decoded: HistoricalNullifierProof =
                pb::HistoricalNullifierProof::from(proof.clone()).try_into()?;
            assert_eq!(decoded, proof);
        }
        Ok(())
    }

    #[test]
    fn historical_bundle_structure_rejects_omission_reordering_and_bad_encoding(
    ) -> anyhow::Result<()> {
        let (window, proof) = historical_bundle(11)?;

        let mut missing = proof.clone();
        missing.tail.clear();
        assert!(missing.validate_structure(window).is_err());

        let mut reordered = proof.clone();
        reordered.completed_chunks[0].chunk_index = 1;
        assert!(reordered.validate_structure(window).is_err());

        let mut bad_root = proof.clone();
        bad_root.tail[0].generation_root[0] ^= 1;
        assert!(bad_root.validate_structure(window).is_err());

        let mut bad_encoding = proof;
        bad_encoding.tail[0].groth16_proof.pop();
        assert!(bad_encoding.validate_structure(window).is_err());
        Ok(())
    }

    #[test]
    fn public_history_records_reject_noncanonical_field_encodings() -> anyhow::Result<()> {
        let invalid = [u8::MAX; 32];

        let mut window = NullifierGenerationState::at_activation(0, 0, root(1))?.window();
        window.archived_history_head = invalid;
        assert!(window.validate().is_err());

        let generation = GenerationNonmembershipProof {
            generation_index: 0,
            generation_root: invalid,
            generation_start_position: 0,
            generation_end_position: 1,
            groth16_proof: vec![0; BLS12_377_PROOF_BYTES],
        };
        assert!(generation.validate().is_err());

        let chunk = HistoricalChunkProof {
            chunk_index: 0,
            end_history_head: invalid,
            groth16_proof: vec![0; BW6_761_PROOF_BYTES],
        };
        assert!(chunk.validate().is_err());

        assert!(append_history(invalid, 0, root(1), 0, 1).is_err());
        assert!(append_history(empty_history_head(), 0, invalid, 0, 1).is_err());
        Ok(())
    }
}
