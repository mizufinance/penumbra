//! BankdProvider implements the [`ClientProvider`] trait for bankd light clients.
//!
//! The verification pipeline:
//! 1. Extract bankd types from `AnyClientState`/`AnyConsensusState`/`AnyHeader`
//! 2. Validate header height advances beyond client's latest height
//! 3. Check trusting period using host timestamp
//! 4. Reconstruct `BlockId = Keccak256(encode(block))`
//! 5. Compute `ConsensusDigest = SHA256(BlockId)`
//! 6. Verify BLS12-381 MinSig threshold signature over ConsensusDigest

use anyhow::{ensure, Result};
use prost::Message as _;
use sha2::{Digest as _, Sha256};
use tiny_keccak::{Hasher as _, Keccak};

use crate::bankd_verification::{validate_group_public_key, verify_bls_threshold_signature};
use crate::client_provider::ClientProvider;
use crate::client_types::{
    AnyClientState, AnyConsensusState, AnyHeader, BankdClientState, BankdConsensusState,
    BankdHeader, BankdMisbehaviour, BANKD_MISBEHAVIOUR_TYPE_URL,
};

/// Light client verifier for the bankd chain (Kora/commonware consensus).
pub struct BankdProvider;

impl ClientProvider for BankdProvider {
    fn verify_header(
        &self,
        client_state: &AnyClientState,
        trusted_consensus: &AnyConsensusState,
        header: &AnyHeader,
        host_timestamp: u64,
    ) -> Result<(AnyClientState, AnyConsensusState)> {
        let cs = client_state.as_bankd()?;
        let cons = trusted_consensus.as_bankd()?;
        let hdr = header.as_bankd()?;

        // --- 1. Validate height monotonicity ---
        let header_height = hdr
            .height
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("bankd header missing height"))?;
        let latest_height = cs
            .latest_height
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("bankd client state missing latest_height"))?;
        ensure!(
            header_height.revision_height > latest_height.revision_height,
            "bankd header height {} must be greater than client latest height {}",
            header_height.revision_height,
            latest_height.revision_height,
        );

        // --- 2. Trusting period check (host timestamp based) ---
        if cs.trusting_period_secs > 0 {
            let elapsed = host_timestamp.checked_sub(cons.timestamp).ok_or_else(|| {
                anyhow::anyhow!(
                    "host timestamp {} is before consensus timestamp {}",
                    host_timestamp,
                    cons.timestamp
                )
            })?;
            ensure!(
                elapsed < cs.trusting_period_secs,
                "bankd trusting period expired: elapsed {}s >= trusting period {}s",
                elapsed,
                cs.trusting_period_secs,
            );
        }

        // --- 3. Reconstruct BlockId = Keccak256(encode(block)) ---
        let encoded_block = encode_block(hdr)?;
        let block_id = keccak256(&encoded_block);

        // --- 4. ConsensusDigest = SHA256(BlockId) ---
        let consensus_digest = sha256(&block_id);

        // --- 5. Verify BLS threshold signature ---
        validate_group_public_key(&cs.group_public_key)?;
        let gpk: &[u8; 96] = cs.group_public_key.as_slice().try_into().map_err(|_| {
            anyhow::anyhow!(
                "bankd group public key must be 96 bytes, got {}",
                cs.group_public_key.len()
            )
        })?;

        let sig: &[u8; 48] = hdr
            .finalization_certificate
            .as_slice()
            .try_into()
            .map_err(|_| {
                anyhow::anyhow!(
                    "bankd finalization certificate must be 48 bytes, got {}",
                    hdr.finalization_certificate.len()
                )
            })?;

        let valid = verify_bls_threshold_signature(gpk, &consensus_digest, sig)?;
        ensure!(valid, "bankd BLS threshold signature verification failed");

        // --- 6. Return updated states ---
        let new_client_state = BankdClientState {
            latest_height: hdr.height.clone(),
            ..cs.clone()
        };

        let new_consensus_state = BankdConsensusState {
            root: hdr.new_root.clone(),
            timestamp: hdr.timestamp,
            group_public_key: cs.group_public_key.clone(),
        };

        Ok((
            AnyClientState::Bankd(new_client_state),
            AnyConsensusState::Bankd(new_consensus_state),
        ))
    }

    fn check_misbehaviour(
        &self,
        client_state: &AnyClientState,
        misbehaviour: &ibc_proto::google::protobuf::Any,
    ) -> Result<bool> {
        let cs = client_state.as_bankd()?;

        ensure!(
            misbehaviour.type_url == BANKD_MISBEHAVIOUR_TYPE_URL,
            "unexpected misbehaviour type URL: {}",
            misbehaviour.type_url,
        );

        let mb = BankdMisbehaviour::decode(misbehaviour.value.as_ref())
            .map_err(|e| anyhow::anyhow!("failed to decode bankd misbehaviour: {e}"))?;

        let h1 = mb
            .header_1
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("bankd misbehaviour missing header_1"))?;
        let h2 = mb
            .header_2
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("bankd misbehaviour missing header_2"))?;

        // Both headers must be for the same height
        let h1_height = h1
            .height
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("header_1 missing height"))?;
        let h2_height = h2
            .height
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("header_2 missing height"))?;

        ensure!(
            h1_height.revision_height == h2_height.revision_height
                && h1_height.revision_number == h2_height.revision_number,
            "bankd misbehaviour headers must be for the same height (got {}/{} and {}/{})",
            h1_height.revision_number,
            h1_height.revision_height,
            h2_height.revision_number,
            h2_height.revision_height,
        );

        // Compute BlockIds — must differ (equivocation)
        let block_id_1 = keccak256(&encode_block(h1)?);
        let block_id_2 = keccak256(&encode_block(h2)?);

        ensure!(
            block_id_1 != block_id_2,
            "bankd misbehaviour headers have identical BlockIds — not equivocation",
        );

        // Verify both BLS signatures
        validate_group_public_key(&cs.group_public_key)?;
        let gpk: &[u8; 96] = cs
            .group_public_key
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("bankd group public key must be 96 bytes"))?;

        let digest_1 = sha256(&block_id_1);
        let sig_1: &[u8; 48] = h1
            .finalization_certificate
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("header_1 finalization certificate must be 48 bytes"))?;
        let valid_1 = verify_bls_threshold_signature(gpk, &digest_1, sig_1)?;
        ensure!(valid_1, "header_1 BLS signature verification failed");

        let digest_2 = sha256(&block_id_2);
        let sig_2: &[u8; 48] = h2
            .finalization_certificate
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("header_2 finalization certificate must be 48 bytes"))?;
        let valid_2 = verify_bls_threshold_signature(gpk, &digest_2, sig_2)?;
        ensure!(valid_2, "header_2 BLS signature verification failed");

        Ok(true)
    }
}

