use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use crate::stateless_cache::TxArtifact;
use anyhow::{Context, Result};
use ark_groth16::Proof;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use decaf377::Bls12_377;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use shieldd_sdk_proof_aggregation::ProofFamilyId;
use shieldd_sdk_proof_params::batch::BatchItem;
use shieldd_sdk_proto::DomainType;
use shieldd_sdk_sct::Nullifier;
use shieldd_sdk_tct::StateCommitment;
use shieldd_sdk_transaction::Transaction;

#[derive(Clone)]
pub struct ProposalArtifactSidecar {
    pub chunk_tx_count: usize,
    pub segment_tx_counts: Vec<usize>,
    pub encoded_bytes: usize,
    pub commitment: [u8; 32],
    entries: BTreeMap<[u8; 32], Arc<Vec<u8>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalArtifactSidecarRecordEntry {
    pub tx_hash: [u8; 32],
    pub encoded_entry: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProposalArtifactSidecarRecord {
    pub chunk_tx_count: usize,
    pub segment_tx_counts: Vec<usize>,
    pub encoded_bytes: usize,
    pub commitment: [u8; 32],
    pub entries: Vec<ProposalArtifactSidecarRecordEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EncodedBatchItem {
    proof: Vec<u8>,
    public_inputs: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct EncodedArtifactSidecarEntry {
    tx_hash: [u8; 32],
    proof_items: BTreeMap<ProofFamilyId, Vec<EncodedBatchItem>>,
    spend_nullifiers: Vec<[u8; 32]>,
    anchor_pairs: Vec<([u8; 32], [u8; 32])>,
    total_proof_count: usize,
}

impl ProposalArtifactSidecar {
    pub fn build(
        artifacts: &[Arc<TxArtifact>],
        chunk_tx_count: usize,
        segment_tx_counts: Vec<usize>,
    ) -> Result<Self> {
        anyhow::ensure!(
            segment_tx_counts.iter().sum::<usize>() == artifacts.len(),
            "sidecar segment counts must cover every artifact exactly once"
        );

        let mut entries = BTreeMap::new();
        let mut encoded_bytes = 0usize;
        let mut hasher = sha2::Sha256::new();
        hasher.update((chunk_tx_count as u64).to_le_bytes());
        for segment_tx_count in &segment_tx_counts {
            hasher.update((*segment_tx_count as u64).to_le_bytes());
        }

        for artifact in artifacts {
            let tx_hash: [u8; 32] =
                sha2::Sha256::digest(artifact.tx.encode_to_vec().as_slice()).into();
            let encoded_entry = encode_sidecar_entry(tx_hash, artifact)?;
            encoded_bytes += encoded_entry.len();
            hasher.update(tx_hash);
            hasher.update((encoded_entry.len() as u64).to_le_bytes());
            hasher.update(encoded_entry.as_slice());
            entries.insert(tx_hash, Arc::new(encoded_entry));
        }

        Ok(Self {
            chunk_tx_count,
            segment_tx_counts,
            encoded_bytes,
            commitment: hasher.finalize().into(),
            entries,
        })
    }

    pub fn entry_bytes(&self, tx_hash: &[u8; 32]) -> Option<Arc<Vec<u8>>> {
        self.entries.get(tx_hash).cloned()
    }

    pub fn to_record(&self) -> ProposalArtifactSidecarRecord {
        ProposalArtifactSidecarRecord {
            chunk_tx_count: self.chunk_tx_count,
            segment_tx_counts: self.segment_tx_counts.clone(),
            encoded_bytes: self.encoded_bytes,
            commitment: self.commitment,
            entries: self
                .entries
                .iter()
                .map(
                    |(tx_hash, encoded_entry)| ProposalArtifactSidecarRecordEntry {
                        tx_hash: *tx_hash,
                        encoded_entry: (**encoded_entry).clone(),
                    },
                )
                .collect(),
        }
    }

    pub fn from_record(record: ProposalArtifactSidecarRecord) -> Self {
        Self {
            chunk_tx_count: record.chunk_tx_count,
            segment_tx_counts: record.segment_tx_counts,
            encoded_bytes: record.encoded_bytes,
            commitment: record.commitment,
            entries: record
                .entries
                .into_iter()
                .map(|entry| (entry.tx_hash, Arc::new(entry.encoded_entry)))
                .collect(),
        }
    }

    pub fn decode_artifact(
        &self,
        tx_hash: [u8; 32],
        tx: Arc<Transaction>,
        encoded_entry: &[u8],
    ) -> Result<Arc<TxArtifact>> {
        let decoded: EncodedArtifactSidecarEntry =
            bincode::deserialize(encoded_entry).context("decoding sidecar entry")?;
        anyhow::ensure!(
            decoded.tx_hash == tx_hash,
            "sidecar entry tx hash mismatch: expected {}, got {}",
            hex::encode(tx_hash),
            hex::encode(decoded.tx_hash)
        );

        let proof_items = decoded
            .proof_items
            .into_iter()
            .map(|(family_id, items)| {
                let decoded_items = items
                    .into_iter()
                    .map(decode_batch_item)
                    .collect::<Result<Vec<_>>>()?;
                Ok((family_id, decoded_items))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;

        let spend_nullifiers = decoded
            .spend_nullifiers
            .into_iter()
            .map(|bytes| Nullifier::try_from(bytes.as_slice()))
            .collect::<Result<Vec<_>, _>>()
            .context("decoding sidecar nullifiers")?;
        let anchor_pairs = decoded
            .anchor_pairs
            .into_iter()
            .map(|(compliance, asset)| {
                Ok((
                    StateCommitment::try_from(compliance)
                        .context("decoding sidecar compliance anchor")?,
                    StateCommitment::try_from(asset).context("decoding sidecar asset anchor")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Arc::new(TxArtifact {
            tx,
            proof_items,
            spend_nullifiers,
            anchor_pairs,
            total_proof_count: decoded.total_proof_count,
            historical_validation: None,
        }))
    }
}

fn encode_sidecar_entry(tx_hash: [u8; 32], artifact: &TxArtifact) -> Result<Vec<u8>> {
    let proof_items = artifact
        .proof_items
        .iter()
        .map(|(family_id, items)| {
            let encoded_items = items
                .iter()
                .map(encode_batch_item)
                .collect::<Result<Vec<_>>>()?;
            Ok((*family_id, encoded_items))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let spend_nullifiers = artifact
        .spend_nullifiers
        .iter()
        .copied()
        .map(Into::into)
        .collect::<Vec<[u8; 32]>>();
    let anchor_pairs = artifact
        .anchor_pairs
        .iter()
        .copied()
        .map(|(compliance, asset)| (compliance.into(), asset.into()))
        .collect::<Vec<_>>();

    bincode::serialize(&EncodedArtifactSidecarEntry {
        tx_hash,
        proof_items,
        spend_nullifiers,
        anchor_pairs,
        total_proof_count: artifact.total_proof_count,
    })
    .context("encoding sidecar entry")
}

fn encode_batch_item(item: &BatchItem) -> Result<EncodedBatchItem> {
    let mut proof = Vec::new();
    item.proof
        .serialize_compressed(&mut proof)
        .context("serializing sidecar proof")?;
    let public_inputs = item
        .public_inputs
        .iter()
        .map(|input| {
            let mut encoded = Vec::new();
            input
                .serialize_compressed(&mut encoded)
                .context("serializing sidecar public input")?;
            Ok(encoded)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(EncodedBatchItem {
        proof,
        public_inputs,
    })
}

fn decode_batch_item(encoded: EncodedBatchItem) -> Result<BatchItem> {
    let proof = Proof::<Bls12_377>::deserialize_compressed(&mut Cursor::new(encoded.proof))
        .context("decoding sidecar proof")?;
    let public_inputs = encoded
        .public_inputs
        .into_iter()
        .map(|bytes| {
            <decaf377::Fq as CanonicalDeserialize>::deserialize_compressed(&mut Cursor::new(bytes))
                .context("decoding sidecar public input")
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(BatchItem {
        proof,
        public_inputs,
    })
}

#[cfg(any(test, feature = "fuzzing"))]
pub fn decode_batch_item_for_fuzz(proof: Vec<u8>, public_inputs: Vec<Vec<u8>>) -> Result<()> {
    decode_batch_item(EncodedBatchItem {
        proof,
        public_inputs,
    })
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(32))]

        #[test]
        fn decode_batch_item_do_not_panic(
            proof in prop::collection::vec(any::<u8>(), 0usize..=4096),
            public_inputs in prop::collection::vec(
                prop::collection::vec(any::<u8>(), 0usize..=128),
                0usize..=4,
            ),
        ) {
            let _ = decode_batch_item_for_fuzz(proof, public_inputs);
        }

        #[test]
        fn decode_artifact_do_not_panic(
            tx_hash in any::<[u8; 32]>(),
            entry_bytes in prop::collection::vec(any::<u8>(), 0usize..=4096),
        ) {
            let sidecar = ProposalArtifactSidecar {
                chunk_tx_count: 0,
                segment_tx_counts: Vec::new(),
                encoded_bytes: 0,
                commitment: [0; 32],
                entries: BTreeMap::new(),
            };
            let tx = Arc::new(Transaction::default());
            let _ = sidecar.decode_artifact(tx_hash, tx.clone(), &entry_bytes);

            let structured = EncodedArtifactSidecarEntry {
                tx_hash,
                proof_items: BTreeMap::new(),
                spend_nullifiers: Vec::new(),
                anchor_pairs: Vec::new(),
                total_proof_count: 0,
            };
            if let Ok(encoded) = bincode::serialize(&structured) {
                let _ = sidecar.decode_artifact(tx_hash, tx, &encoded);
            }
        }
    }
}
