use anyhow::Result;

use shieldd_sdk_custody::{AuthorizeRequest, CustodyClient};
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_transaction::{AuthorizationData, Transaction, TransactionPlan};
#[tonic::async_trait]
pub trait WitnessSource {
    async fn witness(
        &mut self,
        plan: &TransactionPlan,
    ) -> Result<shieldd_sdk_transaction::WitnessData>;
}

#[tonic::async_trait]
impl WitnessSource for shieldd_sdk_view::Storage {
    async fn witness(
        &mut self,
        plan: &TransactionPlan,
    ) -> Result<shieldd_sdk_transaction::WitnessData> {
        self.witness_plan(plan).await
    }
}

pub async fn build_transaction<V, C>(
    fvk: &FullViewingKey,
    view: &mut V,
    custody: &mut C,
    plan: TransactionPlan,
) -> Result<Transaction>
where
    V: WitnessSource,
    C: CustodyClient,
{
    // Get the authorization data from the custody service...
    let auth_data: AuthorizationData = custody
        .authorize(AuthorizeRequest {
            plan: plan.clone(),
            pre_authorizations: Vec::new(),
        })
        .await?
        .data
        .ok_or_else(|| anyhow::anyhow!("empty AuthorizeResponse message"))?
        .try_into()?;

    // Send a witness request to the view service to get witness data
    let witness_data = view.witness(&plan).await?;

    // ... and then build the transaction:
    #[cfg(not(feature = "parallel"))]
    {
        let tx = plan.build(fvk, &witness_data, &auth_data)?;
        return Ok(tx);
    }

    #[cfg(feature = "parallel")]
    {
        let tx = plan
            .build_concurrent(fvk, &witness_data, &auth_data)
            .await
            .map_err(|e| {
                tonic::Status::failed_precondition(format!("Error building transaction: {}", e))
            })?;

        Ok(tx)
    }
}
