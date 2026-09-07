use std::{
    convert::TryFrom,
    error::Error as ErrorTrait,
    fmt::{Display, Formatter, Result as FmtResult},
};

use digest::{Digest, Output};
use sha2::Sha256;

const CHALLENGE_DOMAIN: &[u8] = b"shieldd.snarkpack.challenge\0";
const CHALLENGE_CONTEXT_DOMAIN: &[u8] = b"shieldd.snarkpack.challenge_context\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengeContext {
    bytes: [u8; 32],
}

impl ChallengeContext {
    pub fn from_statement_digest(digest: [u8; 32]) -> Self {
        let preimage = challenge_context_preimage(digest);
        let mut hasher = Sha256::new();
        hasher.update(&preimage);
        Self::from_bytes(hasher.finalize().into())
    }

    /// Constructs the context from an already executed shipping SHA-256 call.
    #[doc(hidden)]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChallengeTraceEntry {
    pub stage_label: &'static [u8],
    pub nonce: u64,
    pub preimage: Vec<u8>,
    pub digest: Vec<u8>,
}

pub trait ChallengeTraceSink {
    fn record(&mut self, stage_label: &'static [u8], nonce: u64, preimage: &[u8], digest: &[u8]);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ChallengeNonceExhausted;

impl Display for ChallengeNonceExhausted {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("Fiat-Shamir rejection sampler exhausted the u64 nonce domain")
    }
}

impl ErrorTrait for ChallengeNonceExhausted {}

pub(crate) fn checked_next_challenge_nonce(nonce: u64) -> Option<u64> {
    if nonce == u64::MAX {
        None
    } else {
        Some(nonce + 1)
    }
}

pub(crate) fn next_challenge_nonce<E>(nonce: u64) -> Result<u64, E>
where
    E: From<ChallengeNonceExhausted>,
{
    checked_next_challenge_nonce(nonce).ok_or_else(|| ChallengeNonceExhausted.into())
}

/// Runs the deployed rejection-sampling schedule from nonce zero.
///
/// The candidate at a nonce is always queried before advancing. In
/// particular, `u64::MAX` is queried and may succeed; only rejection at that
/// nonce returns `ChallengeNonceExhausted`.
pub(crate) fn sample_bounded_challenge<T, E, F>(attempt: F) -> Result<T, E>
where
    E: From<ChallengeNonceExhausted>,
    F: FnMut(u64) -> Result<Option<T>, E>,
{
    sample_bounded_challenge_from_nonce(0, attempt)
}

/// Injectable form of `sample_bounded_challenge` used by the production
/// samplers and their nonce-boundary tests.
pub(crate) fn sample_bounded_challenge_from_nonce<T, E, F>(
    mut nonce: u64,
    mut attempt: F,
) -> Result<T, E>
where
    E: From<ChallengeNonceExhausted>,
    F: FnMut(u64) -> Result<Option<T>, E>,
{
    loop {
        if let Some(challenge) = attempt(nonce)? {
            return Ok(challenge);
        }
        nonce = next_challenge_nonce(nonce)?;
    }
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
        self.entries.push(ChallengeTraceEntry {
            stage_label,
            nonce,
            preimage: preimage.to_vec(),
            digest: digest.to_vec(),
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
    challenge_preimage_core(
        context.as_bytes(),
        &stage_label_len.to_le_bytes(),
        stage_label,
        &nonce.to_le_bytes(),
        messages,
    )
}

/// Extraction-friendly production core for the challenge frame. Integer
/// conversions remain explicit caller inputs, while this function fixes the
/// exact byte order consumed by every deployed challenge hash.
fn challenge_preimage_core(
    context: &[u8; 32],
    stage_label_len: &[u8; 4],
    stage_label: &[u8],
    nonce: &[u8; 8],
    messages: &[u8],
) -> Vec<u8> {
    let mut preimage = Vec::new();
    preimage.extend_from_slice(CHALLENGE_DOMAIN);
    preimage.extend_from_slice(stage_label_len);
    preimage.extend_from_slice(stage_label);
    preimage.extend_from_slice(context);
    preimage.extend_from_slice(nonce);
    preimage.extend_from_slice(messages);
    preimage
}

#[cfg(test)]
mod tests {
    use super::{
        challenge_preimage, challenge_preimage_core, checked_next_challenge_nonce,
        next_challenge_nonce, sample_bounded_challenge, sample_bounded_challenge_from_nonce,
        ChallengeContext, ChallengeNonceExhausted, CHALLENGE_DOMAIN,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum SamplerTestError {
        Attempt,
        Exhausted,
    }

    impl From<ChallengeNonceExhausted> for SamplerTestError {
        fn from(_: ChallengeNonceExhausted) -> Self {
            Self::Exhausted
        }
    }

    #[test]
    fn bounded_challenge_sampler_immediate_success_queries_nonce_zero_once() {
        let mut queries = Vec::new();
        let result = sample_bounded_challenge::<_, SamplerTestError, _>(|nonce| {
            queries.push(nonce);
            Ok(Some(17))
        })
        .unwrap();

        assert_eq!(result, 17);
        assert_eq!(queries, vec![0]);
    }

    #[test]
    fn bounded_challenge_sampler_retries_rejections_in_nonce_order() {
        let mut queries = Vec::new();
        let result = sample_bounded_challenge::<_, SamplerTestError, _>(|nonce| {
            queries.push(nonce);
            Ok((nonce == 3).then_some(23))
        })
        .unwrap();

        assert_eq!(result, 23);
        assert_eq!(queries, vec![0, 1, 2, 3]);
    }

    #[test]
    fn bounded_challenge_sampler_accepts_success_at_max_nonce() {
        let mut queries = Vec::new();
        let result =
            sample_bounded_challenge_from_nonce::<_, SamplerTestError, _>(u64::MAX, |nonce| {
                queries.push(nonce);
                Ok(Some(29))
            })
            .unwrap();

        assert_eq!(result, 29);
        assert_eq!(queries, vec![u64::MAX]);
    }

    #[test]
    fn bounded_challenge_sampler_rejection_at_max_fails_closed() {
        let mut queries = Vec::new();
        let error =
            sample_bounded_challenge_from_nonce::<u64, SamplerTestError, _>(u64::MAX, |nonce| {
                queries.push(nonce);
                Ok(None)
            })
            .unwrap_err();

        assert_eq!(error, SamplerTestError::Exhausted);
        assert_eq!(queries, vec![u64::MAX]);
    }

    #[test]
    fn bounded_challenge_sampler_queries_before_incrementing() {
        let mut queries = Vec::new();
        let result =
            sample_bounded_challenge_from_nonce::<_, SamplerTestError, _>(u64::MAX - 1, |nonce| {
                queries.push(nonce);
                Ok((nonce == u64::MAX).then_some(31))
            })
            .unwrap();

        assert_eq!(result, 31);
        assert_eq!(queries, vec![u64::MAX - 1, u64::MAX]);
    }

    #[test]
    fn bounded_challenge_sampler_preserves_attempt_error_before_exhaustion() {
        let mut queries = Vec::new();
        let error =
            sample_bounded_challenge_from_nonce::<u64, SamplerTestError, _>(u64::MAX, |nonce| {
                queries.push(nonce);
                Err(SamplerTestError::Attempt)
            })
            .unwrap_err();

        assert_eq!(error, SamplerTestError::Attempt);
        assert_eq!(queries, vec![u64::MAX]);
    }

    #[test]
    fn bounded_challenge_sampler_nonce_helpers_match_core_boundaries() {
        assert_eq!(checked_next_challenge_nonce(0), Some(1));
        assert_eq!(checked_next_challenge_nonce(u64::MAX - 1), Some(u64::MAX));
        assert_eq!(next_challenge_nonce::<crate::Error>(0).unwrap(), 1);
        assert_eq!(
            next_challenge_nonce::<crate::Error>(u64::MAX - 1).unwrap(),
            u64::MAX
        );
        assert_eq!(checked_next_challenge_nonce(u64::MAX), None);
        let error = next_challenge_nonce::<crate::Error>(u64::MAX).unwrap_err();
        assert_eq!(error.to_string(), ChallengeNonceExhausted.to_string());
    }

    #[test]
    fn challenge_preimage_core_is_the_shipping_frame() {
        let context = ChallengeContext { bytes: [0x43; 32] };
        let stage = b"tipp-mipp.kzg";
        let nonce: u64 = 0x0807_0605_0403_0201;
        let messages = [0x4d, 0x53, 0x47];

        let mut expected = Vec::new();
        expected.extend_from_slice(CHALLENGE_DOMAIN);
        expected.extend_from_slice(&(stage.len() as u32).to_le_bytes());
        expected.extend_from_slice(stage);
        expected.extend_from_slice(context.as_bytes());
        expected.extend_from_slice(&nonce.to_le_bytes());
        expected.extend_from_slice(&messages);

        assert_eq!(
            challenge_preimage(&context, stage, nonce, &messages),
            expected
        );
        assert_eq!(
            challenge_preimage_core(
                context.as_bytes(),
                &(stage.len() as u32).to_le_bytes(),
                stage,
                &nonce.to_le_bytes(),
                &messages,
            ),
            expected
        );
    }
}
