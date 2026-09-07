use anyhow::{ensure, Context};
use cnidarium::Storage;
use shieldd_sdk_proto::{StateReadProto, StateWriteProto};

use super::APP_VERSION;

/// Reject populated state without the current schema safeguard.
pub async fn check_app_version(storage: &Storage) -> anyhow::Result<()> {
    if storage.latest_version() == u64::MAX {
        return Ok(());
    }
    let found: Option<u64> = storage
        .latest_snapshot()
        .nonverifiable_get_proto(crate::app::state_key::app_version::safeguard().as_bytes())
        .await
        .context("reading app version safeguard")?;
    let found = found.context("missing app version safeguard; recreate development state")?;
    ensure!(
        found == APP_VERSION,
        "incompatible app version {found}; expected {APP_VERSION}; recreate development state"
    );
    Ok(())
}

/// Set the safeguard as part of fresh genesis initialization.
pub(crate) fn initialize_app_version<S: StateWriteProto>(state: &mut S) {
    state.nonverifiable_put_proto(
        crate::app::state_key::app_version::safeguard()
            .as_bytes()
            .to_vec(),
        APP_VERSION,
    );
}

#[cfg(test)]
mod test {
    use super::*;
    use cnidarium::StateDelta;

    #[tokio::test]
    async fn populated_state_without_version_is_rejected() {
        let storage = cnidarium::TempStorage::new().await.unwrap();
        let mut state = StateDelta::new(storage.latest_snapshot());
        state.nonverifiable_put_proto(b"test/populated".to_vec(), 1u64);
        storage.commit(state).await.unwrap();
        let error = check_app_version(&storage).await.unwrap_err();
        assert!(
            error.to_string().contains("missing app version safeguard"),
            "{error:#}"
        );
    }

    #[tokio::test]
    async fn fresh_state_and_initialized_current_version_are_accepted() {
        let storage = cnidarium::TempStorage::new().await.unwrap();
        check_app_version(&storage).await.unwrap();
        assert_eq!(storage.latest_version(), u64::MAX);
        let mut state = StateDelta::new(storage.latest_snapshot());
        initialize_app_version(&mut state);
        storage.commit(state).await.unwrap();
        check_app_version(&storage).await.unwrap();
    }

    #[tokio::test]
    async fn incompatible_version_is_rejected() {
        let storage = cnidarium::TempStorage::new().await.unwrap();
        let mut state = StateDelta::new(storage.latest_snapshot());
        state.nonverifiable_put_proto(
            crate::app::state_key::app_version::safeguard()
                .as_bytes()
                .to_vec(),
            APP_VERSION - 1,
        );
        storage.commit(state).await.unwrap();
        let error = check_app_version(&storage).await.unwrap_err();
        assert!(error.to_string().contains("incompatible app version"));
    }
}
