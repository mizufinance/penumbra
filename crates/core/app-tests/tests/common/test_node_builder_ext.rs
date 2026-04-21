use {
    decaf377_rdsa::VerificationKey,
    penumbra_sdk_app::genesis::AppState,
    penumbra_sdk_keys::keys::{SpendKey, SpendKeyBytes},
    penumbra_sdk_mock_consensus::builder::Builder,
    penumbra_sdk_proto::{
        core::keys::v1::{GovernanceKey, IdentityKey},
        penumbra::core::component::validator::v1::Validator as PenumbraValidator,
    },
    rand::Rng,
    rand_core::OsRng,
    tracing::trace,
};

/// Penumbra-specific extensions to the mock consensus builder.
pub trait BuilderExt: Sized {
    /// The error thrown by [`with_penumbra_auto_app_state`]
    type Error;
    /// Add the provided Penumbra [`AppState`] to the builder.
    ///
    /// This will inject any configured validators into the state before serializing it into bytes.
    fn with_penumbra_auto_app_state(self, app_state: AppState) -> Result<Self, Self::Error>;
}

impl BuilderExt for Builder {
    type Error = anyhow::Error;
    fn with_penumbra_auto_app_state(mut self, app_state: AppState) -> Result<Self, Self::Error> {
        let Self { keyring, .. } = &self;
        let mut content = match app_state {
            AppState::Content(c) => c,
            AppState::Checkpoint(_) => anyhow::bail!("checkpointed state is not supported"),
        };

        for (consensus_vk, _) in keyring {
            // Let the seed for the penumbra validator be derived from the verification key,
            // that way tests can operate with no rng.
            let seed = Some(SpendKeyBytes(consensus_vk.to_bytes()));

            // Generate a penumbra validator with this consensus key.
            let validator = generate_penumbra_sdk_validator(consensus_vk, seed);

            trace!(?validator, "adding validator to validator genesis content");
            content.validator_content.validators.push(validator);
        }

        // Set the chain ID from the content
        if !content.chain_id.is_empty() {
            self.chain_id = Some(content.chain_id.clone());
        }

        // Serialize the app state into bytes, and add it to the builder.
        let app_state = AppState::Content(content);
        serde_json::to_vec(&app_state)
            .map_err(Self::Error::from)
            .map(|s| self.app_state(s))
    }
}

/// Generates a [`Validator`][PenumbraValidator] given a consensus verification key.
fn generate_penumbra_sdk_validator(
    consensus_key: &ed25519_consensus::VerificationKey,
    seed: Option<SpendKeyBytes>,
) -> PenumbraValidator {
    let seed = seed.unwrap_or(SpendKeyBytes(OsRng.gen()));
    let spend_key = SpendKey::from(seed.clone());
    let validator_id_sk = spend_key.spend_auth_key();
    let validator_id_vk = VerificationKey::from(validator_id_sk);

    let v = PenumbraValidator {
        identity_key: Some(IdentityKey {
            ik: validator_id_vk.to_bytes().to_vec(),
        }),
        // NB: for now, we will use the same key for governance. See the documentation of
        // `GovernanceKey` for more information about cold storage of validator keys.
        governance_key: Some(GovernanceKey {
            gk: validator_id_vk.to_bytes().to_vec(),
        }),
        consensus_key: consensus_key.as_bytes().to_vec(),
        enabled: true,
        sequence_number: 0,
        name: String::default(),
        website: String::default(),
        description: String::default(),
    };

    v
}
