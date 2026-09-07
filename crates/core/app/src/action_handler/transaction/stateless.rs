use anyhow::{Context, Result};
use shieldd_sdk_transaction::{is_no_binding_signature, Action, Transaction};
use shieldd_sdk_txhash::AuthorizingData;

fn note_creating_output_count(tx: &Transaction) -> usize {
    let action_outputs = tx
        .actions()
        .map(|action| match action {
            Action::Transfer(transfer) => transfer.body.outputs.len(),
            Action::NoteReshape(note_reshape) => note_reshape.body.outputs.len(),
            Action::ShieldedIcs20Withdrawal(_) => 1,
            Action::ShieldedHostWithdrawal(_) => 1,
            Action::IbcRelay(_)
            | Action::ComplianceRegisterAsset(_)
            | Action::ComplianceRegisterUser(_)
            | Action::AggregateBundle(_) => 0,
        })
        .sum::<usize>();

    let fee_outputs = tx
        .transaction_body()
        .fee_funding
        .map(|fee_funding| fee_funding.transfer.body.outputs.len())
        .unwrap_or_default();

    action_outputs + fee_outputs
}

#[tracing::instrument(skip(tx))]
pub(crate) fn valid_binding_signature(tx: &Transaction) -> Result<()> {
    let auth_hash = tx.auth_hash();
    let bvk = tx.binding_verification_key();

    tracing::debug!(?bvk, ?auth_hash);

    if bvk.is_identity() {
        // Shielded proof actions carry ordered spend-authorization bytes in the
        // transaction body. Their aggregate binding signature must authenticate
        // that exact ordering, even when individual action balance commitments
        // happen to cancel to identity.
        anyhow::ensure!(
            tx.num_proofs() == 0,
            "transactions containing shielded proofs require a nonidentity binding verification key"
        );
        // Identity is reserved for transactions with no shielded proof action.
        anyhow::ensure!(
            is_no_binding_signature(tx.binding_sig()),
            "identity binding verification key requires the canonical no-binding signature"
        );
        Ok(())
    } else {
        bvk.verify(auth_hash.as_bytes(), tx.binding_sig())
            .context("binding signature failed to verify")
    }
}

#[allow(clippy::if_same_then_else)]
pub fn check_memo_exists_if_outputs_absent_if_not(tx: &Transaction) -> anyhow::Result<()> {
    let num_outputs = note_creating_output_count(tx);
    if num_outputs > 0 && tx.transaction_body().memo.is_none() {
        Err(anyhow::anyhow!(
            "consensus rule violated: must have memo if outputs present"
        ))
    } else if num_outputs > 0 && tx.transaction_body().memo.is_some() {
        Ok(())
    } else if num_outputs == 0 && tx.transaction_body().memo.is_none() {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "consensus rule violated: cannot have memo if no outputs present"
        ))
    }
}

pub fn check_non_empty_transaction(tx: &Transaction) -> anyhow::Result<()> {
    let num_actions = tx.actions().count();
    if num_actions > 0 {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "consensus rule violated: transaction must have more than 0 actions"
        ))
    }
}

#[cfg(test)]
mod tests {
    use decaf377::{Fq, Fr};
    use decaf377_rdsa::{Binding, SigningKey, SpendAuth, VerificationKey};
    use rand_core::OsRng;
    use shieldd_sdk_asset::Balance;
    use shieldd_sdk_sct::Nullifier;
    use shieldd_sdk_shielded_pool::{
        EncryptedBackref, Transfer, TransferBody, TransferInputBody, TransferProof,
    };
    use shieldd_sdk_tct::StateCommitment;
    use shieldd_sdk_txhash::{AuthorizingData as _, EffectingData as _};

    use super::*;

    fn transaction_with_binding_blinding(blinding: Fr) -> Transaction {
        let mut tx = Transaction::default();
        tx.transaction_body.actions.push(Action::Transfer(Transfer {
            body: TransferBody {
                anchor: tx.anchor,
                balance_commitment: Balance::default().commit(blinding),
                inputs: Vec::new(),
                outputs: Vec::new(),
                target_timestamp: 0,
                compliance_anchor: StateCommitment(decaf377::Fq::from(0u64)),
                asset_anchor: StateCommitment(decaf377::Fq::from(0u64)),
                routing: Default::default(),
                routing_parameter_set_id: Fq::from(0u64),
                volume_accumulator:
                    shieldd_sdk_shielded_pool::VolumeAccumulatorPayload::canonical_fee_funding(),
                proof_context: shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
            },
            auth_sigs: Vec::new(),
            proof: TransferProof::default(),
        }));
        tx
    }

