use serde::{Deserialize, Serialize};

use penumbra_sdk_proto::{penumbra::core::component::governance::v1 as pb, DomainType};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "pb::ProposalState", into = "pb::ProposalState")]
pub enum State {
    Voting,
    Finished { outcome: Outcome },
}

impl State {
    pub fn is_voting(&self) -> bool {
        matches!(self, State::Voting)
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, State::Finished { .. })
    }

    pub fn is_passed(&self) -> bool {
        matches!(
            self,
            State::Finished {
                outcome: Outcome::Passed
            }
        )
    }

    pub fn is_failed(&self) -> bool {
        matches!(
            self,
            State::Finished {
                outcome: Outcome::Failed
            }
        )
    }

    pub fn is_slashed(&self) -> bool {
        matches!(
            self,
            State::Finished {
                outcome: Outcome::Slashed
            }
        )
    }
}

impl DomainType for State {
    type Proto = pb::ProposalState;
}

impl From<State> for pb::ProposalState {
    fn from(state: State) -> Self {
        let state = match state {
            State::Voting => pb::proposal_state::State::Voting(pb::proposal_state::Voting {}),
            State::Finished { outcome } => {
                pb::proposal_state::State::Finished(pb::proposal_state::Finished {
                    outcome: Some(outcome.into()),
                })
            }
        };
        pb::ProposalState { state: Some(state) }
    }
}

impl TryFrom<pb::ProposalState> for State {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ProposalState) -> Result<Self, Self::Error> {
        Ok(
            match msg
                .state
                .ok_or_else(|| anyhow::anyhow!("missing proposal state"))?
            {
                pb::proposal_state::State::Voting(pb::proposal_state::Voting {}) => State::Voting,
                pb::proposal_state::State::Finished(pb::proposal_state::Finished { outcome }) => {
                    State::Finished {
                        outcome: outcome
                            .ok_or_else(|| anyhow::anyhow!("missing proposal outcome"))?
                            .try_into()?,
                    }
                }
            },
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "pb::ProposalOutcome", into = "pb::ProposalOutcome")]
pub enum Outcome {
    Passed,
    Failed,
    Slashed,
}

impl Outcome {
    pub fn should_be_refunded(&self) -> bool {
        !self.is_slashed()
    }

    pub fn is_slashed(&self) -> bool {
        matches!(self, Outcome::Slashed)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Outcome::Failed | Outcome::Slashed)
    }

    pub fn is_passed(&self) -> bool {
        matches!(self, Outcome::Passed)
    }
}

impl DomainType for Outcome {
    type Proto = pb::ProposalOutcome;
}

impl From<Outcome> for pb::ProposalOutcome {
    fn from(outcome: Outcome) -> Self {
        let outcome = match outcome {
            Outcome::Passed => {
                pb::proposal_outcome::Outcome::Passed(pb::proposal_outcome::Passed {})
            }
            Outcome::Failed => {
                pb::proposal_outcome::Outcome::Failed(pb::proposal_outcome::Failed {})
            }
            Outcome::Slashed => {
                pb::proposal_outcome::Outcome::Slashed(pb::proposal_outcome::Slashed {})
            }
        };
        pb::ProposalOutcome {
            outcome: Some(outcome),
        }
    }
}

impl TryFrom<pb::ProposalOutcome> for Outcome {
    type Error = anyhow::Error;

    fn try_from(msg: pb::ProposalOutcome) -> Result<Self, Self::Error> {
        Ok(
            match msg
                .outcome
                .ok_or_else(|| anyhow::anyhow!("missing proposal outcome"))?
            {
                pb::proposal_outcome::Outcome::Passed(pb::proposal_outcome::Passed {}) => {
                    Outcome::Passed
                }
                pb::proposal_outcome::Outcome::Failed(pb::proposal_outcome::Failed {}) => {
                    Outcome::Failed
                }
                pb::proposal_outcome::Outcome::Slashed(pb::proposal_outcome::Slashed {}) => {
                    Outcome::Slashed
                }
            },
        )
    }
}
