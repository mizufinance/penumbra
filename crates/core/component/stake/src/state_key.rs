pub mod parameters {
    pub fn key() -> &'static str {
        "staking/parameters"
    }
}

pub mod validators {
    pub mod consensus_set_index {
        pub fn prefix() -> &'static str {
            "staking/validators/consensus_set_index/"
        }
        pub fn by_id(id: &crate::IdentityKey) -> String {
            format!("{}{id}", prefix())
        }
    }

    pub mod lookup_by {
        use tendermint::PublicKey;

        pub fn consensus_key(pk: &PublicKey) -> String {
            format!("staking/validators/lookup_by/consensus_key/{}", pk.to_hex())
        }

        pub fn cometbft_address(address: &[u8; 20]) -> String {
            format!(
                "staking/validators/lookup_by/cometbft_address/{}",
                hex::encode(address)
            )
        }
    }

    pub mod definitions {
        pub fn prefix() -> &'static str {
            "staking/validators/definitions/"
        }
        pub fn by_id(id: &crate::IdentityKey) -> String {
            format!("{}{id}", prefix())
        }
    }

    pub mod state {
        pub fn by_id(id: &crate::IdentityKey) -> String {
            format!("staking/validators/data/state/{id}")
        }
    }

    pub mod power {
        pub fn by_id(id: &crate::IdentityKey) -> String {
            format!("staking/validators/data/power/{id}")
        }
    }

    pub mod uptime {
        pub fn by_id(id: &crate::IdentityKey) -> String {
            format!("staking/validators/data/uptime/{id}")
        }
    }

    pub mod last_disabled {
        pub fn by_id(id: &crate::IdentityKey) -> String {
            format!("staking/validators/data/last_disabled/{id}")
        }
    }
}

pub mod consensus_update {
    pub fn consensus_keys() -> &'static str {
        "staking/cometbft_data/consensus_keys"
    }
}

pub(super) mod internal {

    pub fn cometbft_validator_updates() -> &'static str {
        "staking/cometbft_validator_updates"
    }
}
