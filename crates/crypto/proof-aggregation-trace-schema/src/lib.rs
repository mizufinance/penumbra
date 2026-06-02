#![no_std]
#![forbid(unsafe_code)]

//! Dependency-free trace schema for SnarkPack proof aggregation.
//!
//! The policy table mirrors the Spec Row Index in `docs/snarkpack/ripp-spec.md`.
//! The invariant gate checks that this table and the spec stay synchronized.

extern crate alloc;

use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceComparisonLevel {
    PenumbraByte,
    AbstractTrace,
    FilecoinShape,
    PenumbraLocal,
}

impl TraceComparisonLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PenumbraByte => "penumbra-byte",
            Self::AbstractTrace => "abstract-trace",
            Self::FilecoinShape => "filecoin-shape",
            Self::PenumbraLocal => "penumbra-local",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceEventKind {
    ChallengeContext,
    ChallengePreimage,
    ChallengeDigest,
    EquationRole,
    ObjectRole,
    FilecoinBugClass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilecoinBugClass {
    FirstRoundHashOmission,
    FinalRandomnessOmission,
    PublicMessageReordering,
    HiddenDefaultContext,
    ProverVerifierChallengeMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TracePolicy {
    pub spec_row_id: &'static str,
    pub primary_level: TraceComparisonLevel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    pub spec_row_id: &'static str,
    pub primary_level: TraceComparisonLevel,
    pub event_kind: TraceEventKind,
    pub stage_label: &'static str,
    pub nonce: Option<u64>,
    pub round_index: Option<u32>,
    pub byte_payload: Vec<u8>,
    pub abstract_payload: Option<&'static str>,
    pub filecoin_bug_class: Option<FilecoinBugClass>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceEventError {
    MissingBytePayload,
    MissingAbstractPayload,
    MissingFilecoinBugClass,
}

impl TraceEvent {
    pub fn validate(&self) -> Result<(), TraceEventError> {
        match self.primary_level {
            TraceComparisonLevel::PenumbraByte if self.byte_payload.is_empty() => {
                Err(TraceEventError::MissingBytePayload)
            }
            TraceComparisonLevel::AbstractTrace if self.abstract_payload.is_none() => {
                Err(TraceEventError::MissingAbstractPayload)
            }
            TraceComparisonLevel::FilecoinShape if self.filecoin_bug_class.is_none() => {
                Err(TraceEventError::MissingFilecoinBugClass)
            }
            _ => Ok(()),
        }
    }
}

pub const TRACE_POLICIES: &[TracePolicy] = &[
    TracePolicy {
        spec_row_id: "fs.context-constructor",
        primary_level: TraceComparisonLevel::PenumbraByte,
    },
    TracePolicy {
        spec_row_id: "fs.challenge-preimage",
        primary_level: TraceComparisonLevel::PenumbraByte,
    },
    TracePolicy {
        spec_row_id: "fs.stage-labels",
        primary_level: TraceComparisonLevel::PenumbraByte,
    },
    TracePolicy {
        spec_row_id: "fs.filecoin-bug-class",
        primary_level: TraceComparisonLevel::FilecoinShape,
    },
    TracePolicy {
        spec_row_id: "gipa.input-relation",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "gipa.round-folding",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "gipa.challenge-dependency",
        primary_level: TraceComparisonLevel::PenumbraByte,
    },
    TracePolicy {
        spec_row_id: "gipa.verifier-folding",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "tipa.srs",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "tipa.ab.gipa",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "tipa.ab.kzg-challenge",
        primary_level: TraceComparisonLevel::PenumbraByte,
    },
    TracePolicy {
        spec_row_id: "tipa.ab.kzg-equations",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "ssm.power-sequence",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "ssm.kzg-challenge",
        primary_level: TraceComparisonLevel::PenumbraByte,
    },
    TracePolicy {
        spec_row_id: "ssm.base-equation",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "groth16.randomizer",
        primary_level: TraceComparisonLevel::PenumbraByte,
    },
    TracePolicy {
        spec_row_id: "groth16.folded-inputs",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
    TracePolicy {
        spec_row_id: "groth16.ppe",
        primary_level: TraceComparisonLevel::AbstractTrace,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn penumbra_byte_events_require_bytes() {
        let event = TraceEvent {
            spec_row_id: "fs.challenge-preimage",
            primary_level: TraceComparisonLevel::PenumbraByte,
            event_kind: TraceEventKind::ChallengePreimage,
            stage_label: "aggregate.randomizer",
            nonce: Some(0),
            round_index: None,
            byte_payload: Vec::new(),
            abstract_payload: None,
            filecoin_bug_class: None,
        };
        assert_eq!(event.validate(), Err(TraceEventError::MissingBytePayload));
    }

    #[test]
    fn filecoin_shape_events_require_bug_class() {
        let event = TraceEvent {
            spec_row_id: "fs.filecoin-bug-class",
            primary_level: TraceComparisonLevel::FilecoinShape,
            event_kind: TraceEventKind::FilecoinBugClass,
            stage_label: "filecoin.v2",
            nonce: None,
            round_index: None,
            byte_payload: Vec::new(),
            abstract_payload: None,
            filecoin_bug_class: None,
        };
        assert_eq!(
            event.validate(),
            Err(TraceEventError::MissingFilecoinBugClass)
        );
    }

    #[test]
    fn abstract_trace_events_require_abstract_payload() {
        let event = TraceEvent {
            spec_row_id: "groth16.ppe",
            primary_level: TraceComparisonLevel::AbstractTrace,
            event_kind: TraceEventKind::EquationRole,
            stage_label: "groth16.ppe",
            nonce: None,
            round_index: None,
            byte_payload: Vec::new(),
            abstract_payload: None,
            filecoin_bug_class: None,
        };
        assert_eq!(
            event.validate(),
            Err(TraceEventError::MissingAbstractPayload)
        );
    }
}
