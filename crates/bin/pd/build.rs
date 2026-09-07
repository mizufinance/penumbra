use std::io::Read;
use std::path::Path;

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    check_frontend_asset_zipfiles()?;
    Ok(())
}

// Check that the zip files for bundled frontend code are functional.
fn check_frontend_asset_zipfiles() -> anyhow::Result<()> {
    const MINIMUM_FILESIZE_BYTES: usize = 500;
    let zipfiles = vec![
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/minifront.zip"),
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/node-status.zip"),
    ];
    for zipfile in zipfiles {
        let mut bytes = Vec::new();
        let f = std::fs::File::open(&zipfile).context(format!(
            "failed to open zip file of frontend code: {}",
            &zipfile.display()
        ))?;
        let mut reader = std::io::BufReader::new(f);
        reader.read_to_end(&mut bytes).context(format!(
            "failed to read zip file of frontend code: {}",
            zipfile.display()
        ))?;
        if bytes.len() < MINIMUM_FILESIZE_BYTES {
            anyhow::bail!(
                format!(
                    "asset zip file {} is smaller than {} bytes; restore the repository asset and retry the build",
                    zipfile.display(),
                    MINIMUM_FILESIZE_BYTES
                )
                );
        }
    }
    Ok(())
}
