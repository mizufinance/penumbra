use std::convert::TryFrom;

use digest::{Digest, Output};
use sha2::Sha256;

const CHALLENGE_DOMAIN: &[u8] = b"penumbra.snarkpack.challenge.v1\0";
const CHALLENGE_CONTEXT_DOMAIN: &[u8] = b"penumbra.snarkpack.challenge_context.v1\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengeContext {
    bytes: [u8; 32],
}

impl ChallengeContext {
    pub fn from_statement_digest(digest: [u8; 32]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(CHALLENGE_CONTEXT_DOMAIN);
        hasher.update(digest);
        Self {
            bytes: hasher.finalize().into(),
        }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengeTraceEntry {
    pub stage_label: Vec<u8>,
    pub nonce: u64,
    pub digest_bytes: Vec<u8>,
}

pub trait ChallengeTraceSink {
    fn record(&mut self, stage_label: &'static [u8], nonce: u64, digest: &[u8]);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopChallengeTraceSink;

impl ChallengeTraceSink for NoopChallengeTraceSink {
    #[inline]
    fn record(&mut self, _stage_label: &'static [u8], _nonce: u64, _digest: &[u8]) {}
}

#[derive(Clone, Debug, Default)]
pub struct VecChallengeTraceSink {
    entries: Vec<ChallengeTraceEntry>,
}

impl VecChallengeTraceSink {
    pub fn entries(&self) -> &[ChallengeTraceEntry] {
        &self.entries
    }

    pub fn into_entries(self) -> Vec<ChallengeTraceEntry> {
        self.entries
    }
}

impl ChallengeTraceSink for VecChallengeTraceSink {
    fn record(&mut self, stage_label: &'static [u8], nonce: u64, digest: &[u8]) {
        self.entries.push(ChallengeTraceEntry {
            stage_label: stage_label.to_vec(),
            nonce,
            digest_bytes: digest.to_vec(),
        });
    }
}

pub(crate) fn challenge_digest<D, S>(
    context: &ChallengeContext,
    trace: &mut S,
    stage_label: &'static [u8],
    nonce: u64,
    messages: &[u8],
) -> Output<D>
where
    D: Digest,
    S: ChallengeTraceSink,
{
    let preimage = challenge_preimage(context, stage_label, nonce, messages);
    let digest = D::digest(&preimage);
    trace.record(stage_label, nonce, digest.as_slice());
    digest
}

pub fn challenge_preimage(
    context: &ChallengeContext,
    stage_label: &'static [u8],
    nonce: u64,
    messages: &[u8],
) -> Vec<u8> {
    let stage_label_len = u32::try_from(stage_label.len()).expect("challenge stage label fits u32");
    let mut preimage = Vec::with_capacity(
        CHALLENGE_DOMAIN.len() + 4 + stage_label.len() + 32 + 8 + messages.len(),
    );
    preimage.extend_from_slice(CHALLENGE_DOMAIN);
    preimage.extend_from_slice(&stage_label_len.to_le_bytes());
    preimage.extend_from_slice(stage_label);
    preimage.extend_from_slice(context.as_bytes());
    preimage.extend_from_slice(&nonce.to_le_bytes());
    preimage.extend_from_slice(messages);
    preimage
}
