//! A set of basic spend authorization policies.

use std::collections::HashSet;

use penumbra_sdk_keys::Address;
use penumbra_sdk_proto::{
    core::{
        component::{
            governance::v1::ProposalSubmitBody as ProtoProposalSubmitBody,
            governance::v1::ValidatorVoteBody as ProtoValidatorVoteBody,
            validator::v1::Validator as ProtoValidator,
        },
        transaction::v1::TransactionPlan as ProtoTransactionPlan,
    },
    Message as _,
};
use penumbra_sdk_transaction::plan::ActionPlan;
use serde::{Deserialize, Serialize};

use crate::{
    AuthorizeProposalSubmitRequest, AuthorizeRequest, AuthorizeValidatorDefinitionRequest,
    AuthorizeValidatorVoteRequest, PreAuthorization,
};

/// A trait for checking whether a transaction plan is allowed by a policy.
pub trait Policy {
    /// Checks whether the proposed transaction plan is allowed by this policy.
    fn check_transaction(&self, request: &AuthorizeRequest) -> anyhow::Result<()>;

    /// Checks whether the proposed validator definition is allowed by this policy.
    fn check_validator_definition(
        &self,
        _request: &AuthorizeValidatorDefinitionRequest,
    ) -> anyhow::Result<()>;

    /// Checks whether the proposed validator vote is allowed by this policy.
    fn check_validator_vote(&self, _request: &AuthorizeValidatorVoteRequest) -> anyhow::Result<()>;

    /// Checks whether the proposed proposal submission is allowed by this policy.
    fn check_proposal_submit(
        &self,
        _request: &AuthorizeProposalSubmitRequest,
    ) -> anyhow::Result<()>;
}

/// A set of basic spend authorization policies.
///
/// These policies are intended to be simple enough that they can be written by hand in a config
/// file.  More complex policy logic than should be implemented by a custom implementation of
/// the [`Policy`] trait.
///
/// These policies do not permit validator votes or validator definition updates, so a custom policy
/// must be used to approve these actions.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
#[serde(tag = "type")]
pub enum AuthPolicy {
    /// Only allow transactions whose outputs are controlled by one of the
    /// allowed destination addresses.
    DestinationAllowList {
        #[serde(with = "address_as_string")]
        allowed_destination_addresses: Vec<Address>,
    },
    /// Intended for relayers, only allows `IbcAction` transactions plus at most
    /// one self-addressed transfer used to fund fees.
    OnlyIbcRelay,
    /// Require specific pre-authorizations for submitted [`TransactionPlan`](penumbra_sdk_transaction::TransactionPlan)s.
    PreAuthorization(PreAuthorizationPolicy),
}

/// A set of pre-authorization policies.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
// We need to use a different tag name here, so we can stack it with the
// SpendPolicy tag; in toml, for instance, this will turn into
// [[spend_policy]]
// type = 'PreAuthorization'
// method = 'Ed25519'
#[serde(tag = "method")]
pub enum PreAuthorizationPolicy {
    Ed25519 {
        /// The number of distinct pre-authorizations required to authorize a transaction plan.
        ///
        /// Each `allowed_signer`'s contributions count only once towards this total.
        required_signatures: u32,
        /// A list of pre-authorization keys that can be used to authorize a transaction plan.
        #[serde(with = "ed25519_vec_base64")]
        allowed_signers: Vec<ed25519_consensus::VerificationKey>,
    },
}

impl PreAuthorizationPolicy {
    fn check_pre_authorizations(
        &self,
        pre_authorizations: &[PreAuthorization],
        signed_data: impl AsRef<[u8]>,
    ) -> anyhow::Result<()> {
        let signed_data = signed_data.as_ref();
        match self {
            PreAuthorizationPolicy::Ed25519 {
                required_signatures,
                allowed_signers,
            } => {
                #[allow(clippy::unnecessary_filter_map)]
                let ed25519_pre_auths =
                    pre_authorizations
                        .iter()
                        .filter_map(|pre_auth| match pre_auth {
                            PreAuthorization::Ed25519(pre_auth) => Some(pre_auth),
                            // _ => None,
                        });

                let mut allowed_signers = allowed_signers.iter().cloned().collect::<HashSet<_>>();
                let mut seen_signers = HashSet::new();

                for pre_auth in ed25519_pre_auths {
                    // Remove the signer from the allowed signers set, so that
                    // each signer can only submit one pre-authorization.
                    if let Some(signer) = allowed_signers.take(&pre_auth.vk) {
                        pre_auth.verify(signed_data)?;
                        seen_signers.insert(signer);
                    }
                }

                if seen_signers.len() < *required_signatures as usize {
                    anyhow::bail!(
                        "required {} pre-authorization signatures but only saw {}",
                        required_signatures,
                        seen_signers.len(),
                    );
                }
                Ok(())
            }
        }
    }
}

