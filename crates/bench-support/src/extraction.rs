#[derive(Clone, Debug, Default)]
pub struct SpendOutputExtractionProfile {
    pub binding_sig_ms: f64,
    pub spend_auth_sig_ms: f64,
    pub spend_extract_ms: f64,
    pub output_extract_ms: f64,
    pub spend_ciphertext_parse_ms: f64,
    pub spend_dleq_parse_ms: f64,
    pub spend_to_batch_item_ms: f64,
    pub output_ciphertext_parse_ms: f64,
    pub output_dleq_parse_ms: f64,
    pub output_to_batch_item_ms: f64,
}

impl SpendOutputExtractionProfile {
    pub fn finalize(&mut self) {
        // Totals are populated directly by the profiled extraction helper.
    }

    pub fn total_extract_ms(&self) -> f64 {
        self.binding_sig_ms
            + self.spend_auth_sig_ms
            + self.spend_extract_ms
            + self.output_extract_ms
    }
}
