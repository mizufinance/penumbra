#![forbid(unsafe_code)]

use std::path::PathBuf;

use penumbra_sdk_proof_aggregation_lean_conformance::{
    lean_oracle_shapes_for_counts, smoke_counts, write_oracle_shapes_json,
};

fn main() {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: snarkpack_lean_oracle <output-json>");
    let shapes =
        lean_oracle_shapes_for_counts(&smoke_counts()).expect("Lean oracle should execute");
    write_oracle_shapes_json(&shapes, output).expect("write Lean oracle shape JSON");
}
