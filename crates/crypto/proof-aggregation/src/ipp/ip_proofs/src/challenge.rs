use std::convert::TryFrom;

use digest::{Digest, Output};
use penumbra_sdk_proof_aggregation_trace_schema::{
    TraceComparisonLevel, TraceEvent, TraceEventKind,
};
use sha2::Sha256;

const CHALLENGE_DOMAIN: &[u8] = b"penumbra.snarkpack.challenge.v1\0";
const CHALLENGE_CONTEXT_DOMAIN: &[u8] = b"penumbra.snarkpack.challenge_context.v1\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengeContext {
    bytes: [u8; 32],
}

impl ChallengeContext {
    pub fn from_statement_digest(digest: [u8; 32]) -> Self {
        let preimage = challenge_context_preimage(digest);
        let mut hasher = Sha256::new();
        hasher.update(&preimage);
        Self {
            bytes: hasher.finalize().into(),
        }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

pub fn challenge_context_preimage(statement_digest: [u8; 32]) -> Vec<u8> {
    let mut preimage = Vec::with_capacity(CHALLENGE_CONTEXT_DOMAIN.len() + 32);
    preimage.extend_from_slice(CHALLENGE_CONTEXT_DOMAIN);
    preimage.extend_from_slice(&statement_digest);
    preimage
}

pub type ChallengeTraceEntry = TraceEvent;

pub trait ChallengeTraceSink {
    fn record(&mut self, stage_label: &'static [u8], nonce: u64, preimage: &[u8], digest: &[u8]);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopChallengeTraceSink;

impl ChallengeTraceSink for NoopChallengeTraceSink {
    #[inline]
    fn record(
        &mut self,
        _stage_label: &'static [u8],
        _nonce: u64,
        _preimage: &[u8],
        _digest: &[u8],
    ) {
    }
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
    fn record(&mut self, stage_label: &'static [u8], nonce: u64, preimage: &[u8], digest: &[u8]) {
        let stage_label_str = trace_stage_label(stage_label);
        self.entries.push(TraceEvent {
            spec_row_id: "fs.stage-labels",
            primary_level: TraceComparisonLevel::PenumbraByte,
            event_kind: TraceEventKind::ChallengePreimage,
            stage_label: stage_label_str,
            nonce: Some(nonce),
            round_index: None,
            byte_payload: stage_label.to_vec(),
            abstract_payload: None,
            filecoin_bug_class: None,
        });
        self.entries.push(TraceEvent {
            spec_row_id: "fs.challenge-preimage",
            primary_level: TraceComparisonLevel::PenumbraByte,
            event_kind: TraceEventKind::ChallengePreimage,
            stage_label: stage_label_str,
            nonce: Some(nonce),
            round_index: None,
            byte_payload: preimage.to_vec(),
            abstract_payload: None,
            filecoin_bug_class: None,
        });
        self.entries.push(TraceEvent {
            spec_row_id: trace_spec_row_id(stage_label),
            primary_level: TraceComparisonLevel::PenumbraByte,
            event_kind: TraceEventKind::ChallengeDigest,
            stage_label: stage_label_str,
            nonce: Some(nonce),
            round_index: None,
            byte_payload: digest.to_vec(),
            abstract_payload: None,
            filecoin_bug_class: None,
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
    trace.record(stage_label, nonce, &preimage, digest.as_slice());
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

fn trace_stage_label(stage_label: &'static [u8]) -> &'static str {
    match stage_label {
        b"aggregate.randomizer" => "aggregate.randomizer",
        b"tipa.ab.gipa.round" => "tipa.ab.gipa.round",
        b"tipa.ab.kzg" => "tipa.ab.kzg",
        b"tipa.c.gipa.round" => "tipa.c.gipa.round",
        b"tipa.c.kzg" => "tipa.c.kzg",
        b"tipa.generic.gipa.round" => "tipa.generic.gipa.round",
        b"tipa.generic.kzg" => "tipa.generic.kzg",
        b"tipa.generic.ssm.gipa.round" => "tipa.generic.ssm.gipa.round",
        _ => "unknown",
    }
}

fn trace_spec_row_id(stage_label: &'static [u8]) -> &'static str {
    match stage_label {
        b"aggregate.randomizer" => "groth16.randomizer",
        b"tipa.ab.kzg" | b"tipa.generic.kzg" => "tipa.ab.kzg-challenge",
        b"tipa.c.kzg" => "ssm.kzg-challenge",
        b"tipa.ab.gipa.round"
        | b"tipa.c.gipa.round"
        | b"tipa.generic.gipa.round"
        | b"tipa.generic.ssm.gipa.round" => "gipa.challenge-dependency",
        _ => "fs.challenge-preimage",
    }
}
