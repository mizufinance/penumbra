#![no_main]
risc0_zkvm::guest::entry!(main);

fn main() {
    use std::io::Read;
    let mut bytes = Vec::new();
    risc0_zkvm::guest::env::stdin()
        .take(shieldd_sdk_disclosure::MAX_WITNESS_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .expect("cannot read disclosure witness");
    let witness =
        shieldd_sdk_disclosure::decode_witness(&bytes).expect("invalid disclosure witness");
    let statement = shieldd_sdk_disclosure::evaluate(&witness).expect("invalid disclosure claim");
    risc0_zkvm::guest::env::commit_slice(&serde_json::to_vec(&statement).unwrap());
}
