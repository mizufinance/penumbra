//! Explicit unregulated fixtures for plan and builder tests.
use crate::{
    ActionWitness, AssetWitness, HostWithdrawal, NoteReshapeContext, NoteReshapeFamilyId,
    NoteReshapePlan, ShieldedHostWithdrawalPlan, ShieldedInputPlan, ShieldedOutputPlan,
    TransferContext, TransferPlan, UserWitness, WithdrawalContext,
};
use anyhow::{Context, Result};
use decaf377::Fr;
use shieldd_sdk_asset::asset;
use shieldd_sdk_compliance::ComplianceLeaf;
use shieldd_sdk_keys::Address;

pub const TIMESTAMP: u64 = 1_700_000_000;

pub fn witness(asset_id: asset::Id, sender: &Address) -> ActionWitness {
    let sender = user_witness(asset_id, sender);
    let (user_root, _, _) = shieldd_sdk_compliance::default_user_proof(&sender.leaf);
    let (root, leaf, path, position) = shieldd_sdk_compliance::create_default_imt_proof(asset_id.0);
    ActionWitness {
        asset: AssetWitness {
            asset_id,
            root,
            leaf,
            position,
            path,
            is_regulated: false,
        },
        policy: None,
        user_root,
        sender,
    }
}

pub fn user_witness(asset_id: asset::Id, address: &Address) -> UserWitness {
    let leaf = ComplianceLeaf::synthetic_unregulated(address.clone(), asset_id);
    let (_, path, position) = shieldd_sdk_compliance::default_user_proof(&leaf);
    UserWitness {
        leaf,
        path,
        position,
    }
}

pub fn transfer_context(spend: &ShieldedInputPlan, recipient: &Address) -> TransferContext {
    TransferContext {
        witness: witness(spend.note.asset_id(), &spend.note.address()),
        recipient: user_witness(spend.note.asset_id(), recipient),
        timestamp: TIMESTAMP,
        nonce: Fr::rand(&mut rand_core::OsRng),
    }
}

pub fn transfer(
    spends: Vec<ShieldedInputPlan>,
    outputs: Vec<ShieldedOutputPlan>,
    blinding: Fr,
) -> Result<TransferPlan> {
    let context = transfer_context(
        spends.first().context("fixture requires a spend")?,
        &outputs
            .first()
            .context("fixture requires an output")?
            .dest_address,
    );
    let volume_accumulator = crate::VolumeAccumulatorPlan::padding(context.timestamp);
    TransferPlan::new(
        spends,
        outputs,
        blinding,
        context,
        volume_accumulator,
        crate::TransferProofContext::Ordinary,
        crate::discovery::Parameters::default(),
    )
}

pub fn fee_funding(
    spends: Vec<ShieldedInputPlan>,
    outputs: Vec<ShieldedOutputPlan>,
    blinding: Fr,
) -> Result<TransferPlan> {
    let context = transfer_context(
        spends.first().context("fixture requires a spend")?,
        &outputs
            .first()
            .context("fixture requires an output")?
            .dest_address,
    );
    let volume = crate::VolumeAccumulatorPlan::padding(context.timestamp);
    TransferPlan::new(
        spends,
        outputs,
        blinding,
        context,
        volume,
        crate::TransferProofContext::FeeFunding,
        crate::discovery::Parameters::default(),
    )
}

pub fn note_reshape(
    family: NoteReshapeFamilyId,
    spends: Vec<ShieldedInputPlan>,
    outputs: Vec<ShieldedOutputPlan>,
    blinding: Fr,
) -> Result<NoteReshapePlan> {
    let spend = spends.first().context("fixture requires a spend")?;
    let context = NoteReshapeContext {
        witness: witness(spend.note.asset_id(), &spend.note.address()),
        nonce: Fr::rand(&mut rand_core::OsRng),
    };
    NoteReshapePlan::new(
        family,
        spends,
        outputs,
        blinding,
        context,
        crate::discovery::Parameters::default(),
    )
}

pub fn withdrawal_context(spend: &ShieldedInputPlan) -> WithdrawalContext {
    WithdrawalContext {
        witness: witness(spend.note.asset_id(), &spend.note.address()),
        timestamp: TIMESTAMP,
        nonce: Fr::rand(&mut rand_core::OsRng),
    }
}

pub fn host_withdrawal(
    spends: Vec<ShieldedInputPlan>,
    change: Option<ShieldedOutputPlan>,
    withdrawal: HostWithdrawal,
    blinding: Fr,
) -> Result<ShieldedHostWithdrawalPlan> {
    let context = withdrawal_context(spends.first().context("fixture requires a spend")?);
    let volume_accumulator = crate::VolumeAccumulatorPlan::padding(context.timestamp);
    ShieldedHostWithdrawalPlan::new(
        spends,
        change,
        withdrawal,
        blinding,
        context,
        volume_accumulator,
        crate::discovery::Parameters::default(),
    )
}