// ---------------------------------------------------------------------------
// Block encoding — must match Kora's commonware-codec Write implementation
// ---------------------------------------------------------------------------

/// Encode a bankd block matching Kora's `commonware_codec::Write` field order.
///
/// Field order:
/// 1. parent:     BlockId   (32 raw bytes)
/// 2. height:     u64       (8 bytes big-endian)
/// 3. timestamp:  u64       (8 bytes big-endian)
/// 4. prevrandao: B256      (32 raw bytes)
/// 5. state_root: StateRoot (32 raw bytes)
/// 6. ibc_root:   B256      (32 raw bytes)
/// 7. txs:        Vec<Tx>   (varint count + each tx varint-length-prefixed)
fn encode_block(header: &BankdHeader) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(256);

    // parent: BlockId (32 raw bytes)
    ensure!(
        header.parent_hash.len() == 32,
        "parent_hash must be 32 bytes, got {}",
        header.parent_hash.len()
    );
    buf.extend_from_slice(&header.parent_hash);

    // height: u64 big-endian
    let height = header
        .height
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("header missing height"))?
        .revision_height;
    buf.extend_from_slice(&height.to_be_bytes());

    // timestamp: u64 big-endian
    buf.extend_from_slice(&header.timestamp.to_be_bytes());

    // prevrandao: B256 (32 raw bytes)
    ensure!(
        header.prevrandao.len() == 32,
        "prevrandao must be 32 bytes, got {}",
        header.prevrandao.len()
    );
    buf.extend_from_slice(&header.prevrandao);

    // state_root: StateRoot (32 raw bytes)
    ensure!(
        header.state_root.len() == 32,
        "state_root must be 32 bytes, got {}",
        header.state_root.len()
    );
    buf.extend_from_slice(&header.state_root);

    // ibc_root: B256 (32 raw bytes)
    ensure!(
        header.ibc_root.len() == 32,
        "ibc_root must be 32 bytes, got {}",
        header.ibc_root.len()
    );
    buf.extend_from_slice(&header.ibc_root);

    // txs: Vec<Tx> — varint(count) + for each tx: varint(len) + bytes
    write_varint_u32(header.transactions.len() as u32, &mut buf);
    for tx in &header.transactions {
        write_varint_u32(tx.len() as u32, &mut buf);
        buf.extend_from_slice(tx);
    }

    Ok(buf)
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    hasher.update(data);
    let mut output = [0u8; 32];
    hasher.finalize(&mut output);
    output
}

fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Encode a u32 as a protobuf-style varint (7 data bits per byte, MSB continuation).
/// Matches `commonware_codec::varint::UInt<u32>::write()`.
fn write_varint_u32(mut value: u32, buf: &mut Vec<u8>) {
    loop {
        if value < 0x80 {
            buf.push(value as u8);
            return;
        }
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_types::{BankdClientState, BankdConsensusState, BankdHeader};

    // -- Test helpers --

    fn test_keypair() -> (blst::min_sig::SecretKey, [u8; 96]) {
        let ikm = [42u8; 32];
        let sk = blst::min_sig::SecretKey::key_gen(&ikm, &[]).expect("keygen");
        let pk = sk.sk_to_pk();
        let pk_bytes: [u8; 96] = pk.compress();
        (sk, pk_bytes)
    }

    fn sign_header(sk: &blst::min_sig::SecretKey, header: &BankdHeader) -> Vec<u8> {
        let encoded = encode_block(header).expect("encode");
        let block_id = keccak256(&encoded);
        let consensus_digest = sha256(&block_id);

        // Sign using union_unique(SIMPLEX_NAMESPACE, consensus_digest) + BLS_DST
        let payload = crate::bankd_verification::union_unique(
            crate::bankd_verification::SIMPLEX_NAMESPACE,
            &consensus_digest,
        );
        let sig = sk.sign(&payload, crate::bankd_verification::BLS_DST, &[]);
        sig.compress().to_vec()
    }

    fn make_client_state(pk_bytes: &[u8; 96], height: u64) -> BankdClientState {
        BankdClientState {
            chain_id: "bankd-test-1".to_string(),
            latest_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: height,
            }),
            frozen_height: None,
            proof_specs: vec![],
            group_public_key: pk_bytes.to_vec(),
            trusting_period_secs: 86_400,
        }
    }

    fn make_consensus_state(pk_bytes: &[u8; 96], timestamp: u64) -> BankdConsensusState {
        BankdConsensusState {
            root: vec![0xaa; 32],
            timestamp,
            group_public_key: pk_bytes.to_vec(),
        }
    }

    fn make_header(height: u64, timestamp: u64) -> BankdHeader {
        BankdHeader {
            height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: height,
            }),
            trusted_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: height - 1,
            }),
            timestamp,
            new_root: vec![0xbb; 32],
            parent_hash: vec![0x01; 32],
            prevrandao: vec![0x02; 32],
            state_root: vec![0x03; 32],
            ibc_root: vec![0x04; 32],
            transactions: vec![vec![0x05; 64]],
            finalization_certificate: vec![], // filled in by sign_header
        }
    }

    // -- Encoding tests --

    #[test]
    fn encode_block_deterministic() {
        let h = make_header(10, 1_700_000_000);
        let enc1 = encode_block(&h).expect("encode");
        let enc2 = encode_block(&h).expect("encode");
        assert_eq!(enc1, enc2);
    }

    #[test]
    fn encode_block_field_order() {
        let h = BankdHeader {
            height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: 42,
            }),
            trusted_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: 41,
            }),
            timestamp: 1000,
            new_root: vec![0xcc; 32],
            parent_hash: vec![0x11; 32],
            prevrandao: vec![0x22; 32],
            state_root: vec![0x33; 32],
            ibc_root: vec![0x44; 32],
            transactions: vec![],
            finalization_certificate: vec![],
        };

        let enc = encode_block(&h).expect("encode");

        // parent: 32 bytes
        assert_eq!(&enc[0..32], &[0x11; 32]);
        // height: u64 BE = 42
        assert_eq!(&enc[32..40], &42u64.to_be_bytes());
        // timestamp: u64 BE = 1000
        assert_eq!(&enc[40..48], &1000u64.to_be_bytes());
        // prevrandao: 32 bytes
        assert_eq!(&enc[48..80], &[0x22; 32]);
        // state_root: 32 bytes
        assert_eq!(&enc[80..112], &[0x33; 32]);
        // ibc_root: 32 bytes
        assert_eq!(&enc[112..144], &[0x44; 32]);
        // txs: varint(0) = single byte 0x00
        assert_eq!(enc[144], 0x00);
        assert_eq!(enc.len(), 145);
    }

    #[test]
    fn encode_block_with_transactions() {
        let h = BankdHeader {
            height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: 1,
            }),
            trusted_height: Some(ibc_proto::ibc::core::client::v1::Height {
                revision_number: 0,
                revision_height: 0,
            }),
            timestamp: 100,
            new_root: vec![0xcc; 32],
            parent_hash: vec![0x00; 32],
            prevrandao: vec![0x00; 32],
            state_root: vec![0x00; 32],
            ibc_root: vec![0x00; 32],
            transactions: vec![vec![0xAA; 3], vec![0xBB; 5]],
            finalization_certificate: vec![],
        };

        let enc = encode_block(&h).expect("encode");
        let tx_start = 144; // after 5 fixed fields (32*5) + 2 u64s (8*2) = 176... wait

        // Fixed fields: parent(32) + height(8) + timestamp(8) + prevrandao(32) + state_root(32) + ibc_root(32) = 144
        // txs: varint(2) = 0x02, then tx1: varint(3) + 3 bytes, tx2: varint(5) + 5 bytes
        assert_eq!(enc[tx_start], 0x02); // count = 2
        assert_eq!(enc[tx_start + 1], 0x03); // tx1 len = 3
        assert_eq!(&enc[tx_start + 2..tx_start + 5], &[0xAA; 3]);
        assert_eq!(enc[tx_start + 5], 0x05); // tx2 len = 5
        assert_eq!(&enc[tx_start + 6..tx_start + 11], &[0xBB; 5]);
        assert_eq!(enc.len(), tx_start + 11);
    }

    #[test]
    fn encode_block_rejects_wrong_field_lengths() {
        let mut h = make_header(1, 100);
        h.parent_hash = vec![0x00; 31]; // wrong length
        assert!(encode_block(&h).is_err());

        let mut h = make_header(1, 100);
        h.prevrandao = vec![0x00; 33];
        assert!(encode_block(&h).is_err());

        let mut h = make_header(1, 100);
        h.state_root = vec![];
        assert!(encode_block(&h).is_err());

        let mut h = make_header(1, 100);
        h.ibc_root = vec![0x00; 16];
        assert!(encode_block(&h).is_err());
    }

    // -- Keccak256 / SHA256 sanity --

    #[test]
    fn keccak256_known_value() {
        // keccak256("") = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
        let hash = keccak256(b"");
        assert_eq!(
            hex::encode(hash),
            "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470"
        );
    }

    #[test]
    fn sha256_known_value() {
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        let hash = sha256(b"");
        assert_eq!(
            hex::encode(hash),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // -- verify_header tests --

    #[test]
    fn verify_header_valid() {
        let (sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);
        let cons = make_consensus_state(&pk_bytes, 1_700_000_000);

        let mut hdr = make_header(11, 1_700_000_100);
        hdr.finalization_certificate = sign_header(&sk, &hdr);

        let provider = BankdProvider;
        let result = provider.verify_header(
            &AnyClientState::Bankd(cs),
            &AnyConsensusState::Bankd(cons),
            &AnyHeader::Bankd(hdr),
            1_700_000_200, // host timestamp
        );
        assert!(
            result.is_ok(),
            "valid header should verify: {:?}",
            result.err()
        );

        let (new_cs, new_cons) = result.expect("verified");
        let new_bankd_cs = new_cs.as_bankd().expect("bankd");
        assert_eq!(
            new_bankd_cs
                .latest_height
                .as_ref()
                .expect("height")
                .revision_height,
            11
        );

        let new_bankd_cons = new_cons.as_bankd().expect("bankd");
        assert_eq!(new_bankd_cons.timestamp, 1_700_000_100);
        assert_eq!(new_bankd_cons.root, vec![0xbb; 32]);
    }

    #[test]
    fn verify_header_rejects_non_advancing_height() {
        let (sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);
        let cons = make_consensus_state(&pk_bytes, 1_700_000_000);

        let mut hdr = make_header(10, 1_700_000_100); // same height, not advancing
        hdr.finalization_certificate = sign_header(&sk, &hdr);

        let provider = BankdProvider;
        let err = provider
            .verify_header(
                &AnyClientState::Bankd(cs),
                &AnyConsensusState::Bankd(cons),
                &AnyHeader::Bankd(hdr),
                1_700_000_200,
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("must be greater than"),
            "got: {err}"
        );
    }

    #[test]
    fn verify_header_rejects_expired_trusting_period() {
        let (sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);
        let cons = make_consensus_state(&pk_bytes, 1_700_000_000);

        let mut hdr = make_header(11, 1_700_100_000);
        hdr.finalization_certificate = sign_header(&sk, &hdr);

        let provider = BankdProvider;
        let err = provider
            .verify_header(
                &AnyClientState::Bankd(cs),
                &AnyConsensusState::Bankd(cons),
                &AnyHeader::Bankd(hdr),
                1_700_100_000, // 100_000s elapsed > 86_400s trusting period
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("trusting period expired"),
            "got: {err}"
        );
    }

    #[test]
    fn verify_header_zero_trusting_period_never_expires() {
        let (sk, pk_bytes) = test_keypair();
        let mut cs = make_client_state(&pk_bytes, 10);
        cs.trusting_period_secs = 0; // never expires
        let cons = make_consensus_state(&pk_bytes, 1_000_000_000);

        let mut hdr = make_header(11, 2_000_000_000);
        hdr.finalization_certificate = sign_header(&sk, &hdr);

        let provider = BankdProvider;
        let result = provider.verify_header(
            &AnyClientState::Bankd(cs),
            &AnyConsensusState::Bankd(cons),
            &AnyHeader::Bankd(hdr),
            2_000_000_000, // 1 billion seconds elapsed, but trusting period is 0
        );
        assert!(result.is_ok());
    }

    #[test]
    fn verify_header_rejects_invalid_bls_signature() {
        let (_sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);
        let cons = make_consensus_state(&pk_bytes, 1_700_000_000);

        let mut hdr = make_header(11, 1_700_000_100);
        // Use a valid-looking but wrong signature (sign with different key)
        let wrong_ikm = [99u8; 32];
        let wrong_sk = blst::min_sig::SecretKey::key_gen(&wrong_ikm, &[]).expect("keygen");
        hdr.finalization_certificate = sign_header(&wrong_sk, &hdr);

        let provider = BankdProvider;
        let err = provider
            .verify_header(
                &AnyClientState::Bankd(cs),
                &AnyConsensusState::Bankd(cons),
                &AnyHeader::Bankd(hdr),
                1_700_000_200,
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("signature verification failed"),
            "got: {err}"
        );
    }

    #[test]
    fn verify_header_rejects_tampered_fields() {
        let (sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);
        let cons = make_consensus_state(&pk_bytes, 1_700_000_000);

        let mut hdr = make_header(11, 1_700_000_100);
        hdr.finalization_certificate = sign_header(&sk, &hdr);
        // Tamper with state_root after signing
        hdr.state_root = vec![0xFF; 32];

        let provider = BankdProvider;
        let err = provider
            .verify_header(
                &AnyClientState::Bankd(cs),
                &AnyConsensusState::Bankd(cons),
                &AnyHeader::Bankd(hdr),
                1_700_000_200,
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("signature verification failed"),
            "got: {err}"
        );
    }

    // -- check_misbehaviour tests --

    #[test]
    fn check_misbehaviour_detects_equivocation() {
        let (sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);

        // Two headers at the same height but with different content
        let mut h1 = make_header(11, 1_700_000_100);
        h1.state_root = vec![0xAA; 32]; // different state roots
        h1.finalization_certificate = sign_header(&sk, &h1);

        let mut h2 = make_header(11, 1_700_000_100);
        h2.state_root = vec![0xBB; 32]; // different state roots
        h2.finalization_certificate = sign_header(&sk, &h2);

        let mb = BankdMisbehaviour {
            client_id: "bankd-0".to_string(),
            header_1: Some(h1),
            header_2: Some(h2),
        };

        let mb_any = ibc_proto::google::protobuf::Any {
            type_url: BANKD_MISBEHAVIOUR_TYPE_URL.to_string(),
            value: prost::Message::encode_to_vec(&mb),
        };

        let provider = BankdProvider;
        let result = provider.check_misbehaviour(&AnyClientState::Bankd(cs), &mb_any);
        assert!(result.is_ok());
        assert!(result.expect("should succeed"));
    }

    #[test]
    fn check_misbehaviour_rejects_same_block() {
        let (sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);

        // Two identical headers — NOT equivocation
        let mut h1 = make_header(11, 1_700_000_100);
        h1.finalization_certificate = sign_header(&sk, &h1);
        let h2 = h1.clone();

        let mb = BankdMisbehaviour {
            client_id: "bankd-0".to_string(),
            header_1: Some(h1),
            header_2: Some(h2),
        };

        let mb_any = ibc_proto::google::protobuf::Any {
            type_url: BANKD_MISBEHAVIOUR_TYPE_URL.to_string(),
            value: prost::Message::encode_to_vec(&mb),
        };

        let provider = BankdProvider;
        let err = provider
            .check_misbehaviour(&AnyClientState::Bankd(cs), &mb_any)
            .unwrap_err();
        assert!(err.to_string().contains("not equivocation"), "got: {err}");
    }

    #[test]
    fn check_misbehaviour_rejects_different_heights() {
        let (sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);

        let mut h1 = make_header(11, 1_700_000_100);
        h1.finalization_certificate = sign_header(&sk, &h1);

        let mut h2 = make_header(12, 1_700_000_200); // different height
        h2.state_root = vec![0xBB; 32];
        h2.finalization_certificate = sign_header(&sk, &h2);

        let mb = BankdMisbehaviour {
            client_id: "bankd-0".to_string(),
            header_1: Some(h1),
            header_2: Some(h2),
        };

        let mb_any = ibc_proto::google::protobuf::Any {
            type_url: BANKD_MISBEHAVIOUR_TYPE_URL.to_string(),
            value: prost::Message::encode_to_vec(&mb),
        };

        let provider = BankdProvider;
        let err = provider
            .check_misbehaviour(&AnyClientState::Bankd(cs), &mb_any)
            .unwrap_err();
        assert!(err.to_string().contains("same height"), "got: {err}");
    }

    #[test]
    fn check_misbehaviour_rejects_invalid_sig() {
        let (sk, pk_bytes) = test_keypair();
        let cs = make_client_state(&pk_bytes, 10);

        let mut h1 = make_header(11, 1_700_000_100);
        h1.state_root = vec![0xAA; 32];
        h1.finalization_certificate = sign_header(&sk, &h1);

        // h2 signed with wrong key
        let wrong_ikm = [99u8; 32];
        let wrong_sk = blst::min_sig::SecretKey::key_gen(&wrong_ikm, &[]).expect("keygen");
        let mut h2 = make_header(11, 1_700_000_100);
        h2.state_root = vec![0xBB; 32];
        h2.finalization_certificate = sign_header(&wrong_sk, &h2);

        let mb = BankdMisbehaviour {
            client_id: "bankd-0".to_string(),
            header_1: Some(h1),
            header_2: Some(h2),
        };

        let mb_any = ibc_proto::google::protobuf::Any {
            type_url: BANKD_MISBEHAVIOUR_TYPE_URL.to_string(),
            value: prost::Message::encode_to_vec(&mb),
        };

        let provider = BankdProvider;
        let err = provider
            .check_misbehaviour(&AnyClientState::Bankd(cs), &mb_any)
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("BLS signature verification failed"),
            "got: {err}"
        );
    }
}
