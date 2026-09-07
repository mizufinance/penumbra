use anyhow::{anyhow, ensure, Result};
use decaf377::Fq;
use shieldd_sdk_proof_params::batch::BatchItem;

use crate::app_verifier::app_verify_prepare_public_input_rows_core;

pub const PADDING_RULE_DOMAIN: &[u8] = b"shieldd.snarkpack.padding.repeat-final-row\0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedVerifyInputs {
    pub real_count: usize,
    pub padded_count: usize,
    pub padded_public_inputs: Vec<Vec<Fq>>,
}

pub fn pad_items_to_power_of_two(
    items: &[BatchItem],
    max_padded_count: usize,
) -> Result<Vec<BatchItem>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let padded_count = items.len().next_power_of_two();
    ensure!(
        padded_count <= max_padded_count,
        "padded proof count {padded_count} exceeds max {max_padded_count}"
    );

    let mut padded = items.to_vec();
    let last = padded
        .last()
        .cloned()
        .ok_or_else(|| anyhow!("missing final proof for deterministic padding"))?;
    while padded.len() < padded_count {
        padded.push(last.clone());
    }
    Ok(padded)
}

pub fn prepare_verify_inputs(
    items: &[BatchItem],
    max_padded_count: usize,
) -> Result<PreparedVerifyInputs> {
    let public_inputs = items
        .iter()
        .map(|item| item.public_inputs.clone())
        .collect::<Vec<_>>();
    prepare_verify_public_input_rows(public_inputs, max_padded_count)
}

/// Pure row projection used by the shipping verifier. The input vector is the
/// caller-order real prefix and is consumed by the repeat-final padding core.
pub(crate) fn prepare_verify_public_input_rows(
    public_inputs: Vec<Vec<Fq>>,
    max_padded_count: usize,
) -> Result<PreparedVerifyInputs> {
    let padded_count = if public_inputs.is_empty() {
        0
    } else {
        public_inputs.len().next_power_of_two()
    };
    ensure!(
        padded_count <= max_padded_count,
        "padded proof count {padded_count} exceeds max {max_padded_count}"
    );

    let prepared = app_verify_prepare_public_input_rows_core(public_inputs, padded_count)
        .map_err(|_| anyhow!("missing final public inputs for deterministic padding"))?;

    Ok(PreparedVerifyInputs {
        real_count: prepared.real_count,
        padded_count: prepared.padded_count,
        padded_public_inputs: prepared.padded_public_inputs,
    })
}

#[cfg(test)]
mod tests {
    use ark_groth16::Proof;
    use decaf377::Fq;
    use shieldd_sdk_proof_params::batch::BatchItem;

    use super::{
        pad_items_to_power_of_two, prepare_verify_inputs, prepare_verify_public_input_rows,
    };

    fn dummy_item(value: u64) -> BatchItem {
        BatchItem {
            proof: Proof {
                a: Default::default(),
                b: Default::default(),
                c: Default::default(),
            },
            public_inputs: vec![Fq::from(value)],
        }
    }

    #[test]
    fn empty_input_stays_empty() {
        let padded = pad_items_to_power_of_two(&[], 8).expect("padding succeeds");
        assert!(padded.is_empty());
    }

    #[test]
    fn pads_by_repeating_last_item() {
        let items = vec![dummy_item(1), dummy_item(2), dummy_item(3)];
        let padded = pad_items_to_power_of_two(&items, 8).expect("padding succeeds");

        assert_eq!(padded.len(), 4);
        assert_eq!(padded[2].public_inputs, padded[3].public_inputs);
        assert_eq!(padded[3].public_inputs[0], 3u64.into());
    }

    #[test]
    fn rejects_oversized_padding() {
        let items = vec![dummy_item(1), dummy_item(2), dummy_item(3)];
        match pad_items_to_power_of_two(&items, 2) {
            Ok(_) => panic!("padding should reject oversized input"),
            Err(err) => assert!(err.to_string().contains("padded proof count")),
        }
    }

    #[test]
    fn prepare_verify_inputs_matches_full_padding() {
        let items = vec![dummy_item(1), dummy_item(2), dummy_item(3)];
        let padded = pad_items_to_power_of_two(&items, 8).expect("padding succeeds");
        let prepared = prepare_verify_inputs(&items, 8).expect("verify inputs prepare");

        assert_eq!(prepared.real_count, items.len());
        assert_eq!(prepared.padded_count, padded.len());
        assert_eq!(
            prepared.padded_public_inputs,
            padded
                .into_iter()
                .map(|item| item.public_inputs)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn row_projection_preserves_caller_order_before_repeat_final_suffix() {
        let source = vec![
            vec![Fq::from(1u64)],
            vec![Fq::from(2u64)],
            vec![Fq::from(3u64)],
        ];
        let prepared =
            prepare_verify_public_input_rows(source.clone(), 8).expect("row projection succeeds");

        assert_eq!(prepared.real_count, source.len());
        assert_eq!(prepared.padded_count, 4);
        assert_eq!(
            &prepared.padded_public_inputs[..source.len()],
            source.as_slice()
        );
        assert_eq!(prepared.padded_public_inputs[3], source[2]);
    }
}
