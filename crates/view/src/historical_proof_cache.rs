use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use shieldd_sdk_sct::nullifier_generation::{
    append_history, empty_history_head, ArchivedNullifierProof, GenerationNonmembershipProof,
    HistoricalChunkProof, HistoricalNullifierProof, NullifierWindow, CHUNK_WIDTH, PROTOCOL_VERSION,
};
use shieldd_sdk_sct::Nullifier;

use shieldd_sdk_proof_params::historical::{self, ChunkClaim, GenerationClaim};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum HistoricalProofCacheState {
    PendingBackfill,
    Ready,
    Updating,
    BlockedOnWitnessSource,
    BlockedOnProver,
    Invalid,
}

impl HistoricalProofCacheState {
    pub fn can_transition_to(self, next: Self) -> bool {
        use HistoricalProofCacheState::*;
        matches!(
            (self, next),
            (PendingBackfill, Updating)
                | (Updating, Ready)
                | (Updating, BlockedOnWitnessSource)
                | (Updating, BlockedOnProver)
                | (BlockedOnWitnessSource, Updating)
                | (BlockedOnProver, Updating)
                | (Ready, Updating)
                | (
                    PendingBackfill | Ready | Updating | BlockedOnWitnessSource | BlockedOnProver,
                    Invalid
                )
        )
    }

    pub(crate) const fn storage_id(self) -> i64 {
        match self {
            Self::PendingBackfill => 0,
            Self::Ready => 1,
            Self::Updating => 2,
            Self::BlockedOnWitnessSource => 3,
            Self::Invalid => 4,
            Self::BlockedOnProver => 5,
        }
    }

    pub(crate) fn from_storage_id(value: i64) -> anyhow::Result<Self> {
        match value {
            0 => Ok(Self::PendingBackfill),
            1 => Ok(Self::Ready),
            2 => Ok(Self::Updating),
            3 => Ok(Self::BlockedOnWitnessSource),
            4 => Ok(Self::Invalid),
            5 => Ok(Self::BlockedOnProver),
            _ => anyhow::bail!("invalid historical proof cache state {value}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoricalProofCache {
    pub protocol_version: u32,
    pub nullifier: Nullifier,
    pub covered_generation_count: u64,
    pub terminal_history_head: [u8; 32],
    pub proof: HistoricalNullifierProof,
    pub state: HistoricalProofCacheState,
    pub last_error: Option<String>,
}

impl HistoricalProofCache {
    pub fn pending(nullifier: Nullifier) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            nullifier,
            covered_generation_count: 0,
            terminal_history_head: shieldd_sdk_sct::nullifier_generation::empty_history_head(),
            proof: HistoricalNullifierProof {
                nullifier,
                completed_chunks: Vec::new(),
                tail: Vec::new(),
            },
            state: HistoricalProofCacheState::PendingBackfill,
            last_error: None,
        }
    }

    /// Reset transient work after process restart while preserving completed proof coverage.
    pub fn recover_after_restart(&mut self) -> anyhow::Result<()> {
        if self.state == HistoricalProofCacheState::Updating {
            self.state = HistoricalProofCacheState::PendingBackfill;
        }
        self.validate()
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.protocol_version == PROTOCOL_VERSION,
            "unsupported historical proof cache version"
        );
        anyhow::ensure!(
            self.proof.nullifier == self.nullifier,
            "historical proof cache nullifier mismatch"
        );
        anyhow::ensure!(
            self.proof.completed_chunks.len() as u64 == self.covered_generation_count / CHUNK_WIDTH,
            "historical proof cache chunk count mismatch"
        );
        anyhow::ensure!(
            self.proof.tail.len() as u64 == self.covered_generation_count % CHUNK_WIDTH,
            "historical proof cache tail count mismatch"
        );
        anyhow::ensure!(
            self.last_error.as_ref().map_or(0, String::len) <= 1_024,
            "historical proof cache error exceeds 1024 bytes"
        );
        if self.covered_generation_count == 0 {
            anyhow::ensure!(
                self.terminal_history_head
                    == shieldd_sdk_sct::nullifier_generation::empty_history_head(),
                "empty historical proof cache has a nonempty head"
            );
        } else {
            let window = NullifierWindow {
                protocol_version: self.protocol_version,
                current_generation: self.covered_generation_count + 1,
                recent_position_floor: 0,
                archived_generation_count: self.covered_generation_count,
                archived_history_head: self.terminal_history_head,
            };
            self.proof.validate_structure(window)?;
        }
        Ok(())
    }

