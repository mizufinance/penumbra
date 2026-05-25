use anyhow::Result;
use rand::{CryptoRng, RngCore};

use penumbra_sdk_keys::keys::SpendKey;

use crate::{AuthorizationData, TransactionPlan};

impl TransactionPlan {
    /// Authorize this [`TransactionPlan`] with the provided [`SpendKey`].
    pub fn authorize<R: RngCore + CryptoRng>(
        &self,
        mut rng: R,
        sk: &SpendKey,
    ) -> Result<AuthorizationData> {
        let effect_hash = self.effect_hash(sk.full_viewing_key())?;
        let mut spend_auths = Vec::new();

        for action_plan in &self.actions {
            match action_plan {
                crate::ActionPlan::Transfer(plan) => {
                    for spend_plan in &plan.spends {
                        let rsk = sk.spend_auth_key().randomize(&spend_plan.randomizer);
                        spend_auths.push(rsk.sign(&mut rng, effect_hash.as_ref()));
                    }
                }
                crate::ActionPlan::Consolidate(plan) => {
                    for spend_plan in &plan.spends {
                        let rsk = sk.spend_auth_key().randomize(&spend_plan.randomizer);
                        spend_auths.push(rsk.sign(&mut rng, effect_hash.as_ref()));
                    }
                }
                crate::ActionPlan::Split(plan) => {
                    for spend_plan in &plan.spends {
                        let rsk = sk.spend_auth_key().randomize(&spend_plan.randomizer);
                        spend_auths.push(rsk.sign(&mut rng, effect_hash.as_ref()));
                    }
                }
                crate::ActionPlan::ShieldedIcs20Withdrawal(plan) => {
                    for spend_plan in &plan.spends {
                        let rsk = sk.spend_auth_key().randomize(&spend_plan.randomizer);
                        spend_auths.push(rsk.sign(&mut rng, effect_hash.as_ref()));
                    }
                }
                _ => {}
            }
        }
        if let Some(fee_funding) = &self.fee_funding {
            for spend_plan in &fee_funding.transfer.spends {
                let rsk = sk.spend_auth_key().randomize(&spend_plan.randomizer);
                spend_auths.push(rsk.sign(&mut rng, effect_hash.as_ref()));
            }
        }

        Ok(AuthorizationData {
            effect_hash: Some(effect_hash),
            spend_auths,
        })
    }
}
