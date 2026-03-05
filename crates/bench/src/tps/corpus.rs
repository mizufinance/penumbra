use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use penumbra_sdk_proto::util::tendermint_proxy::v1::{
    tendermint_proxy_service_client::TendermintProxyServiceClient, GetStatusRequest,
};
use penumbra_sdk_proto::DomainType;
use penumbra_sdk_transaction::Transaction;
use serde::{Deserialize, Serialize};
use sha2::Digest;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub chain_id: String,
    pub genesis_hash: String,
    pub scenario: String,
    pub tx_count: usize,
    pub created_at: u64,
    pub source_label: String,
    pub notes: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexRow {
    pub ordinal: usize,
    pub tx_hash_hex: String,
    pub offset: u64,
    pub length: u64,
    pub asset_kind: String,
}

#[derive(Clone, Debug)]
pub struct CorpusEntry {
    pub ordinal: usize,
    pub tx_hash_hex: String,
    pub offset: u64,
    pub length: u64,
    pub asset_kind: String,
    pub tx_bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Corpus {
    pub manifest: Manifest,
    pub entries: Vec<CorpusEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CorpusVerifyReport {
    pub observer_endpoint: String,
    pub chain_id: Option<String>,
    pub tx_count: usize,
    pub unique_hashes: usize,
}

pub fn tx_hash_hex(tx_bytes: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(tx_bytes))
}

pub fn load_corpus(corpus_dir: &Path) -> Result<Corpus> {
    let manifest_path = corpus_dir.join("manifest.json");
    let index_path = corpus_dir.join("index.csv");
    let txs_path = corpus_dir.join("txs.bin");

    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .with_context(|| format!("failed to read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;

    let txs_file = std::fs::read(&txs_path)
        .with_context(|| format!("failed to read {}", txs_path.display()))?;
    let scanned = scan_txs_bin(&txs_file).context("invalid txs.bin length-prefixed encoding")?;

    let mut rdr = csv::Reader::from_path(&index_path)
        .with_context(|| format!("failed to read {}", index_path.display()))?;
    let mut rows: Vec<IndexRow> = Vec::new();
    for row in rdr.deserialize::<IndexRow>() {
        rows.push(row.context("failed to parse index.csv row")?);
    }
    rows.sort_by_key(|r| r.ordinal);

    anyhow::ensure!(
        !rows.is_empty(),
        "corpus index is empty: {}",
        index_path.display()
    );
    anyhow::ensure!(
        rows.len() == scanned.len(),
        "index entries ({}) do not match txs.bin entries ({})",
        rows.len(),
        scanned.len()
    );

    let mut seen_hashes = HashSet::new();
    let mut entries = Vec::with_capacity(rows.len());

    for row in rows {
        anyhow::ensure!(
            row.ordinal < scanned.len(),
            "ordinal {} out of range",
            row.ordinal
        );
        let (expected_offset, expected_len) = scanned[row.ordinal];
        anyhow::ensure!(
            row.offset == expected_offset && row.length == expected_len,
            "index row {} offset/length mismatch; expected ({expected_offset},{expected_len}), got ({},{})",
            row.ordinal,
            row.offset,
            row.length
        );

        let start = row.offset as usize;
        let end = start + row.length as usize;
        let tx_bytes = txs_file[start..end].to_vec();
        let actual_hash = tx_hash_hex(&tx_bytes);
        anyhow::ensure!(
            actual_hash == row.tx_hash_hex,
            "hash mismatch for ordinal {}: index={}, actual={}",
            row.ordinal,
            row.tx_hash_hex,
            actual_hash
        );
        anyhow::ensure!(
            seen_hashes.insert(row.tx_hash_hex.clone()),
            "duplicate tx_hash_hex in index: {}",
            row.tx_hash_hex
        );

        entries.push(CorpusEntry {
            ordinal: row.ordinal,
            tx_hash_hex: row.tx_hash_hex,
            offset: row.offset,
            length: row.length,
            asset_kind: row.asset_kind,
            tx_bytes,
        });
    }

    anyhow::ensure!(
        manifest.tx_count == entries.len(),
        "manifest tx_count={} does not match corpus entries={}",
        manifest.tx_count,
        entries.len()
    );

    Ok(Corpus { manifest, entries })
}

pub async fn verify_corpus(
    corpus_dir: &Path,
    observer_endpoint: &str,
) -> Result<CorpusVerifyReport> {
    let corpus = load_corpus(corpus_dir)?;

    let mut client = TendermintProxyServiceClient::connect(observer_endpoint.to_string())
        .await
        .with_context(|| format!("failed to connect to observer endpoint {observer_endpoint}"))?;
    let status = client
        .get_status(GetStatusRequest {})
        .await
        .context("GetStatus failed")?
        .into_inner();
    let chain_id = status.node_info.map(|n| n.network);

    if let Some(node_chain_id) = &chain_id {
        anyhow::ensure!(
            corpus.manifest.chain_id == "unknown" || corpus.manifest.chain_id == *node_chain_id,
            "manifest chain_id={} does not match observer chain_id={}",
            corpus.manifest.chain_id,
            node_chain_id
        );
    }

    Ok(CorpusVerifyReport {
        observer_endpoint: observer_endpoint.to_string(),
        chain_id,
        tx_count: corpus.entries.len(),
        unique_hashes: corpus.entries.len(),
    })
}

pub fn build_corpus_from_transactions(
    out_dir: &Path,
    scenario: &str,
    source_label: &str,
    chain_id: &str,
    genesis_hash: &str,
    notes: &str,
    asset_kind: &str,
    txs: &[Transaction],
) -> Result<()> {
    anyhow::ensure!(!txs.is_empty(), "transaction list is empty");

    let mut seen = HashSet::new();
    let mut entries = Vec::with_capacity(txs.len());
    for tx in txs {
        let tx_bytes = tx.encode_to_vec();
        let tx_hash = tx_hash_hex(&tx_bytes);
        anyhow::ensure!(
            seen.insert(tx_hash.clone()),
            "duplicate tx hash generated: {tx_hash}"
        );
        entries.push(CorpusEntry {
            ordinal: entries.len(),
            tx_hash_hex: tx_hash,
            offset: 0,
            length: tx_bytes.len() as u64,
            asset_kind: asset_kind.to_string(),
            tx_bytes,
        });
    }

    let manifest = Manifest {
        chain_id: chain_id.to_string(),
        genesis_hash: genesis_hash.to_string(),
        scenario: scenario.to_string(),
        tx_count: txs.len(),
        created_at: unix_ts(),
        source_label: source_label.to_string(),
        notes: notes.to_string(),
    };

    write_corpus_from_entries(out_dir, &manifest, &entries)?;
    Ok(())
}

pub fn append_corpus_from_transactions(
    corpus_dir: &Path,
    asset_kind: &str,
    source_label: &str,
    notes: &str,
    txs: &[Transaction],
) -> Result<usize> {
    anyhow::ensure!(!txs.is_empty(), "transaction list is empty");
    let mut corpus = load_corpus(corpus_dir)?;

    let mut seen: HashSet<String> = corpus
        .entries
        .iter()
        .map(|entry| entry.tx_hash_hex.clone())
        .collect();

    let mut added = 0usize;
    for tx in txs {
        let tx_bytes = tx.encode_to_vec();
        let tx_hash = tx_hash_hex(&tx_bytes);
        anyhow::ensure!(
            seen.insert(tx_hash.clone()),
            "duplicate tx hash while appending: {tx_hash}"
        );

        corpus.entries.push(CorpusEntry {
            ordinal: corpus.entries.len(),
            tx_hash_hex: tx_hash,
            offset: 0,
            length: tx_bytes.len() as u64,
            asset_kind: asset_kind.to_string(),
            tx_bytes,
        });
        added += 1;
    }

    corpus.manifest.tx_count = corpus.entries.len();
    corpus.manifest.created_at = unix_ts();
    if !source_label.is_empty() {
        corpus.manifest.source_label = format!("{},{}", corpus.manifest.source_label, source_label);
    }
    if !notes.is_empty() {
        if corpus.manifest.notes.is_empty() {
            corpus.manifest.notes = notes.to_string();
        } else {
            corpus.manifest.notes = format!("{} | {}", corpus.manifest.notes, notes);
        }
    }

    write_corpus_from_entries(corpus_dir, &corpus.manifest, &corpus.entries)?;
    Ok(added)
}

pub fn load_transactions_from_json_dir(json_dir: &Path) -> Result<Vec<Transaction>> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(json_dir)
        .with_context(|| format!("failed to read {}", json_dir.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    anyhow::ensure!(
        !files.is_empty(),
        "no json files found in {}",
        json_dir.display()
    );

    let mut txs = Vec::with_capacity(files.len());
    for file in files {
        let bytes =
            std::fs::read(&file).with_context(|| format!("failed reading {}", file.display()))?;
        let tx: Transaction = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed decoding Transaction JSON {}", file.display()))?;
        txs.push(tx);
    }
    Ok(txs)
}

fn write_index_csv(path: &Path, rows: &[IndexRow]) -> Result<()> {
    let mut wtr = csv::Writer::from_path(path)
        .with_context(|| format!("failed opening {} for write", path.display()))?;
    for row in rows {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    Ok(())
}

fn write_corpus_from_entries(
    out_dir: &Path,
    manifest: &Manifest,
    entries: &[CorpusEntry],
) -> Result<()> {
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let mut txs_bin = Vec::new();
    let mut rows = Vec::with_capacity(entries.len());
    for (ordinal, entry) in entries.iter().enumerate() {
        let offset = txs_bin.len() as u64 + 4;
        let length = entry.tx_bytes.len() as u64;
        txs_bin.write_all(&(entry.tx_bytes.len() as u32).to_le_bytes())?;
        txs_bin.write_all(&entry.tx_bytes)?;
        rows.push(IndexRow {
            ordinal,
            tx_hash_hex: entry.tx_hash_hex.clone(),
            offset,
            length,
            asset_kind: entry.asset_kind.clone(),
        });
    }

    let txs_path = out_dir.join("txs.bin");
    let index_path = out_dir.join("index.csv");
    let manifest_path = out_dir.join("manifest.json");

    std::fs::write(&txs_path, &txs_bin)
        .with_context(|| format!("failed to write {}", txs_path.display()))?;
    write_index_csv(&index_path, &rows)?;
    std::fs::write(&manifest_path, serde_json::to_vec_pretty(manifest)?)
        .with_context(|| format!("failed to write {}", manifest_path.display()))?;
    Ok(())
}

fn scan_txs_bin(bytes: &[u8]) -> Result<Vec<(u64, u64)>> {
    let mut out = Vec::new();
    let mut cursor = std::io::Cursor::new(bytes);
    let total_len = bytes.len() as u64;

    while cursor.position() < total_len {
        let mut len_bytes = [0u8; 4];
        cursor
            .read_exact(&mut len_bytes)
            .context("failed to read tx length prefix")?;
        let len = u32::from_le_bytes(len_bytes) as u64;
        let offset = cursor.position();
        let end = offset + len;
        anyhow::ensure!(end <= total_len, "tx payload exceeds txs.bin bounds");
        cursor.set_position(end);
        out.push((offset, len));
    }

    Ok(out)
}

fn unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_txs_bin_round_trip_offsets() {
        let tx_a = vec![1u8, 2, 3];
        let tx_b = vec![9u8, 8];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(tx_a.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&tx_a);
        bytes.extend_from_slice(&(tx_b.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&tx_b);

        let scanned = scan_txs_bin(&bytes).expect("scan should pass");
        assert_eq!(scanned, vec![(4, 3), (11, 2)]);
    }

    #[test]
    fn tx_hash_hex_is_stable() {
        let hash = tx_hash_hex(b"abc");
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn duplicate_hashes_rejected() {
        let dir =
            std::env::temp_dir().join(format!("penumbra-tps-dup-hash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        // Build a malformed corpus manually: same tx hash listed twice.
        let tx = vec![1u8, 2, 3];
        let mut txs = Vec::new();
        txs.extend_from_slice(&(tx.len() as u32).to_le_bytes());
        txs.extend_from_slice(&tx);
        txs.extend_from_slice(&(tx.len() as u32).to_le_bytes());
        txs.extend_from_slice(&tx);
        std::fs::write(dir.join("txs.bin"), txs).expect("write txs.bin");

        let hash = tx_hash_hex(&tx);
        let mut wtr = csv::Writer::from_path(dir.join("index.csv")).expect("open index");
        wtr.serialize(IndexRow {
            ordinal: 0,
            tx_hash_hex: hash.clone(),
            offset: 4,
            length: 3,
            asset_kind: "regulated".to_string(),
        })
        .expect("row 0");
        wtr.serialize(IndexRow {
            ordinal: 1,
            tx_hash_hex: hash,
            offset: 11,
            length: 3,
            asset_kind: "regulated".to_string(),
        })
        .expect("row 1");
        wtr.flush().expect("flush index");

        let manifest = Manifest {
            chain_id: "unknown".to_string(),
            genesis_hash: "unknown".to_string(),
            scenario: "regulated".to_string(),
            tx_count: 2,
            created_at: 0,
            source_label: "test".to_string(),
            notes: String::new(),
        };
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest).expect("manifest json"),
        )
        .expect("write manifest");

        let err = load_corpus(&dir).expect_err("duplicate hash should fail");
        assert!(err.to_string().contains("duplicate tx_hash_hex"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
