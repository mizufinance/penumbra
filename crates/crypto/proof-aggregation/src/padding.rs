use anyhow::{anyhow, ensure, Result};
use decaf377::Fq;
use penumbra_sdk_proof_params::batch::BatchItem;

pub const PADDING_RULE_DOMAIN: &[u8] = b"penumbra.snarkpack.padding.repeat-final-row.v1\0";

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
    if items.is_empty() {
        return Ok(PreparedVerifyInputs {
            real_count: 0,
            padded_count: 0,
            padded_public_inputs: Vec::new(),
        });
    }

    let padded_count = items.len().next_power_of_two();
    ensure!(
        padded_count <= max_padded_count,
        "padded proof count {padded_count} exceeds max {max_padded_count}"
    );

    let mut padded_public_inputs = items
        .iter()
        .map(|item| item.public_inputs.clone())
        .collect::<Vec<_>>();
    let last = padded_public_inputs
        .last()
        .cloned()
        .ok_or_else(|| anyhow!("missing final public inputs for deterministic padding"))?;
    while padded_public_inputs.len() < padded_count {
        padded_public_inputs.push(last.clone());
    }

    Ok(PreparedVerifyInputs {
        real_count: items.len(),
        padded_count,
        padded_public_inputs,
    })
}

#[cfg(test)]
mod tests {
    use ark_groth16::Proof;
    use decaf377::Fq;
    use penumbra_sdk_proof_params::batch::BatchItem;

    use super::{pad_items_to_power_of_two, prepare_verify_inputs};

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
}