    pub fn transition(&mut self, next: HistoricalProofCacheState) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.state.can_transition_to(next),
            "illegal historical proof cache transition {:?} -> {:?}",
            self.state,
            next
        );
        self.state = next;
        if !matches!(
            next,
            HistoricalProofCacheState::BlockedOnWitnessSource
                | HistoricalProofCacheState::BlockedOnProver
        ) {
            self.last_error = None;
        }
        Ok(())
    }

    pub fn block_on_witness_source(&mut self, error: impl Into<String>) -> anyhow::Result<()> {
        self.transition(HistoricalProofCacheState::BlockedOnWitnessSource)?;
        let mut error = error.into();
        error.truncate(1_024);
        self.last_error = Some(error);
        Ok(())
    }

    pub fn block_on_prover(&mut self, error: impl Into<String>) -> anyhow::Result<()> {
        self.transition(HistoricalProofCacheState::BlockedOnProver)?;
        let mut error = error.into();
        error.truncate(1_024);
        self.last_error = Some(error);
        Ok(())
    }

    pub fn append_generation(
        &mut self,
        generation: GenerationNonmembershipProof,
        closed_chunk: Option<HistoricalChunkProof>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.state == HistoricalProofCacheState::Updating,
            "historical proof cache must be updating"
        );
        generation.validate()?;
        anyhow::ensure!(
            generation.generation_index == self.covered_generation_count,
            "historical generation proof is not the next missing generation"
        );
        let next_head = append_history(
            self.terminal_history_head,
            generation.generation_index,
            generation.generation_root,
            generation.generation_start_position,
            generation.generation_end_position,
        )?;
        self.proof.tail.push(generation);
        self.covered_generation_count += 1;
        self.terminal_history_head = next_head;

        if self.covered_generation_count % CHUNK_WIDTH == 0 {
            let chunk = closed_chunk
                .ok_or_else(|| anyhow::anyhow!("completed history chunk proof is missing"))?;
            chunk.validate()?;
            anyhow::ensure!(
                chunk.chunk_index + 1 == self.covered_generation_count / CHUNK_WIDTH,
                "completed history chunk has the wrong index"
            );
            anyhow::ensure!(
                chunk.end_history_head == next_head,
                "completed history chunk has the wrong terminal head"
            );
            self.proof.completed_chunks.push(chunk);
            self.proof.tail.clear();
        } else {
            anyhow::ensure!(
                closed_chunk.is_none(),
                "partial historical tail cannot carry a chunk proof"
            );
        }
        self.validate()
    }

    pub fn mark_ready(&mut self, window: NullifierWindow) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.covered_generation_count == window.archived_generation_count
                && self.terminal_history_head == window.archived_history_head,
            "historical proof cache does not cover the current window"
        );
        if window.archived_generation_count > 0 {
            self.proof.validate_structure(window)?;
        } else {
            anyhow::ensure!(
                self.proof.completed_chunks.is_empty() && self.proof.tail.is_empty(),
                "empty retired prefix has historical proofs"
            );
        }
        self.transition(HistoricalProofCacheState::Ready)?;
        self.validate()
    }

    pub fn bundle_for(&self, window: NullifierWindow) -> anyhow::Result<HistoricalNullifierProof> {
        anyhow::ensure!(
            self.state == HistoricalProofCacheState::Ready,
            "historical proof cache is not ready"
        );
        anyhow::ensure!(
            self.covered_generation_count == window.archived_generation_count
                && self.terminal_history_head == window.archived_history_head,
            "historical proof cache is stale"
        );
        self.proof.validate_structure(window)?;
        Ok(self.proof.clone())
    }
}

#[async_trait]
pub trait HistoricalWitnessSource: Send + Sync {
    async fn nonmembership_proof(
        &self,
        nullifier: Nullifier,
        generation_index: u64,
    ) -> anyhow::Result<ArchivedNullifierProof>;
}

#[async_trait]
pub trait HistoricalProofProvider: Send + Sync {
    async fn prove_generation(
        &self,
        nullifier: Nullifier,
        archived: ArchivedNullifierProof,
        start_history_head: [u8; 32],
        end_history_head: [u8; 32],
    ) -> anyhow::Result<GenerationNonmembershipProof>;

