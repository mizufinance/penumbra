#![allow(dead_code)]

use std::time::Instant;

use anyhow::Result;
use penumbra_sdk_bench_support::extraction::SpendOutputExtractionProfile;
use penumbra_sdk_proof_params::batch::BatchItem;
use penumbra_sdk_shielded_pool::component::{
    output_build_public, output_check_stateless_and_extract, output_parse_ciphertext_fields,
    output_parse_dleq_fields, output_to_batch_item, spend_build_public,
    spend_check_stateless_and_extract, spend_parse_ciphertext_fields, spend_parse_dleq_fields,
    spend_to_batch_item, spend_verify_auth_sig,
};
use penumbra_sdk_transaction::txhash::AuthorizingData;
use penumbra_sdk_transaction::{Action, Transaction};

#[derive(Clone)]
pub struct ExtractedProofItems {
    pub spend_items: Vec<BatchItem>,
    pub output_items: Vec<BatchItem>,
}

#[derive(Clone)]
pub struct ProfiledExtractedProofItems {
    pub spend_items: Vec<BatchItem>,
    pub output_items: Vec<BatchItem>,
    pub profile: SpendOutputExtractionProfile,
}

pub fn verify_binding_sig(tx: &Transaction) -> Result<()> {
    let auth_hash = tx.auth_hash();
    tx.binding_verification_key()
        .verify(auth_hash.as_bytes(), tx.binding_sig())
        .map_err(|e| anyhow::anyhow!("binding signature should verify: {e}"))
}

pub fn extract_proof_items(txs: &[Transaction]) -> Result<ExtractedProofItems> {
    let mut spend_items = Vec::new();
    let mut output_items = Vec::new();

    for tx in txs {
        verify_binding_sig(tx)?;
        let context = tx.context();

        for action in tx.actions() {
            match action {
                Action::Spend(spend) => {
                    spend_items.push(spend_check_stateless_and_extract(spend, &context)?);
                }
                Action::Output(output) => {
                    output_items.push(output_check_stateless_and_extract(output)?);
                }
                _ => {}
            }
        }
    }

    Ok(ExtractedProofItems {
        spend_items,
        output_items,
    })
}

pub fn extract_proof_items_profiled(txs: &[Transaction]) -> Result<ProfiledExtractedProofItems> {
    let mut spend_items = Vec::new();
    let mut output_items = Vec::new();
    let mut profile = SpendOutputExtractionProfile::default();

    for tx in txs {
        let start = Instant::now();
        verify_binding_sig(tx)?;
        profile.binding_sig_ms += start.elapsed().as_secs_f64() * 1000.0;

        let context = tx.context();
        for action in tx.actions() {
            match action {
                Action::Spend(spend) => {
                    let spend_extract_start = Instant::now();

                    let start = Instant::now();
                    spend_verify_auth_sig(spend, &context)?;
                    profile.spend_auth_sig_ms += start.elapsed().as_secs_f64() * 1000.0;

                    let ciphertext = spend_parse_ciphertext_fields(spend)?;
                    let dleq = spend_parse_dleq_fields(spend)?;

                    let public = spend_build_public(spend, &context, ciphertext, dleq);

                    let start = Instant::now();
                    let item = spend_to_batch_item(spend, public)?;
                    profile.spend_to_batch_item_ms += start.elapsed().as_secs_f64() * 1000.0;

                    spend_items.push(item);
                    profile.spend_extract_ms +=
                        spend_extract_start.elapsed().as_secs_f64() * 1000.0;
                }
                Action::Output(output) => {
                    let output_extract_start = Instant::now();

                    let start = Instant::now();
                    let ciphertext = output_parse_ciphertext_fields(output)?;
                    profile.output_ciphertext_parse_ms += start.elapsed().as_secs_f64() * 1000.0;

                    let dleq = output_parse_dleq_fields(output)?;

                    let public = output_build_public(output, ciphertext, dleq);

                    let start = Instant::now();
                    let item = output_to_batch_item(output, public)?;
                    profile.output_to_batch_item_ms += start.elapsed().as_secs_f64() * 1000.0;

                    output_items.push(item);
                    profile.output_extract_ms +=
                        output_extract_start.elapsed().as_secs_f64() * 1000.0;
                }
                _ => {}
            }
        }
    }

    Ok(ProfiledExtractedProofItems {
        spend_items,
        output_items,
        profile,
    })
}
