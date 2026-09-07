pub mod denom_metadata_by_asset {
    use shieldd_sdk_asset::asset;
    use std::string::String;

    pub fn prefix() -> &'static str {
        "shielded_pool/assets/"
    }

    pub fn by_asset_id(asset_id: &asset::Id) -> String {
        format!("shielded_pool/assets/{asset_id}/denom")
    }
}

// State keys used to temporarily store payloads and nullifiers to be inserted into the compact
// block
pub fn pending_notes() -> &'static str {
    "shielded_pool/pending_notes"
}

pub fn pending_rolled_up_payloads() -> &'static str {
    "shielded_pool/pending_rolled_up_payloads"
}

pub fn pending_volume_accumulator_payloads() -> &'static str {
    "shielded_pool/pending_volume_accumulator_payloads"
}

pub mod volume_nullifiers {
    use shieldd_sdk_sct::Nullifier;

    pub fn prefix() -> &'static str {
        "shielded_pool/volume_nullifiers/entries/"
    }

    pub fn day_prefix(day_start: u64) -> String {
        format!("{}{day_start:020}/", prefix())
    }

    pub fn by_day_and_nullifier(day_start: u64, nullifier: Nullifier) -> String {
        format!(
            "{}{}",
            day_prefix(day_start),
            hex::encode(nullifier.to_bytes())
        )
    }

    pub fn day_marker_prefix() -> &'static str {
        "shielded_pool/volume_nullifiers/days/"
    }

    pub fn day_marker(day_start: u64) -> String {
        format!("{}{day_start:020}", day_marker_prefix())
    }
}

pub fn shielded_pool_params() -> &'static str {
    "shielded_pool/params"
}

pub fn host_withdrawals_enabled() -> &'static str {
    "shielded_pool/host_withdrawals_enabled"
}
