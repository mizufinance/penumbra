pub const PADDED_TRANSFER_INPUTS: usize = 2;
pub const PADDED_TRANSFER_OUTPUTS: usize = 2;
pub const TRANSFER_STATEMENT_FIELD_COUNT: usize =
    crate::public_input_hash::transfer_statement_field_count(
        PADDED_TRANSFER_INPUTS,
        PADDED_TRANSFER_OUTPUTS,
    );
pub const TRANSFER_PROOF_LABEL: &str = "transfer";
pub const TRANSFER_ARTIFACT_NAME: &str = "transfer";

pub fn transfer_input_count() -> usize {
    PADDED_TRANSFER_INPUTS
}

pub fn transfer_output_count() -> usize {
    PADDED_TRANSFER_OUTPUTS
}

pub fn transfer_auth_sig_count() -> usize {
    PADDED_TRANSFER_INPUTS
}

#[cfg(test)]
mod tests {
    use super::{
        transfer_auth_sig_count, transfer_input_count, transfer_output_count,
        TRANSFER_ARTIFACT_NAME, TRANSFER_PROOF_LABEL, TRANSFER_STATEMENT_FIELD_COUNT,
    };

    #[test]
    fn transfer_shape_is_fixed() {
        assert_eq!(transfer_input_count(), 2);
        assert_eq!(transfer_output_count(), 2);
        assert_eq!(transfer_auth_sig_count(), 2);
        assert_eq!(
            TRANSFER_STATEMENT_FIELD_COUNT,
            crate::public_input_hash::transfer_statement_field_count(2, 2)
        );
        assert_eq!(TRANSFER_PROOF_LABEL, "transfer");
        assert_eq!(TRANSFER_ARTIFACT_NAME, "transfer");
    }
}
