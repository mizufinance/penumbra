pub mod app_version {
    pub fn safeguard() -> &'static str {
        "application/version/safeguard"
    }
}

pub mod genesis {
    pub fn app_state() -> &'static str {
        "application/genesis/app_state"
    }
}

pub mod data {
    pub fn chain_id() -> &'static str {
        "application/data/chain_id"
    }
}

pub mod block_data {
    pub fn transactions_by_height(block_height: u64) -> String {
        format!(
            "{}/transactions_by_height/{block_height:020}",
            crate::COMETBFT_SUBSTORE_PREFIX
        )
    }
}