mod address_as_string {
    use std::str::FromStr;

    use penumbra_sdk_keys::Address;

    pub fn serialize<S: serde::Serializer>(
        addresses: &[Address],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::Serialize;
        let mut string_addresses = Vec::with_capacity(addresses.len());
        for address in addresses {
            string_addresses.push(address.to_string());
        }
        string_addresses.serialize(serializer)
    }
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Address>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::Deserialize;
        let string_addresses: Vec<String> = Vec::deserialize(deserializer)?;
        let mut addresses = Vec::with_capacity(string_addresses.len());
        for string_address in string_addresses {
            let address = Address::from_str(&string_address).map_err(serde::de::Error::custom)?;
            addresses.push(address);
        }
        Ok(addresses)
    }
}

/// A serde helper to serialize pre-authorization keys as base64-encoded data.
/// Because Go's encoding/json will encode byte[] as base64-encoded strings,
/// and Go's Ed25519 keys are byte[] values, this hopefully makes it easier to
/// copy-paste pre-authorization keys from Go programs into the Rust config.
// TODO: remove this after <https://github.com/mizufinance/ed25519-consensus/issues/7>
mod ed25519_vec_base64 {
    use base64::prelude::*;

    pub fn serialize<S: serde::Serializer>(
        keys: &[ed25519_consensus::VerificationKey],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        use serde::Serialize;
        let mut base64_keys = Vec::with_capacity(keys.len());
        for key in keys {
            base64_keys.push(BASE64_STANDARD.encode(key.as_bytes()));
        }
        base64_keys.serialize(serializer)
    }
    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Vec<ed25519_consensus::VerificationKey>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::Deserialize;
        let base64_keys: Vec<String> = Vec::deserialize(deserializer)?;
        let mut vks = Vec::with_capacity(base64_keys.len());
        for base64_key in base64_keys {
            let bytes = BASE64_STANDARD
                .decode(base64_key)
                .map_err(serde::de::Error::custom)?;
            let vk = ed25519_consensus::VerificationKey::try_from(bytes.as_slice())
                .map_err(serde::de::Error::custom)?;
            vks.push(vk);
        }
        Ok(vks)
    }
}

impl Policy for AuthPolicy {
    fn check_transaction(&self, request: &AuthorizeRequest) -> anyhow::Result<()> {
        let plan = &request.plan;
        match self {
            AuthPolicy::DestinationAllowList {
                allowed_destination_addresses,
            } => {
                for address in plan.dest_addresses() {
                    if !allowed_destination_addresses.contains(&address) {
                        anyhow::bail!("destination address {:?} not in allow list", address);
                    }
                }
                Ok(())
            }
            AuthPolicy::OnlyIbcRelay => {
                let mut ibc_action_count = 0usize;
                let mut transfer_count = 0usize;

                for action in &plan.actions {
                    match action {
                        ActionPlan::IbcAction { .. } => {
                            ibc_action_count += 1;
                        }
                        ActionPlan::Transfer(transfer) => {
                            transfer_count += 1;
                            if transfer_count > 1 {
                                anyhow::bail!(
                                    "OnlyIbcRelay allows at most one transfer funding action"
                                );
                            }

                            let sender = transfer
                                .inputs()
                                .first()
                                .expect("transfer plans always contain at least one real input")
                                .note
                                .address();
                            if transfer.dest_addresses().any(|dest| dest != sender) {
                                anyhow::bail!(
                                    "OnlyIbcRelay only allows self-addressed transfer outputs"
                                );
                            }
                        }
                        _ => {
                            anyhow::bail!("action {:?} not allowed by OnlyRelay policy", action);
                        }
                    }
                }

                if ibc_action_count == 0 {
                    anyhow::bail!("OnlyIbcRelay requires at least one IBC relay action");
                }

                Ok(())
            }
            AuthPolicy::PreAuthorization(policy) => policy.check_transaction(request),
        }
    }

    fn check_validator_definition(
        &self,
        _request: &AuthorizeValidatorDefinitionRequest,
    ) -> anyhow::Result<()> {
        anyhow::bail!("validator definitions are not allowed by this policy")
    }

    fn check_validator_vote(&self, _request: &AuthorizeValidatorVoteRequest) -> anyhow::Result<()> {
        anyhow::bail!("validator votes are not allowed by this policy")
    }

    fn check_proposal_submit(
        &self,
        _request: &AuthorizeProposalSubmitRequest,
    ) -> anyhow::Result<()> {
        anyhow::bail!("proposal submissions are not allowed by this policy")
    }
}

