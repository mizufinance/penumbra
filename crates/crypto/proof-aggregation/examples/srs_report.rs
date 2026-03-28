use penumbra_sdk_proof_aggregation::{srs_id, srs_report, DevSrs};

fn main() -> anyhow::Result<()> {
    let srs = DevSrs::default();
    let report = srs_report(&srs)?;
    let srs_id_hex = hex::encode(srs_id(&srs));

    println!(
        "backend: {}",
        penumbra_sdk_proof_aggregation::DEV_SRS_BACKEND_ID
    );
    println!(
        "curve: {}",
        penumbra_sdk_proof_aggregation::DEV_SRS_CURVE_ID
    );
    println!("max_padded_count: {}", srs.max_padded_count);
    println!("generation_ms: {}", report.generation_ms);
    println!("load_ms: {}", report.load_ms);
    println!("compressed_bytes: {}", report.compressed_bytes);
    println!("uncompressed_bytes: {}", report.uncompressed_bytes);
    println!("srs_id: {}", srs_id_hex);

    Ok(())
}
