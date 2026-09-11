//! Pure registration-grant validation shared by issuance and chain admission.
use crate::{AssetPolicy, UserAssetStatus};
use anyhow::Result;
use shieldd_sdk_asset::asset;

pub(crate) fn ensure_regulated_asset_id(asset_id: asset::Id, is_regulated: bool) -> Result<()> {
    anyhow::ensure!(
        !is_regulated || asset_id != *shieldd_sdk_asset::BASE_ASSET_ID,
        "the base fee asset cannot be registered as regulated"
    );
    Ok(())
}

/// Validate the authority grant before Orbis issues its separate certificate.
pub fn validate_user_grant(
    action: &crate::structs::MsgRegisterUser,
    policy: &AssetPolicy,
    current_unix: u64,
) -> Result<()> {
    policy.validate_regulated()?;
    let grant = action
        .grant
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing user registration grant"))?;
    anyhow::ensure!(
        grant.body.leaf == action.leaf,
        "user registration grant leaf does not match action leaf"
    );
    action.leaf.validate_registration(policy.ring.ring_pk)?;
    anyhow::ensure!(
        action.leaf.audit_keys.epoch == policy.ring.audit_keys.epoch,
        "user audit key epoch does not match asset policy"
    );
    anyhow::ensure!(
        action.leaf.status == UserAssetStatus::Active,
        "user registrations must start active"
    );
    anyhow::ensure!(
        grant.body.policy_id == policy.ring.policy_id,
        "user registration grant policy_id does not match asset policy"
    );
    anyhow::ensure!(
        current_unix <= grant.body.valid_until_unix,
        "user registration grant expired"
    );
    let authority_vk = policy
        .registration_authority_vk
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("regulated asset policy missing registration authority"))?;
    grant.verify(authority_vk)?;
    Ok(())
}

/// Validate the registrar grant before Orbis certifies the general audit keys.
pub fn policy_from_asset_grant(
    action: &crate::structs::MsgRegisterAsset,
    current_unix: u64,
) -> Result<AssetPolicy> {
    ensure_regulated_asset_id(action.asset_id, action.is_regulated)?;
    action.validate_authorization_keys()?;
    let grant = action
        .asset_registration_grant
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing asset registration grant"))?;
    anyhow::ensure!(
        grant.body == action.registration_grant_body(grant.body.valid_until_unix),
        "asset registration grant body does not match action"
    );
    grant.verify()?;
    anyhow::ensure!(
        current_unix <= grant.body.valid_until_unix,
        "asset registration grant expired"
    );

    let policy = if action.is_regulated {
        let dk_pub = action
            .dk_pub
            .ok_or_else(|| anyhow::anyhow!("regulated assets require a detection key (dk_pub)"))?;
        let registration_authority_vk = action
            .registration_authority_vk
            .ok_or_else(|| anyhow::anyhow!("regulated assets require registration_authority_vk"))?;
        let seizure_authority_vk = action
            .seizure_authority_vk
            .ok_or_else(|| anyhow::anyhow!("regulated assets require seizure_authority_vk"))?;
        let daily_volume_limit = action.daily_volume_limit.unwrap_or(u128::MAX);
        let ring_pk = action
            .ring_pk
            .ok_or_else(|| anyhow::anyhow!("regulated assets require ring_pk"))?;
        AssetPolicy::new(
            dk_pub,
            daily_volume_limit,
            action.allowed_ibc_routes.clone(),
            action.ibc_origin.clone(),
            action.ring_id.clone(),
            ring_pk,
            action.policy_id.clone(),
            action.permission.clone(),
            action.resource.clone(),
            action
                .audit_keys
                .clone()
                .ok_or_else(|| anyhow::anyhow!("regulated asset requires audit keys"))?,
        )
        .with_registration_authority(registration_authority_vk)
        .with_seizure_authority(seizure_authority_vk)
    } else {
        anyhow::ensure!(
            action.dk_pub.is_none(),
            "unregulated assets cannot set dk_pub"
        );
        anyhow::ensure!(
            action.daily_volume_limit.is_none(),
            "unregulated assets cannot set a compliance daily_volume_limit"
        );
        anyhow::ensure!(
            action.allowed_ibc_routes.is_empty(),
            "unregulated assets cannot set allowed IBC routes"
        );
        anyhow::ensure!(
            action.ibc_origin.is_none(),
            "unregulated assets cannot set IBC origin"
        );
        anyhow::ensure!(
            action.audit_keys.is_none()
                && action.ring_pk.is_none()
                && action.ring_id.is_empty()
                && action.policy_id.is_empty()
                && action.permission.is_empty()
                && action.resource.is_empty(),
            "unregulated assets cannot set Orbis configuration"
        );
        anyhow::ensure!(
            action.registration_authority_vk.is_none() && action.seizure_authority_vk.is_none(),
            "unregulated assets cannot set compliance authorities"
        );
        AssetPolicy::default_unregulated()
    };
    if action.is_regulated {
        policy.validate_regulated()?;
    } else {
        policy.validate_crypto_keys()?;
    }
    Ok(policy)
}
