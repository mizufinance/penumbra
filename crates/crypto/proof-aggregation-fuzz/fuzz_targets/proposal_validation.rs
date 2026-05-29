#![no_main]

mod common;

use std::sync::{Arc, OnceLock};

use libfuzzer_sys::fuzz_target;
use penumbra_sdk_app::app::App;

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

fuzz_target!(|data: &[u8]| {
    let bundle = common::bundle_from_bytes(data);
    let tx = Arc::new(common::aggregate_bundle_tx(data));
    let total_proof_count = data.first().copied().unwrap_or(0) as usize % 2;
    let artifact = common::artifact_from_tx(tx, total_proof_count);
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("fuzz runtime")
    });
    let _ = runtime.block_on(App::verify_aggregate_bundle_for_artifacts_raw_public(
        &[artifact],
        &bundle,
        None,
    ));
});
