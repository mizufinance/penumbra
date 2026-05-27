use std::fmt;

pub const AGGREGATE_PROOF_WRAPPER_DOMAIN: &[u8] = b"penumbra.snarkpack.aggregate_proof.v1\0";

// Consensus-relevant bound: changing this cap changes which aggregate bundle
// bytes validators accept and requires protocol/security review.
pub const MAX_AGGREGATE_PROOF_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AggregateProofBytesError {
    BadVersion,
    StatementDigestMismatch,
    OversizeBytes { max: usize, got: usize },
    MalformedProofBytes,
}

impl fmt::Display for AggregateProofBytesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadVersion => write!(f, "unsupported aggregate proof wrapper version"),
            Self::StatementDigestMismatch => write!(f, "aggregate statement digest mismatch"),
            Self::OversizeBytes { max, got } => {
                write!(f, "aggregate proof bytes exceed cap: got {got}, max {max}")
            }
            Self::MalformedProofBytes => write!(f, "malformed aggregate proof bytes"),
        }
    }
}

impl std::error::Error for AggregateProofBytesError {}

pub fn encode_wrapped_aggregate_proof(
    statement_digest: [u8; 32],
    inner_proof_bytes: &[u8],
) -> Result<Vec<u8>, AggregateProofBytesError> {
    let inner_len = u32::try_from(inner_proof_bytes.len()).map_err(|_| {
        AggregateProofBytesError::OversizeBytes {
            max: u32::MAX as usize,
            got: inner_proof_bytes.len(),
        }
    })?;
    let mut bytes =
        Vec::with_capacity(AGGREGATE_PROOF_WRAPPER_DOMAIN.len() + 32 + 4 + inner_proof_bytes.len());
    bytes.extend_from_slice(AGGREGATE_PROOF_WRAPPER_DOMAIN);
    bytes.extend_from_slice(&statement_digest);
    bytes.extend_from_slice(&inner_len.to_le_bytes());
    bytes.extend_from_slice(inner_proof_bytes);
    Ok(bytes)
}

pub fn decode_wrapped_aggregate_proof<'a>(
    wrapped_proof_bytes: &'a [u8],
    expected_statement_digest: [u8; 32],
    max_aggregate_proof_bytes: Option<usize>,
) -> Result<&'a [u8], AggregateProofBytesError> {
    if let Some(max) = max_aggregate_proof_bytes {
        if wrapped_proof_bytes.len() > max {
            return Err(AggregateProofBytesError::OversizeBytes {
                max,
                got: wrapped_proof_bytes.len(),
            });
        }
    }

    let header_len = AGGREGATE_PROOF_WRAPPER_DOMAIN.len() + 32 + 4;
    if wrapped_proof_bytes.len() < header_len {
        return Err(AggregateProofBytesError::MalformedProofBytes);
    }
    if !wrapped_proof_bytes.starts_with(AGGREGATE_PROOF_WRAPPER_DOMAIN) {
        return Err(AggregateProofBytesError::BadVersion);
    }

    let digest_start = AGGREGATE_PROOF_WRAPPER_DOMAIN.len();
    let digest_end = digest_start + 32;
    let statement_digest = wrapped_proof_bytes
        .get(digest_start..digest_end)
        .ok_or(AggregateProofBytesError::MalformedProofBytes)?;
    if statement_digest != expected_statement_digest {
        return Err(AggregateProofBytesError::StatementDigestMismatch);
    }

    let len_start = digest_end;
    let len_end = len_start + 4;
    let inner_len = u32::from_le_bytes(
        wrapped_proof_bytes
            .get(len_start..len_end)
            .ok_or(AggregateProofBytesError::MalformedProofBytes)?
            .try_into()
            .map_err(|_| AggregateProofBytesError::MalformedProofBytes)?,
    ) as usize;
    let proof_start = len_end;
    let proof_end = proof_start
        .checked_add(inner_len)
        .ok_or(AggregateProofBytesError::MalformedProofBytes)?;
    if proof_end != wrapped_proof_bytes.len() {
        return Err(AggregateProofBytesError::MalformedProofBytes);
    }

    wrapped_proof_bytes
        .get(proof_start..proof_end)
        .ok_or(AggregateProofBytesError::MalformedProofBytes)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_wrapped_aggregate_proof, encode_wrapped_aggregate_proof, AggregateProofBytesError,
    };

    #[test]
    fn wrapper_round_trips_inner_proof() {
        let digest = [7u8; 32];
        let inner = vec![1, 2, 3, 4, 5];
        let wrapped = encode_wrapped_aggregate_proof(digest, &inner).expect("wrapper encode");

        let decoded =
            decode_wrapped_aggregate_proof(&wrapped, digest, None).expect("wrapper decode");

        assert_eq!(decoded, inner.as_slice());
    }

    #[test]
    fn wrapper_rejects_statement_mismatch() {
        let wrapped =
            encode_wrapped_aggregate_proof([7u8; 32], &[1, 2, 3]).expect("wrapper encode");

        let err = decode_wrapped_aggregate_proof(&wrapped, [8u8; 32], None)
            .expect_err("wrong digest should reject");

        assert_eq!(err, AggregateProofBytesError::StatementDigestMismatch);
    }

    #[test]
    fn wrapper_rejects_malformed_length() {
        let mut wrapped =
            encode_wrapped_aggregate_proof([7u8; 32], &[1, 2, 3]).expect("wrapper encode");
        wrapped.pop();

        let err = decode_wrapped_aggregate_proof(&wrapped, [7u8; 32], None)
            .expect_err("truncated proof should reject");

        assert_eq!(err, AggregateProofBytesError::MalformedProofBytes);
    }

    #[test]
    fn wrapper_rejects_oversize_before_parsing() {
        let wrapped =
            encode_wrapped_aggregate_proof([7u8; 32], &[1, 2, 3]).expect("wrapper encode");

        let err = decode_wrapped_aggregate_proof(&wrapped, [7u8; 32], Some(1))
            .expect_err("oversize wrapper should reject");

        assert_eq!(
            err,
            AggregateProofBytesError::OversizeBytes {
                max: 1,
                got: wrapped.len()
            }
        );
    }
}