impl Policy for PreAuthorizationPolicy {
    fn check_transaction(&self, request: &AuthorizeRequest) -> anyhow::Result<()> {
        self.check_pre_authorizations(
            &request.pre_authorizations,
            ProtoTransactionPlan::from(request.plan.clone()).encode_to_vec(),
        )
    }

    fn check_validator_definition(
        &self,
        request: &AuthorizeValidatorDefinitionRequest,
    ) -> anyhow::Result<()> {
        self.check_pre_authorizations(
            &request.pre_authorizations,
            ProtoValidator::from(request.validator_definition.clone()).encode_to_vec(),
        )
    }

    fn check_validator_vote(&self, request: &AuthorizeValidatorVoteRequest) -> anyhow::Result<()> {
        self.check_pre_authorizations(
            &request.pre_authorizations,
            ProtoValidatorVoteBody::from(request.validator_vote.clone()).encode_to_vec(),
        )
    }

    fn check_proposal_submit(
        &self,
        request: &AuthorizeProposalSubmitRequest,
    ) -> anyhow::Result<()> {
        self.check_pre_authorizations(
            &request.pre_authorizations,
            ProtoProposalSubmitBody::from(request.proposal_submit.clone()).encode_to_vec(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use decaf377::Fr;
    use penumbra_sdk_asset::{Value, BASE_ASSET_ID};
    use penumbra_sdk_ibc::IbcRelay;
    use penumbra_sdk_keys::{
        keys::{Bip44Path, SeedPhrase, SpendKey},
        Address,
    };
    use penumbra_sdk_shielded_pool::{
        Note, Rseed, ShieldedInputPlan, ShieldedOutputPlan, TransferPlan,
    };
    use penumbra_sdk_transaction::TransactionPlan;
    use rand_core::OsRng;

    fn test_address(index: u32) -> Address {
        let seed = SeedPhrase::from_randomness(&[index as u8; 32]);
        let spend_key = SpendKey::from_seed_phrase_bip44(seed, &Bip44Path::new(0));
        spend_key
            .full_viewing_key()
            .incoming()
            .payment_address(index.into())
            .0
    }

    fn test_ibc_action() -> IbcRelay {
        IbcRelay::Unknown(pbjson_types::Any {
            type_url: "/penumbra.test.ibc".to_owned(),
            value: vec![1, 2, 3].into(),
        })
    }

    fn test_transfer(dest_address: Address) -> TransferPlan {
        let mut rng = OsRng;
        let sender = test_address(10);
        let note = Note::from_parts(
            sender.clone(),
            Value {
                amount: 5u64.into(),
                asset_id: *BASE_ASSET_ID,
            },
            Rseed::generate(&mut rng),
        )
        .expect("valid test note");
        let spend = ShieldedInputPlan::new(&mut rng, note, 0u64.into());
        let mut output = ShieldedOutputPlan::new(
            &mut rng,
            Value {
                amount: 4u64.into(),
                asset_id: *BASE_ASSET_ID,
            },
            dest_address,
        );
        output.asset_anchor = spend.asset_anchor;
        output.compliance_anchor = spend.compliance_anchor;
        output.target_timestamp = spend.target_timestamp;
        output.is_regulated = spend.is_regulated;
        output.tx_blinding_nonce = spend.tx_blinding_nonce;
        output.asset_indexed_leaf = spend.asset_indexed_leaf.clone();
        output.asset_path = spend.asset_path.clone();
        output.asset_position = spend.asset_position;
        output.asset_policy = spend.asset_policy.clone();

        TransferPlan::from_spend_output(spend.into(), output.into(), Fr::from(1u64))
            .expect("valid transfer plan")
    }

    #[test]
    fn only_ibc_relay_accepts_self_funded_transfer() {
        let sender = test_address(10);
        let request = AuthorizeRequest {
            plan: TransactionPlan {
                actions: vec![
                    ActionPlan::Transfer(test_transfer(sender)),
                    ActionPlan::IbcAction(test_ibc_action()),
                ],
                ..Default::default()
            },
            pre_authorizations: vec![],
        };

        AuthPolicy::OnlyIbcRelay
            .check_transaction(&request)
            .expect("self-funded relay transaction should be allowed");
    }

    #[test]
    fn only_ibc_relay_rejects_external_transfer_outputs() {
        let request = AuthorizeRequest {
            plan: TransactionPlan {
                actions: vec![
                    ActionPlan::Transfer(test_transfer(test_address(12))),
                    ActionPlan::IbcAction(test_ibc_action()),
                ],
                ..Default::default()
            },
            pre_authorizations: vec![],
        };

        let err = AuthPolicy::OnlyIbcRelay
            .check_transaction(&request)
            .expect_err("external transfer outputs should be rejected");
        assert!(err.to_string().contains("self-addressed"));
    }
}
