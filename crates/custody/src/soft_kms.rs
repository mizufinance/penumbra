//! A basic software key management system that stores keys in memory but
//! presents as an asynchronous signer.

use rand_core::OsRng;
use shieldd_sdk_proto::custody::v1::{self as pb, AuthorizeResponse};
use shieldd_sdk_transaction::AuthorizationData;
use tonic::{async_trait, Request, Response, Status};

use crate::{policy::Policy, AuthorizeRequest};

mod config;

pub use config::Config;

/// A basic software key management system that stores keys in memory but
/// presents as an asynchronous signer.
pub struct SoftKms {
    config: Config,
}

impl SoftKms {
    /// Initialize with the given [`Config`].
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Sign a fresh disclosure challenge without releasing the spending key.
    pub fn sign_disclosure(
        &self,
        request: &shieldd_sdk_disclosure::DisclosureRequest,
        public: &shieldd_sdk_disclosure::PublicOutput,
        randomizer: [u8; 32],
    ) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(
            request
                .outputs
                .iter()
                .any(|c| c.reference == public.reference && c.spending_control),
            "output does not request authority control"
        );
        let randomizer = decaf377::Fr::from_bytes_checked(&randomizer)
            .map_err(|_| anyhow::anyhow!("invalid authorization randomizer"))?;
        let key = self
            .config
            .spend_key
            .spend_auth_key()
            .randomize(&randomizer);
        let verification_key = self
            .config
            .spend_key
            .full_viewing_key()
            .spend_verification_key()
            .randomize(&randomizer);
        anyhow::ensure!(
            !verification_key.is_identity(),
            "identity authority cannot prove secret control"
        );
        let expected: [u8; 32] = verification_key.into();
        anyhow::ensure!(
            public.spend_verification_key.as_deref() == Some(expected.as_slice()),
            "custody does not control selected Transfer authority"
        );
        let signature: [u8; 64] = key
            .sign(OsRng, &shieldd_sdk_disclosure::control_message(request)?)
            .into();
        Ok(signature.to_vec())
    }

    /// Attempt to authorize the requested [`TransactionPlan`](shieldd_sdk_transaction::TransactionPlan).
    #[tracing::instrument(skip(self, request), name = "softhsm_sign")]
    pub fn sign(&self, request: &AuthorizeRequest) -> anyhow::Result<AuthorizationData> {
        tracing::debug!(?request.plan);

        for policy in &self.config.auth_policy {
            policy.check_transaction(request)?;
        }

        Ok(request.plan.authorize(OsRng, &self.config.spend_key)?)
    }
}

#[async_trait]
impl pb::custody_service_server::CustodyService for SoftKms {
    async fn authorize(
        &self,
        request: Request<pb::AuthorizeRequest>,
    ) -> Result<Response<AuthorizeResponse>, Status> {
        let request = request
            .into_inner()
            .try_into()
            .map_err(|e: anyhow::Error| Status::invalid_argument(e.to_string()))?;

        let authorization_data = self
            .sign(&request)
            .map_err(|e| Status::unauthenticated(format!("{e:#}")))?;

        let authorization_response = AuthorizeResponse {
            data: Some(authorization_data.into()),
        };

        Ok(Response::new(authorization_response))
    }

    async fn export_full_viewing_key(
        &self,
        _request: Request<pb::ExportFullViewingKeyRequest>,
    ) -> Result<Response<pb::ExportFullViewingKeyResponse>, Status> {
        Ok(Response::new(pb::ExportFullViewingKeyResponse {
            full_viewing_key: Some(self.config.spend_key.full_viewing_key().clone().into()),
        }))
    }

    async fn confirm_address(
        &self,
        request: Request<pb::ConfirmAddressRequest>,
    ) -> Result<Response<pb::ConfirmAddressResponse>, Status> {
        let address_index = request
            .into_inner()
            .address_index
            .ok_or_else(|| {
                Status::invalid_argument("missing address index in confirm address request")
            })?
            .try_into()
            .map_err(|e| {
                Status::invalid_argument(format!(
                    "invalid address index in confirm address request: {e:#}"
                ))
            })?;

        let address = self
            .config
            .spend_key
            .full_viewing_key()
            .payment_address(address_index);

        Ok(Response::new(pb::ConfirmAddressResponse {
            address: Some(address.into()),
        }))
    }
}
