use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt::{self, Display, Formatter},
    ops::{Add, AddAssign},
    str::FromStr,
};

use penumbra_sdk_proto::{penumbra::core::component::governance::v1 as pb, DomainType};

use crate::{params::GovernanceParameters, proposal_state::Outcome as StateOutcome, vote::Vote};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(try_from = "pb::Tally", into = "pb::Tally")]
pub struct Tally {
    yes: u64,
    no: u64,
    abstain: u64,
}

impl Tally {
    pub fn yes(&self) -> u64 {
        self.yes
    }

    pub fn no(&self) -> u64 {
        self.no
    }

    pub fn abstain(&self) -> u64 {
        self.abstain
    }

    pub fn total(&self) -> u64 {
        self.yes + self.no + self.abstain
    }
}

impl From<Tally> for pb::Tally {
    fn from(tally: Tally) -> Self {
        Self {
            yes: tally.yes,
            no: tally.no,
            abstain: tally.abstain,
        }
    }
}

impl From<pb::Tally> for Tally {
    fn from(tally: pb::Tally) -> Self {
        Self {
            yes: tally.yes,
            no: tally.no,
            abstain: tally.abstain,
        }
    }
}

impl DomainType for Tally {
    type Proto = pb::Tally;
}

impl From<(Vote, u64)> for Tally {
    fn from((vote, power): (Vote, u64)) -> Self {
        let mut tally = Self::default();
        *match vote {
            Vote::Yes => &mut tally.yes,
            Vote::No => &mut tally.no,
            Vote::Abstain => &mut tally.abstain,
        } = power;
        tally
    }
}

impl From<(u64, Vote)> for Tally {
    fn from((power, vote): (u64, Vote)) -> Self {
        Self::from((vote, power))
    }
}

impl Add for Tally {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            yes: self.yes + rhs.yes,
            no: self.no + rhs.no,
            abstain: self.abstain + rhs.abstain,
        }
    }
}

impl AddAssign for Tally {
    fn add_assign(&mut self, rhs: Self) {
        self.yes += rhs.yes;
        self.no += rhs.no;
        self.abstain += rhs.abstain;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Slash,
}

impl Outcome {
    pub fn is_pass(&self) -> bool {
        matches!(self, Self::Pass)
    }

    pub fn is_fail(&self) -> bool {
        matches!(self, Self::Fail)
    }

    pub fn is_slash(&self) -> bool {
        matches!(self, Self::Slash)
    }
}

impl From<Outcome> for StateOutcome {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Pass => Self::Passed,
            Outcome::Fail => Self::Failed,
            Outcome::Slash => Self::Slashed,
        }
    }
}

impl Tally {
    fn meets_quorum(&self, total_voting_power: u64, params: &GovernanceParameters) -> bool {
        Ratio::new(self.total(), total_voting_power) >= params.proposal_valid_quorum
    }

    fn slashed(&self, params: &GovernanceParameters) -> bool {
        Ratio::new(self.no, self.total()) > params.proposal_slash_threshold
    }

    fn yes_ratio(&self) -> Ratio {
        let non_abstain_votes = self.yes + self.no;
        Ratio::new(self.yes, non_abstain_votes.max(1))
        // ^ when the only votes cast are abstains, treat the yes ratio as 0/1 to avoid
        // division-by-zero while still ensuring the proposal cannot pass.
    }

    pub fn outcome(self, total_voting_power: u64, params: &GovernanceParameters) -> Outcome {
        use Outcome::*;

        // Check to see if we've met quorum
        if !self.meets_quorum(total_voting_power, params) {
            return Fail;
        }

        // Check to see if it has been slashed
        if self.slashed(params) {
            return Slash;
        }

        // Now that we've checked for slash and quorum, we can just check to see if it should pass
        if self.yes_ratio() > params.proposal_pass_threshold {
            Pass
        } else {
            Fail
        }
    }

