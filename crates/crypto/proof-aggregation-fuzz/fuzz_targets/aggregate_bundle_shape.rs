#![no_main]

mod common;

use libfuzzer_sys::fuzz_target;
use penumbra_sdk_app::app::App;

fuzz_target!(|data: &[u8]| {
    let tx = common::aggregate_bundle_tx(data);
    let _ = App::ensure_aggregate_bundle_tx_shape_for_fuzz(&tx);
});
