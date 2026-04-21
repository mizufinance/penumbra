use penumbra_sdk_sct::Nullifier;
use penumbra_sdk_validator::IdentityKey;

pub fn governance_params() -> &'static str {
    "governance/params"
}

pub fn next_proposal_id() -> &'static str {
    "governance/next_proposal_id"
}

pub fn proposal_definition(proposal_id: u64) -> String {
    format!("governance/proposal/{proposal_id:020}/data")
}

pub fn proposal_state(proposal_id: u64) -> String {
    format!("governance/proposal/{proposal_id:020}/state")
}

pub fn proposal_submitter(proposal_id: u64) -> String {
    format!("governance/proposal/{proposal_id:020}/submitter")
}

pub fn proposal_voting_start(proposal_id: u64) -> String {
    format!("governance/proposal/{proposal_id:020}/voting_start")
}

pub fn proposal_voting_start_position(proposal_id: u64) -> String {
    format!("governance/proposal/{proposal_id:020}/voting_start_position")
}

pub fn proposal_voting_end(proposal_id: u64) -> String {
    format!("governance/proposal/{proposal_id:020}/voting_end")
}

pub fn unfinished_proposal(proposal_id: u64) -> String {
    format!("governance/unfinished_proposals/{proposal_id:020}")
}

pub fn all_unfinished_proposals() -> &'static str {
    // Note: this has to be the prefix of the `unfinished_proposal` function above.
    "governance/unfinished_proposals/"
}

pub fn voted_nullifier_lookup_for_proposal(proposal_id: u64, nullifier: &Nullifier) -> String {
    format!("governance/proposal/{proposal_id:020}/voted_nullifiers/{nullifier}")
}

pub fn voting_power_at_proposal_start(proposal_id: u64, identity_key: IdentityKey) -> String {
    format!("governance/proposal/{proposal_id:020}/voting_power_at_start/{identity_key}")
}

pub fn all_voting_power_at_proposal_start(proposal_id: u64) -> String {
    // Note: this has to be the prefix of the `voting_power_at_proposal_start` function above.
    format!("governance/proposal/{proposal_id:020}/voting_power_at_start/")
}

pub fn validator_vote(proposal_id: u64, identity_key: IdentityKey) -> String {
    format!("governance/validator_vote/{proposal_id:020}/{identity_key}")
}

pub fn validator_vote_reason(proposal_id: u64, identity_key: IdentityKey) -> String {
    format!("governance/validator_vote_reason/{proposal_id:020}/{identity_key}")
}

pub fn all_validator_votes_for_proposal(proposal_id: u64) -> String {
    // Note: this has to be the prefix of the `validator_vote` function above.
    format!("governance/validator_vote/{proposal_id:020}/")
}

pub fn param_changes_for_height(block_height: u64) -> String {
    format!("governance/param_changes/{block_height}/")
}

pub fn proposal_started() -> &'static str {
    "governance/proposal_started"
}

pub mod upgrades {
    pub fn next_upgrade() -> &'static str {
        "governance/upgrades/next_upgrade"
    }
}

pub mod persistent_flags {
    pub fn halt_bit() -> &'static str {
        "governance/persistent_flags/halt_bit"
    }
}
