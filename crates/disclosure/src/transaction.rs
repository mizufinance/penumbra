use crate::*;
use anyhow::{ensure, Context, Result};
use shieldd_sdk_keys::FullViewingKey;
use shieldd_sdk_shielded_pool::{Note, NotePayload};
use shieldd_sdk_transaction::{Action, Transaction};

pub fn public_output(
    tx: &Transaction,
    reference: &OutputRef,
    chain_id: &str,
) -> Result<PublicOutput> {
    ensure!(
        tx.id().to_string() == reference.transaction_id,
        "transaction reference mismatch"
    );
    ensure!(
        tx.transaction_parameters().chain_id == chain_id,
        "transaction chain mismatch"
    );
    let body = tx.transaction_body();
    let index = usize::try_from(reference.output)?;
    let (payload, wrapped, rk): (&NotePayload, &[u8], Option<Vec<u8>>) = match reference.action {
        ActionRef::FeeFunding => {
            let transfer = &body
                .fee_funding
                .as_ref()
                .context("missing fee funding")?
                .transfer;
            let output = transfer
                .body
                .outputs
                .get(index)
                .context("invalid output index")?;
            (&output.note_payload, &output.wrapped_memo_key.0, None)
        }
        ActionRef::Body(i) => match tx
            .actions()
            .nth(i as usize)
            .context("invalid action index")?
        {
            Action::Transfer(t) => {
                t.body.validate_shape()?;
                ensure!(
                    t.body.proof_context
                        == shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
                    "body Transfer must use ordinary proof context"
                );
                let output = t.body.outputs.get(index).context("invalid output index")?;
                let rk: [u8; 32] = t.body.inputs[0].rk.into();
                (
                    &output.note_payload,
                    &output.wrapped_memo_key.0,
                    Some(rk.to_vec()),
                )
            }
            Action::NoteReshape(t) => {
                let output = t.body.outputs.get(index).context("invalid output index")?;
                (&output.note_payload, &output.wrapped_memo_key.0, None)
            }
            Action::ShieldedHostWithdrawal(t) => {
                ensure!(index == 0, "invalid change output index");
                let output = &t.body.change_output;
                (&output.note_payload, &output.wrapped_memo_key.0, None)
            }
            _ => anyhow::bail!("action does not contain disclosable notes"),
        },
    };
    Ok(PublicOutput {
        reference: reference.clone(),
        commitment: hex::encode(payload.note_commitment.0.to_bytes()),
        ephemeral_key: payload.ephemeral_key.0.to_vec(),
        encrypted_note: payload.encrypted_note.0.to_vec(),
        wrapped_memo_key: wrapped.to_vec(),
        memo_ciphertext: tx.transaction_body().memo.as_ref().map(|m| m.0.to_vec()),
        spend_verification_key: rk,
    })
}

/// Prepare just the selected notes. Signatures are supplied by custody.
pub fn prepare(
    request: DisclosureRequest,
    transactions: &[Transaction],
    fvk: &FullViewingKey,
) -> Result<DisclosureWitness> {
    validate_request(&request)?;
    let mut outputs = Vec::new();
    for claim in &request.outputs {
        let tx = transactions
            .iter()
            .find(|t| t.id().to_string() == claim.reference.transaction_id)
            .context("selected transaction unavailable")?;
        let public = public_output(tx, &claim.reference, &request.chain_id)?;
        let commitment = shieldd_sdk_shielded_pool::note::StateCommitment::try_from(
            hex::decode(&public.commitment)?.as_slice(),
        )?;
        let keys = tx.payload_keys(fvk)?;
        let key = keys
            .get(&commitment)
            .context("selected payload key unavailable")?;
        let epk = decaf377_ka::Public(public.ephemeral_key.as_slice().try_into()?);
        let ciphertext =
            shieldd_sdk_shielded_pool::NoteCiphertext(public.encrypted_note.as_slice().try_into()?);
        let note = Note::decrypt_with_payload_key(&ciphertext, key, &epk)?;
        ensure!(
            note.commit() == commitment,
            "decrypted note commitment mismatch"
        );
        outputs.push(OutputWitness {
            public,
            note: note.to_bytes().to_vec(),
            control_signature: None,
        });
    }
    Ok(DisclosureWitness { request, outputs })
}

/// Caller supplies committed transactions from its chosen node, independently of the package.
pub fn confirm_acceptance(
    statement: &DisclosureStatement,
    chain_id: &str,
    blocks: &[AcceptedBlock],
) -> Result<Acceptance> {
    validate_request(&statement.request)?;
    ensure!(
        chain_id == statement.request.chain_id,
        "node chain mismatch"
    );
    ensure!(
        statement.outputs.len() == statement.request.outputs.len(),
        "statement count mismatch"
    );
    let mut heights = std::collections::BTreeSet::new();
    for (claim, output) in statement.request.outputs.iter().zip(&statement.outputs) {
        ensure!(
            claim.reference == output.public.reference,
            "statement reference mismatch"
        );
        let reference = &claim.reference;
        let block = blocks
            .iter()
            .find(|b| b.height == reference.height)
            .context("accepted block unavailable")?;
        let tx = block
            .transactions
            .iter()
            .find(|tx| tx.id().to_string() == reference.transaction_id)
            .context("transaction not accepted at supplied height")?;
        ensure!(
            public_output(tx, reference, chain_id)? == output.public,
            "accepted output differs from disclosure"
        );
        heights.insert(reference.height);
    }
    Ok(Acceptance::Confirmed {
        chain_id: chain_id.to_owned(),
        heights: heights.into_iter().collect(),
    })
}

/// Indexer data is candidate transaction content, never successful-execution evidence.
pub fn verify_candidates(
    statement: &DisclosureStatement,
    transactions: &[Transaction],
) -> Result<()> {
    validate_request(&statement.request)?;
    ensure!(
        transactions.len() <= MAX_OUTPUTS,
        "too many candidate transactions"
    );
    ensure!(
        statement.outputs.len() == statement.request.outputs.len(),
        "output count mismatch"
    );
    for (claim, output) in statement.request.outputs.iter().zip(&statement.outputs) {
        let tx = transactions
            .iter()
            .find(|tx| tx.id().to_string() == claim.reference.transaction_id)
            .context("candidate transaction missing")?;
        ensure!(
            public_output(tx, &claim.reference, &statement.request.chain_id)? == output.public,
            "candidate public data mismatch"
        );
    }
    Ok(())
}
