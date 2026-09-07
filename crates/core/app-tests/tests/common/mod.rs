//! Shared integration testing facilities.

// NB: these reëxports are shared and consumed by files in `tests/`.
#[allow(unused_imports)]
pub use {
    self::{
        temp_storage_ext::TempStorageExt, test_node_builder_ext::BuilderExt,
        test_node_ext::TestNodeExt,
    },
    shieldd_sdk_test_subscriber::{set_tracing_subscriber, set_tracing_subscriber_with_env_filter},
};

use cnidarium::StateWrite;
use shieldd_sdk_asset::asset;
use shieldd_sdk_compliance::{ComplianceLeaf, ComplianceRegistryWrite};
use shieldd_sdk_keys::Address;
/// Register test users in the compliance registry with BLACK_HOLE_ACK.
///
/// This helper registers the given addresses for the specified assets as unregulated
/// users (using BLACK_HOLE_ACK). This is necessary for tests that build transactions
/// with shielded input/output plans, as the compliance circuit requires valid Merkle proofs.
///
/// # Example
/// ```ignore
/// let mut state = StateDelta::new(storage.latest_snapshot());
/// register_test_users_for_compliance(
///     &mut state,
///     &[sender_address, recipient_address],
///     &[staking_token_id],
/// ).await?;
/// storage.commit(state).await?;
/// ```
#[allow(dead_code)]
pub async fn register_test_users_for_compliance<S: StateWrite>(
    state: &mut S,
    addresses: &[Address],
    asset_ids: &[asset::Id],
) -> anyhow::Result<()> {
    for address in addresses {
        for &asset_id in asset_ids {
            let leaf = ComplianceLeaf::synthetic_unregulated(address.clone(), asset_id);
            state.test_only_add_compliance_leaf(leaf).await?;
        }
    }
    Ok(())
}

/// Create a StateDelta with compliance registrations for building transactions.
///
/// For tests that use TestNode.block().execute() pattern, this creates a state layer
/// with compliance data for `witness_auth_build_with_compliance`. The returned StateDelta
/// is NOT committed to storage - it's only used for building the transaction.
///
/// Note: The actual chain will NOT have this compliance data, which means transactions
/// will fail stateful checks unless the assets are already registered.
#[allow(dead_code)]
pub async fn state_with_compliance_for_build(
    storage: &cnidarium::TempStorage,
    addresses: &[Address],
    asset_ids: &[asset::Id],
) -> anyhow::Result<cnidarium::StateDelta<cnidarium::Snapshot>> {
    use cnidarium::StateDelta;

    let mut delta = StateDelta::new(storage.latest_snapshot());

    // Register users with real d (matching what the circuit derives from the address)
    for address in addresses {
        for &asset_id in asset_ids {
            let leaf = ComplianceLeaf::synthetic_unregulated(address.clone(), asset_id);
            delta.test_only_add_compliance_leaf(leaf).await?;
        }
    }

    Ok(delta)
}

/// Shieldd-specific extensions to the mock consensus builder.
///
/// See [`BuilderExt`].
mod test_node_builder_ext;

/// Extensions to [`TempStorage`][cnidarium::TempStorage].
mod temp_storage_ext;

/// Shieldd-specific extensions to the mock consensus test node.
///
/// See [`TestNodeExt`].
mod test_node_ext;

/// Methods for testing IBC functionality.
#[allow(unused)]
pub mod ibc_tests;
