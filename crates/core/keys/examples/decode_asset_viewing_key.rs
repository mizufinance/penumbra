use penumbra_sdk_keys::keys::AssetViewingKey;

// cargo run --quiet --example decode_asset_viewing_key

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let key_str = if args.len() > 1 {
        &args[1]
    } else {
        "penumbraassetviewingkey14h46dmcyy6fl5vyz7xx933n8l7jy202ahrxg92kad6yfladsvcyd936f2engzk0tp4alv7pewr2tsckmjrzdl2c0euntkm96m6efgqz093n4sqesd86zced8r7mhkmc6w2l465"
    };

    match key_str.parse::<AssetViewingKey>() {
        Ok(asset_key) => {
            println!("✓ Successfully decoded AssetViewingKey!\n");
            println!("Asset ID: {}\n", asset_key.asset_id());

            let bytes = asset_key.to_bytes();

            println!("Raw hex components:");
            println!("  Asset ID (32 bytes): {}", hex::encode(&bytes[0..32]));
            println!("  IVK (32 bytes):      {}", hex::encode(&bytes[32..64]));
            println!("  DK (16 bytes):       {}", hex::encode(&bytes[64..80]));
            println!("\nFormat: 80 bytes total");
            println!("  - bytes[0..32]:  Asset ID");
            println!("  - bytes[32..64]: Incoming Viewing Key (IVK) scalar");
            println!("  - bytes[64..80]: Diversifier Key (DK)");
            println!("\nKey capabilities:");
            println!("  ✓ Can decrypt notes at ANY address derived from the original FVK");
            println!("  ✓ Can only VIEW transactions for the specified asset (test_usd)");
            println!("  ✗ Cannot derive Full Viewing Key (missing OVK)");
            println!("  ✗ Cannot spend funds (no spend authority)");
        }
        Err(e) => {
            eprintln!("Error decoding AssetViewingKey: {}", e);
            std::process::exit(1);
        }
    }
}