    async fn prove_chunk(
        &self,
        nullifier: Nullifier,
        chunk_index: u64,
        start_history_head: [u8; 32],
        end_history_head: [u8; 32],
        generation_proofs: Vec<GenerationNonmembershipProof>,
    ) -> anyhow::Result<HistoricalChunkProof>;
}

#[derive(Debug)]
pub enum HistoricalProofUpdateError {
    Invalid(anyhow::Error),
    WitnessSource(anyhow::Error),
    Prover(anyhow::Error),
}

impl std::fmt::Display for HistoricalProofUpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(error) => write!(formatter, "invalid historical proof state: {error:#}"),
            Self::WitnessSource(error) => {
                write!(formatter, "historical witness source failure: {error:#}")
            }
            Self::Prover(error) => write!(formatter, "historical prover failure: {error:#}"),
        }
    }
}

impl std::error::Error for HistoricalProofUpdateError {}

/// Advance one note by at most one retired generation.
///
/// The caller persists the returned cache before requesting another step, so a
/// crash loses at most the proof currently being computed.
pub async fn advance_historical_proof_cache(
    mut cache: HistoricalProofCache,
    window: NullifierWindow,
    witness_source: &dyn HistoricalWitnessSource,
    prover: &dyn HistoricalProofProvider,
) -> Result<HistoricalProofCache, HistoricalProofUpdateError> {
    window
        .validate()
        .map_err(HistoricalProofUpdateError::Invalid)?;
    cache
        .validate()
        .map_err(HistoricalProofUpdateError::Invalid)?;
    if cache.protocol_version != window.protocol_version {
        return Err(HistoricalProofUpdateError::Invalid(anyhow::anyhow!(
            "cache and chain protocol versions differ"
        )));
    }
    if cache.covered_generation_count > window.archived_generation_count {
        return Err(HistoricalProofUpdateError::Invalid(anyhow::anyhow!(
            "cache coverage is ahead of the chain window"
        )));
    }
    if cache.covered_generation_count == window.archived_generation_count {
        if cache.state == HistoricalProofCacheState::Ready {
            if window.archived_generation_count > 0 {
                cache
                    .bundle_for(window)
                    .map_err(HistoricalProofUpdateError::Invalid)?;
            } else {
                cache
                    .validate()
                    .map_err(HistoricalProofUpdateError::Invalid)?;
            }
            return Ok(cache);
        }
        if cache.state != HistoricalProofCacheState::Updating {
            cache
                .transition(HistoricalProofCacheState::Updating)
                .map_err(HistoricalProofUpdateError::Invalid)?;
        }
        cache
            .mark_ready(window)
            .map_err(HistoricalProofUpdateError::Invalid)?;
        return Ok(cache);
    }
    if cache.state != HistoricalProofCacheState::Updating {
        cache
            .transition(HistoricalProofCacheState::Updating)
            .map_err(HistoricalProofUpdateError::Invalid)?;
    }

    let generation_index = cache.covered_generation_count;
    let archived = witness_source
        .nonmembership_proof(cache.nullifier, generation_index)
        .await
        .map_err(HistoricalProofUpdateError::WitnessSource)?;
    if archived.generation_index != generation_index {
        return Err(HistoricalProofUpdateError::WitnessSource(anyhow::anyhow!(
            "witness source returned generation {}, expected {generation_index}",
            archived.generation_index
        )));
    }
    archived
        .verify_for(cache.nullifier)
        .map_err(HistoricalProofUpdateError::WitnessSource)?;

    let next_head = append_history(
        cache.terminal_history_head,
        generation_index,
        archived.generation_root,
        archived.generation_start_position,
        archived.generation_end_position,
    )
    .map_err(HistoricalProofUpdateError::WitnessSource)?;
    let generation = prover
        .prove_generation(
            cache.nullifier,
            archived.clone(),
            cache.terminal_history_head,
            next_head,
        )
        .await
        .map_err(HistoricalProofUpdateError::Prover)?;
    if generation.generation_index != generation_index
        || generation.generation_root != archived.generation_root
        || generation.generation_start_position != archived.generation_start_position
        || generation.generation_end_position != archived.generation_end_position
    {
        return Err(HistoricalProofUpdateError::Prover(anyhow::anyhow!(
            "historical prover returned a proof for the wrong generation claim"
        )));
    }
    let nullifier: [u8; 32] = cache.nullifier.into();
    historical::verify_generation(
        historical::generation_verification_key(),
        GenerationClaim {
            protocol_version: window.protocol_version,
            nullifier,
            generation_index,
            generation_root: generation.generation_root,
            generation_start_position: generation.generation_start_position,
            generation_end_position: generation.generation_end_position,
            start_history_head: cache.terminal_history_head,
            end_history_head: next_head,
        },
        &generation.groth16_proof,
    )
    .map_err(HistoricalProofUpdateError::Prover)?;

    let closes_chunk = (generation_index + 1) % CHUNK_WIDTH == 0;
    let chunk = if closes_chunk {
        let chunk_index = generation_index / CHUNK_WIDTH;
        let start_history_head = cache
            .proof
            .completed_chunks
            .last()
            .map_or_else(empty_history_head, |chunk| chunk.end_history_head);
        let mut generation_proofs = cache.proof.tail.clone();
        generation_proofs.push(generation.clone());
        if generation_proofs.len() as u64 != CHUNK_WIDTH {
            return Err(HistoricalProofUpdateError::Invalid(anyhow::anyhow!(
                "completed chunk does not contain {CHUNK_WIDTH} generation proofs"
            )));
        }
        let chunk = prover
            .prove_chunk(
                cache.nullifier,
                chunk_index,
                start_history_head,
                next_head,
                generation_proofs,
            )
            .await
            .map_err(HistoricalProofUpdateError::Prover)?;
        if chunk.chunk_index != chunk_index || chunk.end_history_head != next_head {
            return Err(HistoricalProofUpdateError::Prover(anyhow::anyhow!(
                "historical prover returned a proof for the wrong chunk claim"
            )));
        }
        historical::verify_chunk(
            historical::chunk_verification_key(),
            ChunkClaim {
                protocol_version: window.protocol_version,
                nullifier,
                chunk_index,
                start_history_head,
                end_history_head: next_head,
            },
            &chunk.groth16_proof,
        )
        .map_err(HistoricalProofUpdateError::Prover)?;
        Some(chunk)
    } else {
        None
    };

    cache
        .append_generation(generation, chunk)
        .map_err(HistoricalProofUpdateError::Invalid)?;
    if cache.covered_generation_count == window.archived_generation_count {
        cache
            .mark_ready(window)
            .map_err(HistoricalProofUpdateError::Invalid)?;
    }
    Ok(cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::Fq;
    use shieldd_sdk_sct::indexed_nullifier_tree::{
        IndexedNullifierLeaf, IndexedNullifierWitness, DEPTH, ZERO_HASHES,
    };
    use shieldd_sdk_sct::nullifier_generation::{BLS12_377_PROOF_BYTES, BW6_761_PROOF_BYTES};

    fn archived_fixture() -> (Nullifier, ArchivedNullifierProof) {
        let nullifier = Nullifier(Fq::from(9u64));
        let lower = IndexedNullifierLeaf {
            value: Fq::from(0u64).to_bytes(),
            next_index: 1,
            next_value: Fq::from(7u64).to_bytes(),
            is_lower_sentinel: true,
            is_terminal: false,
        };
        let witness = IndexedNullifierWitness {
            leaf_position: 1,
            leaf: IndexedNullifierLeaf::ordinary(
                Nullifier(Fq::from(7u64)),
                0,
                Fq::from(0u64).to_bytes(),
                true,
            ),
            auth_path: (0..DEPTH)
                .map(|level| {
                    let sibling = ZERO_HASHES[level as usize].to_bytes();
                    if level == 0 {
                        [
                            lower
                                .commitment()
                                .expect("lower leaf commitment")
                                .to_bytes(),
                            sibling,
                            sibling,
                        ]
                    } else {
                        [sibling, sibling, sibling]
                    }
                })
                .collect(),
        };
        let generation_root = witness.root().expect("sentinel witness root");
        (
            nullifier,
            ArchivedNullifierProof {
                generation_index: 0,
                generation_root,
                generation_start_position: 0,
                generation_end_position: 1 << 32,
                witness,
            },
        )
    }

    struct FixtureArchive {
        nullifier: Nullifier,
        proof: ArchivedNullifierProof,
    }

    #[async_trait]
    impl HistoricalWitnessSource for FixtureArchive {
        async fn nonmembership_proof(
            &self,
            nullifier: Nullifier,
            generation_index: u64,
        ) -> anyhow::Result<ArchivedNullifierProof> {
            anyhow::ensure!(nullifier == self.nullifier);
            anyhow::ensure!(generation_index == self.proof.generation_index);
            Ok(self.proof.clone())
        }
    }

    struct FixtureProver;

    #[async_trait]
    impl HistoricalProofProvider for FixtureProver {
        async fn prove_generation(
            &self,
            _nullifier: Nullifier,
            archived: ArchivedNullifierProof,
            _start_history_head: [u8; 32],
            _end_history_head: [u8; 32],
        ) -> anyhow::Result<GenerationNonmembershipProof> {
            let proof_json = include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tools/gnark/artifacts/historical_generation_indexed/sample_generation_proof.json"
            ));
            Ok(GenerationNonmembershipProof {
                generation_index: archived.generation_index,
                generation_root: archived.generation_root,
                generation_start_position: archived.generation_start_position,
                generation_end_position: archived.generation_end_position,
                groth16_proof: historical::encode_generation_proof_json(proof_json)?,
            })
        }

        async fn prove_chunk(
            &self,
            _nullifier: Nullifier,
            _chunk_index: u64,
            _start_history_head: [u8; 32],
            _end_history_head: [u8; 32],
            _generation_proofs: Vec<GenerationNonmembershipProof>,
        ) -> anyhow::Result<HistoricalChunkProof> {
            anyhow::bail!("one-generation fixture must not close a chunk")
        }
    }

    #[test]
    fn cache_state_machine_and_chunk_closure_are_explicit() -> anyhow::Result<()> {
        let nullifier = Nullifier(decaf377::Fq::from(7u64));
        let mut cache = HistoricalProofCache::pending(nullifier);
        cache.transition(HistoricalProofCacheState::Updating)?;
        for generation_index in 0..CHUNK_WIDTH {
            let generation = GenerationNonmembershipProof {
                generation_index,
                generation_root: Fq::from(generation_index + 1).to_bytes(),
                generation_start_position: generation_index << 32,
                generation_end_position: (generation_index + 1) << 32,
                groth16_proof: vec![1; BLS12_377_PROOF_BYTES],
            };
            let end_head = append_history(
                cache.terminal_history_head,
                generation_index,
                generation.generation_root,
                generation.generation_start_position,
                generation.generation_end_position,
            )?;
            let chunk = (generation_index + 1 == CHUNK_WIDTH).then(|| HistoricalChunkProof {
                chunk_index: 0,
                end_history_head: end_head,
                groth16_proof: vec![2; BW6_761_PROOF_BYTES],
            });
            cache.append_generation(generation, chunk)?;
        }
        assert_eq!(cache.proof.completed_chunks.len(), 1);
        assert!(cache.proof.tail.is_empty());
        cache.mark_ready(NullifierWindow {
            protocol_version: PROTOCOL_VERSION,
            current_generation: CHUNK_WIDTH + 1,
            recent_position_floor: 0,
            archived_generation_count: CHUNK_WIDTH,
            archived_history_head: cache.terminal_history_head,
        })?;
        assert_eq!(cache.state, HistoricalProofCacheState::Ready);
        Ok(())
    }

    #[tokio::test]
    async fn updater_verifies_archive_and_groth16_proof_before_ready() -> anyhow::Result<()> {
        let (nullifier, archived) = archived_fixture();
        archived.verify_for(nullifier)?;
        let end_head = append_history(
            empty_history_head(),
            archived.generation_index,
            archived.generation_root,
            archived.generation_start_position,
            archived.generation_end_position,
        )?;
        let window = NullifierWindow {
            protocol_version: PROTOCOL_VERSION,
            current_generation: 2,
            recent_position_floor: 0,
            archived_generation_count: 1,
            archived_history_head: end_head,
        };
        let cache = advance_historical_proof_cache(
            HistoricalProofCache::pending(nullifier),
            window,
            &FixtureArchive {
                nullifier,
                proof: archived,
            },
            &FixtureProver,
        )
        .await?;

        assert_eq!(cache.state, HistoricalProofCacheState::Ready);
        assert_eq!(cache.covered_generation_count, 1);
        assert_eq!(cache.proof.tail.len(), 1);
        cache.bundle_for(window)?;
        Ok(())
    }
}
