use std::sync::Arc;

use anyhow::bail;
use anyhow::Result;
use async_trait::async_trait;
use cnidarium::{StateRead, StateWrite};
use penumbra_sdk_shielded_pool::component::Ics20Transfer;
use penumbra_sdk_transaction::Action;
use penumbra_sdk_txhash::TransactionContext;

mod submit;

use crate::PenumbraHost;

use super::AppActionHandler;
use cnidarium_component::ActionHandler as _;

#[async_trait]
impl AppActionHandler for Action {
    type CheckStatelessContext = TransactionContext;

    async fn check_stateless(&self, context: TransactionContext) -> Result<()> {
        match self {
            Action::Transfer(action) => action.check_stateless(context).await,
            Action::Consolidate(action) => action.check_stateless(context).await,
            Action::Split(action) => action.check_stateless(context).await,
            Action::ValidatorDefinition(action) => action.check_stateless(()).await,
            Action::ValidatorVote(action) => action.check_stateless(()).await,
            Action::ProposalSubmit(action) => action.check_stateless(()).await,
            Action::IbcRelay(action) => {
                action
                    .clone()
                    .with_handler::<Ics20Transfer, PenumbraHost>()
                    .check_stateless(())
                    .await
            }
            Action::ShieldedIcs20Withdrawal(action) => action.check_stateless(context).await,
            Action::ComplianceRegisterAsset(action) => action.check_stateless(()).await,
            Action::ComplianceRegisterUser(action) => action.check_stateless(()).await,
            Action::AggregateBundle(_) => bail!(
                "aggregate bundle actions are only permitted in the dedicated aggregation pipeline"
            ),
        }
    }

    async fn check_historical<S: StateRead + 'static>(&self, state: Arc<S>) -> Result<()> {
        match self {
            Action::Transfer(action) => action.check_historical(state).await,
            Action::Consolidate(_) => Ok(()),
            Action::Split(_) => Ok(()),
            Action::ValidatorDefinition(action) => action.check_historical(state).await,
            Action::ValidatorVote(action) => action.check_historical(state).await,
            Action::ProposalSubmit(action) => action.check_historical(state).await,
            Action::IbcRelay(action) => {
                action
                    .clone()
                    .with_handler::<Ics20Transfer, PenumbraHost>()
                    .check_historical(state)
                    .await
            }
            Action::ShieldedIcs20Withdrawal(action) => action.check_historical(state).await,
            Action::ComplianceRegisterAsset(action) => action.check_historical(state).await,
            Action::ComplianceRegisterUser(action) => action.check_historical(state).await,
            Action::AggregateBundle(_) => bail!(
                "aggregate bundle actions are only permitted in the dedicated aggregation pipeline"
            ),
        }
    }

    async fn check_and_execute<S: StateWrite>(&self, state: S) -> Result<()> {
        match self {
            Action::Transfer(action) => action.check_and_execute(state).await,
            Action::Consolidate(action) => action.check_and_execute(state).await,
            Action::Split(action) => action.check_and_execute(state).await,
            Action::ValidatorDefinition(action) => action.check_and_execute(state).await,
            Action::ValidatorVote(action) => action.check_and_execute(state).await,
            Action::ProposalSubmit(action) => action.check_and_execute(state).await,
            Action::IbcRelay(action) => {
                action
                    .clone()
                    .with_handler::<Ics20Transfer, PenumbraHost>()
                    .check_and_execute(state)
                    .await
            }
            Action::ShieldedIcs20Withdrawal(action) => action.check_and_execute(state).await,
            Action::ComplianceRegisterAsset(action) => action.check_and_execute(state).await,
            Action::ComplianceRegisterUser(action) => action.check_and_execute(state).await,
            Action::AggregateBundle(_) => bail!(
                "aggregate bundle actions are only permitted in the dedicated aggregation pipeline"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cnidarium::{StateDelta, TempStorage};
    use penumbra_sdk_proof_aggregation::AggregateBundle;
    use penumbra_sdk_tct as tct;
    use penumbra_sdk_transaction::Action;
    use penumbra_sdk_txhash::{EffectHash, TransactionContext};

    use super::AppActionHandler;
    use crate::SUBSTORE_PREFIXES;

    fn aggregate_bundle_action() -> Action {
        Action::AggregateBundle(AggregateBundle {
            version: 1,
            srs_id: vec![],
            families: vec![],
        })
    }

    #[tokio::test]
    async fn aggregate_bundle_is_rejected_by_generic_stateless_handling() {
        let action = aggregate_bundle_action();
        let context = TransactionContext {
            anchor: tct::Tree::new().root(),
            effect_hash: EffectHash::default(),
        };

        let error = action
            .check_stateless(context)
            .await
            .expect_err("aggregate bundle must be rejected by generic stateless handling");

        assert!(error.to_string().contains(
            "aggregate bundle actions are only permitted in the dedicated aggregation pipeline"
        ));
    }

    #[tokio::test]
    async fn aggregate_bundle_is_rejected_by_generic_historical_handling() {
        let action = aggregate_bundle_action();
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec())
            .await
            .expect("temp storage");
        let snapshot = Arc::new(storage.latest_snapshot());

        let error = action
            .check_historical(snapshot)
            .await
            .expect_err("aggregate bundle must be rejected by generic historical handling");

        assert!(error.to_string().contains(
            "aggregate bundle actions are only permitted in the dedicated aggregation pipeline"
        ));
    }

    #[tokio::test]
    async fn aggregate_bundle_is_rejected_by_generic_execution_handling() {
        let action = aggregate_bundle_action();
        let storage = TempStorage::new_with_prefixes(SUBSTORE_PREFIXES.to_vec())
            .await
            .expect("temp storage");
        let state = StateDelta::new(storage.latest_snapshot());

        let error = action
            .check_and_execute(state)
            .await
            .expect_err("aggregate bundle must be rejected by generic execution handling");

        assert!(error.to_string().contains(
            "aggregate bundle actions are only permitted in the dedicated aggregation pipeline"
        ));
    }
}
