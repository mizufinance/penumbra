use core::fmt::Debug;
use penumbra_sdk_proof_params::{GNARK_SPEND_CIRCUIT_METADATA, SPEND_PROOF_VERIFICATION_KEY};
use std::{
    fs::{self, OpenOptions},
    io::Write,
};

fn print_to_file<T: Debug>(data: &T, filename: &str) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(filename)?;
    writeln!(file, "{:#?}", data)?;
    Ok(())
}

#[test]
fn spend_debug() {
    let _ = fs::remove_file("spend_proof.txt");

    print_to_file(&GNARK_SPEND_CIRCUIT_METADATA.len(), "spend_proof.txt")
        .expect("Failed to write bundled metadata size");

    let vk = &*SPEND_PROOF_VERIFICATION_KEY;
    print_to_file(vk, "spend_proof.txt").expect("Failed to write verification key");
}