    #[test]
    fn binding_signature_rejects_wrong_authorization_hash() {
        let binding_blinding = Fr::from(7u64);
        let mut tx = transaction_with_binding_blinding(binding_blinding);
        let signing_key = SigningKey::<Binding>::from(binding_blinding);

        let inverse_error = valid_binding_signature(&tx)
            .expect_err("a nonidentity binding key must reject the no-binding placeholder");
        assert!(
            inverse_error
                .to_string()
                .contains("binding signature failed to verify"),
            "unexpected rejection reason: {inverse_error:#}"
        );

        tx.binding_sig = signing_key.sign_deterministic(tx.auth_hash().as_bytes());
        valid_binding_signature(&tx)
            .expect("binding signature must accept its exact authorization hash");

        tx.transaction_body.transaction_parameters.expiry_height = 1;
        let error = valid_binding_signature(&tx)
            .expect_err("binding signature must reject a mutated transaction");
        assert!(
            error
                .to_string()
                .contains("binding signature failed to verify"),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn identity_binding_key_requires_canonical_no_binding_signature() {
        let mut tx = Transaction::default();
        assert!(tx.binding_verification_key().is_identity());
        valid_binding_signature(&tx)
            .expect("identity binding mode must accept its canonical placeholder");

        tx.transaction_body.transaction_parameters.expiry_height = 1;
        valid_binding_signature(&tx).expect(
            "identity binding mode carries no message authorization; action checks provide it",
        );

        let zero_key = SigningKey::<Binding>::from(Fr::from(0u64));
        tx.binding_sig = zero_key.sign_deterministic(tx.auth_hash().as_bytes());
        let error = valid_binding_signature(&tx)
            .expect_err("identity binding mode must reject a message-dependent RDSA encoding");
        assert!(
            error.to_string().contains(
                "identity binding verification key requires the canonical no-binding signature"
            ),
            "unexpected rejection reason: {error:#}"
        );
    }

    #[test]
    fn proof_bearing_transaction_rejects_identity_binding_key_signature_permutation() {
        let signing_key = SigningKey::<SpendAuth>::from(Fr::from(9u64));
        let rk = VerificationKey::from(&signing_key);
        let mut tx = Transaction::default();
        tx.transaction_body.actions.push(Action::Transfer(Transfer {
            body: TransferBody {
                anchor: tx.anchor,
                balance_commitment: Balance::default().commit(Fr::from(0u64)),
                inputs: vec![
                    TransferInputBody {
                        nullifier: Nullifier(Fq::from(1u64)),
                        rk,
                        encrypted_backref: EncryptedBackref::dummy(),
                        compliance_ciphertext: Vec::new(),
                        history_required: false,
                    },
                    TransferInputBody {
                        nullifier: Nullifier(Fq::from(2u64)),
                        rk,
                        encrypted_backref: EncryptedBackref::dummy(),
                        compliance_ciphertext: Vec::new(),
                        history_required: false,
                    },
                ],
                outputs: Vec::new(),
                target_timestamp: 0,
                compliance_anchor: StateCommitment(Fq::from(0u64)),
                asset_anchor: StateCommitment(Fq::from(0u64)),
                routing: Default::default(),
                routing_parameter_set_id: Fq::from(0u64),
                volume_accumulator:
                    shieldd_sdk_shielded_pool::VolumeAccumulatorPayload::canonical_fee_funding(),
                proof_context: shieldd_sdk_shielded_pool::TransferProofContext::Ordinary,
            },
            auth_sigs: Vec::new(),
            proof: TransferProof::default(),
        }));
        assert!(
            is_no_binding_signature(tx.binding_sig()),
            "the identity aggregate binding branch uses the canonical no-binding sentinel"
        );

        let effect_hash = tx.effect_hash();
        let first = signing_key.sign(&mut OsRng, effect_hash.as_ref());
        let second = signing_key.sign(&mut OsRng, effect_hash.as_ref());
        assert_ne!(
            first.to_bytes(),
            second.to_bytes(),
            "the regression needs two distinct interchangeable signatures"
        );
        let Action::Transfer(transfer) = &mut tx.transaction_body.actions[0] else {
            unreachable!("test constructed a Transfer")
        };
        transfer.auth_sigs = vec![first, second];
        for signature in &transfer.auth_sigs {
            rk.verify(effect_hash.as_ref(), signature)
                .expect("both original slots verify under the shared randomized key");
        }
        let original_auth_hash = tx.auth_hash();
        let original_id = tx.id();

        let Action::Transfer(transfer) = &mut tx.transaction_body.actions[0] else {
            unreachable!("test constructed a Transfer")
        };
        transfer.auth_sigs.swap(0, 1);
        for signature in &transfer.auth_sigs {
            rk.verify(effect_hash.as_ref(), signature)
                .expect("permuted signatures remain valid under the shared randomized key");
        }
        assert_ne!(
            original_auth_hash,
            tx.auth_hash(),
            "ordered spend authorizations must be part of the authorization hash"
        );
        assert_ne!(
            original_id,
            tx.id(),
            "signature permutation must change the transaction identifier"
        );

        let error = valid_binding_signature(&tx)
            .expect_err("proof-bearing transactions must not use identity aggregate binding mode");
        assert!(
            error.to_string().contains(
                "transactions containing shielded proofs require a nonidentity binding verification key"
            ),
            "unexpected rejection reason: {error:#}"
        );

        let binding_blinding = Fr::from(7u64);
        let binding_signing_key = SigningKey::<Binding>::from(binding_blinding);
        let Action::Transfer(transfer) = &mut tx.transaction_body.actions[0] else {
            unreachable!("test constructed a Transfer")
        };
        transfer.body.balance_commitment = Balance::default().commit(binding_blinding);
        let bound_effect_hash = tx.effect_hash();
        let bound_first = signing_key.sign(&mut OsRng, bound_effect_hash.as_ref());
        let bound_second = signing_key.sign(&mut OsRng, bound_effect_hash.as_ref());
        let Action::Transfer(transfer) = &mut tx.transaction_body.actions[0] else {
            unreachable!("test constructed a Transfer")
        };
        transfer.auth_sigs = vec![bound_first, bound_second];
        tx.binding_sig = binding_signing_key.sign_deterministic(tx.auth_hash().as_bytes());
        valid_binding_signature(&tx)
            .expect("a nonidentity binding key authenticates the original signature ordering");

        let Action::Transfer(transfer) = &mut tx.transaction_body.actions[0] else {
            unreachable!("test constructed a Transfer")
        };
        transfer.auth_sigs.swap(0, 1);
        let bound_error = valid_binding_signature(&tx)
            .expect_err("the binding signature must reject a spend-signature permutation");
        assert!(
            bound_error
                .to_string()
                .contains("binding signature failed to verify"),
            "unexpected rejection reason: {bound_error:#}"
        );
    }
}
