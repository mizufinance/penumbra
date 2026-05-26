use std::cell::{Cell, RefCell};

use digest::{Digest, Output};

const CHALLENGE_DOMAIN: &[u8] = b"penumbra.snarkpack.challenge.v1\0";

thread_local! {
    static CHALLENGE_CONTEXT: Cell<[u8; 32]> = const { Cell::new([0u8; 32]) };
    static CHALLENGE_TRACE: RefCell<Option<Vec<ChallengeTraceEntry>>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengeTraceEntry {
    pub stage_label: Vec<u8>,
    pub digest_bytes: Vec<u8>,
}

pub fn with_challenge_context<T>(context: [u8; 32], f: impl FnOnce() -> T) -> T {
    CHALLENGE_CONTEXT.with(|cell| {
        let previous = cell.replace(context);
        let output = f();
        cell.set(previous);
        output
    })
}

pub fn collect_challenge_trace<T>(f: impl FnOnce() -> T) -> (T, Vec<ChallengeTraceEntry>) {
    CHALLENGE_TRACE.with(|trace| {
        let previous = trace.replace(Some(Vec::new()));
        let output = f();
        let collected = trace.replace(previous).unwrap_or_default();
        (output, collected)
    })
}

pub(crate) fn challenge_digest<D: Digest>(
    stage_label: &'static [u8],
    nonce: usize,
    messages: &[u8],
) -> Output<D> {
    CHALLENGE_CONTEXT.with(|context| {
        let mut preimage = Vec::with_capacity(
            CHALLENGE_DOMAIN.len()
                + 4
                + stage_label.len()
                + 32
                + nonce.to_be_bytes().len()
                + messages.len(),
        );
        preimage.extend_from_slice(CHALLENGE_DOMAIN);
        preimage.extend_from_slice(&(stage_label.len() as u32).to_le_bytes());
        preimage.extend_from_slice(stage_label);
        preimage.extend_from_slice(&context.get());
        preimage.extend_from_slice(&nonce.to_be_bytes());
        preimage.extend_from_slice(messages);
        let digest = D::digest(&preimage);
        CHALLENGE_TRACE.with(|trace| {
            if let Some(entries) = trace.borrow_mut().as_mut() {
                entries.push(ChallengeTraceEntry {
                    stage_label: stage_label.to_vec(),
                    digest_bytes: digest.as_slice().to_vec(),
                });
            }
        });
        digest
    })
}