    pub fn emergency_pass(self, total_voting_power: u64, params: &GovernanceParameters) -> bool {
        // Check to see if we've met quorum
        if !self.meets_quorum(total_voting_power, params) {
            return false;
        }

        // Check to see if it has been slashed (this check should be redundant, but we'll do it anyway)
        if self.slashed(params) {
            return false;
        }

        // Now that we've checked for slash and quorum, we can just check to see if it should pass in
        // the emergency condition of 1/3 majority of voting power
        Ratio::new(self.yes, total_voting_power) > Ratio::new(1, 3)
    }
}

/// This is a ratio of two `u64` values, intended to be used solely in governance parameters and
/// tallying. It only implements construction and comparison, not arithmetic, to reduce the trusted
/// codebase for governance.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "pb::Ratio", into = "pb::Ratio")]
pub struct Ratio {
    numerator: u64,
    denominator: u64,
}

impl Display for Ratio {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

impl FromStr for Ratio {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('/');
        let numerator = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing numerator"))?
            .parse()?;
        let denominator = parts
            .next()
            .ok_or_else(|| anyhow::anyhow!("missing denominator"))?
            .parse()?;
        if parts.next().is_some() {
            anyhow::bail!("too many parts");
        }
        Ok(Ratio {
            numerator,
            denominator,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Outcome, Ratio, Tally};
    use crate::{params::GovernanceParameters, vote::Vote};

    fn tally(votes: &[(Vote, u64)]) -> Tally {
        votes
            .iter()
            .copied()
            .map(Tally::from)
            .fold(Tally::default(), |acc, tally| acc + tally)
    }

    #[test]
    fn mixed_yes_and_no_below_threshold_fails() {
        let params = GovernanceParameters::default();
        let tally = tally(&[(Vote::Yes, 50), (Vote::No, 50)]);

        assert_eq!(tally.outcome(100, &params), Outcome::Fail);
    }

    #[test]
    fn mixed_yes_and_no_above_threshold_passes() {
        let params = GovernanceParameters::default();
        let tally = tally(&[(Vote::Yes, 51), (Vote::No, 49)]);

        assert_eq!(tally.outcome(100, &params), Outcome::Pass);
    }

    #[test]
    fn abstain_only_tally_never_passes() {
        let params = GovernanceParameters::default();
        let tally = tally(&[(Vote::Abstain, 100)]);

        assert_eq!(tally.outcome(100, &params), Outcome::Fail);
    }

    #[test]
    fn slash_threshold_is_checked_before_pass_threshold() {
        let mut params = GovernanceParameters::default();
        params.proposal_slash_threshold = Ratio::new(3, 5);
        let tally = tally(&[(Vote::Yes, 20), (Vote::No, 70), (Vote::Abstain, 10)]);

        assert_eq!(tally.outcome(100, &params), Outcome::Slash);
    }
}

impl Ratio {
    pub fn new(numerator: u64, denominator: u64) -> Self {
        Self {
            numerator,
            denominator,
        }
    }
}

impl PartialEq for Ratio {
    fn eq(&self, other: &Self) -> bool {
        // Convert everything to `u128` to avoid overflow when multiplying
        u128::from(self.numerator) * u128::from(other.denominator)
            == u128::from(self.denominator) * u128::from(other.numerator)
    }
}

impl Eq for Ratio {}

impl PartialOrd for Ratio {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ratio {
    fn cmp(&self, other: &Self) -> Ordering {
        // Convert everything to `u128` to avoid overflow when multiplying
        (u128::from(self.numerator) * u128::from(other.denominator))
            .cmp(&(u128::from(self.denominator) * u128::from(other.numerator)))
    }
}

impl From<Ratio> for pb::Ratio {
    fn from(ratio: Ratio) -> Self {
        pb::Ratio {
            numerator: ratio.numerator,
            denominator: ratio.denominator,
        }
    }
}

impl From<pb::Ratio> for Ratio {
    fn from(msg: pb::Ratio) -> Self {
        Ratio {
            numerator: msg.numerator,
            denominator: msg.denominator,
        }
    }
}
