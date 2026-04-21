use std::fmt::Display;

use anyhow::Context;
use anyhow::Result;
use penumbra_sdk_asset::BASE_ASSET_ID;
use penumbra_sdk_fee::FeeParameters;
use penumbra_sdk_governance::change::ParameterChange;
use penumbra_sdk_governance::{params::GovernanceParameters, tally::Ratio};
use penumbra_sdk_ibc::params::IBCParameters;
use penumbra_sdk_sct::params::SctParameters;
use penumbra_sdk_shielded_pool::params::ShieldedPoolParameters;
use penumbra_sdk_validator::params::ValidatorParameters;

use super::AppParameters;

pub trait ParameterChangeExt {
    fn apply_changes(&self, app_parameters: AppParameters) -> Result<AppParameters, anyhow::Error>;
}

impl ParameterChangeExt for ParameterChange {
    fn apply_changes(&self, app_parameters: AppParameters) -> Result<AppParameters, anyhow::Error> {
        let app_parameters_raw = serde_json::value::to_value(app_parameters.clone())
            .context("could not encode app parameters to json value")?;
        let new_app_parameters_raw = self
            .apply_changes_raw(app_parameters_raw)
            .context("error applying parameter changes")?;

        let new_app_parameters = serde_json::value::from_value(new_app_parameters_raw)
            .context("error parsing changed app parameters")?;

        // TODO: validation should be done in the components themselves,
        // in the domain type conversions (#3593).
        // Rather than checking against old parameters we should use a denylist
        // of fields that can't be changed.
        app_parameters
            .check_valid_update(&new_app_parameters)
            .context("parameter change was invalid")?;

        Ok(new_app_parameters)
    }
}

// The checks below validate that a parameter change is valid, since some parameter settings or
// combinations are nonsensical and should be rejected outright, regardless of governance.

#[deny(unused)] // We want to be really careful here to not examine fields!
impl AppParameters {
    pub fn check_valid_update(&self, new: &AppParameters) -> Result<()> {
        new.check_valid()?;
        // TODO: move the checks below into their respective components.
        // Tracked by #3593
        let AppParameters {
            chain_id,
            fee_params:
                FeeParameters {
                    fixed_gas_prices: _,
                    fixed_alt_gas_prices: _,
                },
            governance_params:
                GovernanceParameters {
                    proposal_voting_blocks: _,
                    proposal_valid_quorum,
                    proposal_pass_threshold,
                    proposal_slash_threshold,
                },
            ibc_params:
                IBCParameters {
                    ibc_enabled: _,
                    inbound_ics20_transfers_enabled: _,
                    outbound_ics20_transfers_enabled: _,
                },
            sct_params: SctParameters { epoch_duration },
            shielded_pool_params: ShieldedPoolParameters { fmd_meta_params: _ },
            validator_params:
                ValidatorParameters {
                    active_validator_limit,
                    signed_blocks_window_len,
                    missed_blocks_maximum: _,
                },
            // IMPORTANT: Don't use `..` here! We want to ensure every single field is verified!
        } = self;

        // Ensure that certain parameters are not changed by the update:
        check_invariant([(chain_id, &new.chain_id, "chain ID")])?;
        check_invariant([
            (
                epoch_duration,
                &new.sct_params.epoch_duration,
                "epoch duration",
            ),
            (
                active_validator_limit,
                &new.validator_params.active_validator_limit,
                "active validator limit",
            ),
            (
                signed_blocks_window_len,
                &new.validator_params.signed_blocks_window_len,
                "signed blocks window length",
            ),
        ])?;
        check_invariant([
            (
                proposal_valid_quorum,
                &new.governance_params.proposal_valid_quorum,
                "proposal valid quorum",
            ),
            (
                proposal_pass_threshold,
                &new.governance_params.proposal_pass_threshold,
                "proposal pass threshold",
            ),
            (
                proposal_slash_threshold,
                &new.governance_params.proposal_slash_threshold,
                "proposal slash threshold",
            ),
        ])?;

        Ok(())
    }

    pub fn check_valid(&self) -> Result<()> {
        let AppParameters {
            chain_id,
            fee_params:
                FeeParameters {
                    fixed_gas_prices,
                    fixed_alt_gas_prices,
                },
            governance_params:
                GovernanceParameters {
                    proposal_voting_blocks,
                    proposal_valid_quorum,
                    proposal_pass_threshold,
                    proposal_slash_threshold,
                },
            ibc_params:
                IBCParameters {
                    ibc_enabled,
                    inbound_ics20_transfers_enabled,
                    outbound_ics20_transfers_enabled,
                },
            sct_params: SctParameters { epoch_duration },
            shielded_pool_params: ShieldedPoolParameters { fmd_meta_params: _ },
            validator_params:
                ValidatorParameters {
                    active_validator_limit,
                    signed_blocks_window_len,
                    missed_blocks_maximum,
                },
            // IMPORTANT: Don't use `..` here! We want to ensure every single field is verified!
        } = self;

        check_all([
            (!chain_id.is_empty(), "chain ID must be a non-empty string"),
            (
                *epoch_duration >= 1,
                "epoch duration must be at least one block",
            ),
            (
                *active_validator_limit > 3,
                "active validator limit must be at least 4",
            ),
            (
                *signed_blocks_window_len >= 2,
                "signed blocks window length must be at least 2",
            ),
            (
                *missed_blocks_maximum >= 1,
                "missed blocks maximum must be at least 1",
            ),
            (
                (!*inbound_ics20_transfers_enabled && !*outbound_ics20_transfers_enabled)
                    || *ibc_enabled,
                "IBC must be enabled if either inbound or outbound ICS20 transfers are enabled",
            ),
            (
                *proposal_voting_blocks >= 1,
                "proposal voting blocks must be at least 1",
            ),
            (
                *proposal_valid_quorum > Ratio::new(0, 1),
                "proposal valid quorum must be greater than 0",
            ),
            (
                *proposal_pass_threshold >= Ratio::new(1, 2),
                "proposal pass threshold must be greater than or equal to 1/2",
            ),
            (
                *proposal_slash_threshold > Ratio::new(1, 2),
                "proposal slash threshold must be greater than 1/2",
            ),
            (
                fixed_gas_prices.asset_id == *BASE_ASSET_ID,
                "fee gas prices must use the base asset",
            ),
            (
                fixed_alt_gas_prices.is_empty(),
                "alternate gas-price configuration is not supported on the reduced chain",
            ),
        ])
    }
}

/// Ensure all of the booleans are true, and if any are false, generate an error describing which
/// failed, based on the provided descriptions.
fn check_all<'a>(checks: impl IntoIterator<Item = (bool, impl Display + 'a)>) -> Result<()> {
    let failed_because = checks
        .into_iter()
        .filter_map(|(ok, description)| {
            if !ok {
                Some(description.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if !failed_because.is_empty() {
        anyhow::bail!("invalid chain parameters: {}", failed_because.join(", "));
    }

    Ok(())
}

/// Ensure that all of the provided pairs of values are equal, and if any are not, generate an error
/// stating that the varying names can't be changed.
fn check_invariant<'a, T: Eq + 'a>(
    sides: impl IntoIterator<Item = (&'a T, &'a T, impl Display + 'a)>,
) -> Result<()> {
    check_all(
        sides
            .into_iter()
            .map(|(old, new, name)| ((*old == *new), format!("{name} can't be changed"))),
    )
}
