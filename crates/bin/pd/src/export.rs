use anyhow::Context;
use flate2::{write::GzEncoder, Compression};
use std::{fs::File, path::PathBuf};

/// Compress exported node state to a gzipped tar archive.
pub fn archive_directory(src_directory: PathBuf, archive_filepath: PathBuf) -> anyhow::Result<()> {
    // Don't clobber an existing target archive.
    if archive_filepath.exists() {
        tracing::error!(
            "export archive filepath already exists: {}",
            archive_filepath.display()
        );
        anyhow::bail!("refusing to overwrite existing archive");
    }

    tracing::info!(
        "creating archive {} -> {}",
        src_directory.display(),
        archive_filepath.display()
    );
    let tarball_file = File::create(&archive_filepath)
        .context("failed to create file for archive: check parent directory and permissions")?;
    let enc = GzEncoder::new(tarball_file, Compression::default());
    let mut tarball = tar::Builder::new(enc);
    tarball
        .append_dir_all(".", src_directory.as_path())
        .context("failed to package archive contents")?;
    Ok(())
}
