pub struct GeneratedTransferFamily {
    pub label: &'static str,
    pub artifact_name: &'static str,
    pub n_in: usize,
    pub n_out: usize,
    pub bundled_lib_basename: &'static str,
}

pub const GENERATED_TRANSFER_FAMILIES: &[GeneratedTransferFamily] = &[GeneratedTransferFamily {
    label: "transfer",
    artifact_name: "transfer",
    n_in: 2,
    n_out: 2,
    bundled_lib_basename: "libpenumbra_gnark_transfer",
}];
