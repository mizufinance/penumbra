//! The Penumbra proving and verification key files are binary
//! data that must be provided at build time, so that the key material
//! can be injected into Rust types. The key material is too large, however,
//! for uploading to crates.io (with the keys the crate weights ~100MB).
//!
//! Instead, we'll upload just git raw git-lfs pointer files when publishing to crates.io,
//! then use the build.rs logic to fetch the assets ahead of compilation. Use the feature
//! `download-proving-keys` to enable the auto-download behavior.
use anyhow::Context;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

include!("src/gen/gnark/transfer_families_build.rs");

const TRANSFER_FAMILY_GENERATED_SOURCES: &[(&str, bool)] = &[
    (
        "tools/gnark/internal/generated/transfer_families_generated.go",
        false,
    ),
    (
        "crates/core/component/shielded-pool/src/transfer/generated.rs",
        false,
    ),
    (
        "crates/crypto/proof-params/src/gen/gnark/transfer_families_manifest.json",
        true,
    ),
    (
        "crates/crypto/proof-params/src/gen/gnark/transfer_families_build.rs",
        false,
    ),
    (
        "crates/crypto/proof-params/src/gen/gnark/transfer_registry.rs",
        false,
    ),
    (
        "crates/crypto/proof-aggregation/src/transfer_family_dispatch.rs",
        false,
    ),
];

fn main() {
    emit_transfer_family_rerun_hints().expect("emit transfer family rerun-if-changed hints");
    enforce_transfer_family_codegen_sync().expect("validate transfer family generated sources");

    let mut proving_parameter_files = vec![
        "src/gen/output_pk.bin".to_owned(),
        "src/gen/spend_pk.bin".to_owned(),
        "src/gen/swap_pk.bin".to_owned(),
        "src/gen/swapclaim_pk.bin".to_owned(),
        "src/gen/convert_pk.bin".to_owned(),
        "src/gen/delegator_vote_pk.bin".to_owned(),
        "src/gen/nullifier_derivation_pk.bin".to_owned(),
        "src/gen/gnark/spend/proving_key.bin".to_owned(),
        "src/gen/gnark/output/proving_key.bin".to_owned(),
    ];
    proving_parameter_files.extend(
        GENERATED_TRANSFER_FAMILIES
            .iter()
            .map(|family| format!("src/gen/gnark/{}/proving_key.bin", family.artifact_name)),
    );

    let mut verification_parameter_files = vec![
        "src/gen/output_vk.param".to_owned(),
        "src/gen/spend_vk.param".to_owned(),
        "src/gen/swap_vk.param".to_owned(),
        "src/gen/swapclaim_vk.param".to_owned(),
        "src/gen/convert_vk.param".to_owned(),
        "src/gen/delegator_vote_vk.param".to_owned(),
        "src/gen/nullifier_derivation_vk.param".to_owned(),
        "src/gen/gnark/spend/verifying_key.json".to_owned(),
        "src/gen/gnark/output/verifying_key.json".to_owned(),
        "src/gen/gnark/spend/circuit_metadata.json".to_owned(),
        "src/gen/gnark/output/circuit_metadata.json".to_owned(),
    ];
    verification_parameter_files.extend(GENERATED_TRANSFER_FAMILIES.iter().flat_map(|family| {
        [
            format!("src/gen/gnark/{}/verifying_key.json", family.artifact_name),
            format!(
                "src/gen/gnark/{}/circuit_metadata.json",
                family.artifact_name
            ),
        ]
    }));

    for file in proving_parameter_files
        .iter()
        .map(|file| file.as_str())
        .chain(
            verification_parameter_files
                .iter()
                .map(|file| file.as_str()),
        )
    {
        println!("cargo:rerun-if-changed={file}");
    }

    for file in &proving_parameter_files {
        handle_proving_key(file).expect("failed while handling proving keys");
    }

    write_bundled_gnark_runtime_paths().expect("failed while preparing bundled gnark runtime");
}

fn emit_transfer_family_rerun_hints() -> anyhow::Result<()> {
    let repo_root = repo_root()?;
    println!(
        "cargo:rerun-if-changed={}",
        repo_root
            .join("tools/gnark/transfer_families.json")
            .display()
    );
    for (relative_path, _) in TRANSFER_FAMILY_GENERATED_SOURCES {
        println!(
            "cargo:rerun-if-changed={}",
            repo_root.join(relative_path).display()
        );
    }
    Ok(())
}

fn enforce_transfer_family_codegen_sync() -> anyhow::Result<()> {
    let repo_root = repo_root()?;
    let manifest_path = repo_root.join("tools/gnark/transfer_families.json");
    if !manifest_path.exists() {
        return Ok(());
    }

    let manifest_bytes = fs::read(&manifest_path).with_context(|| {
        format!(
            "read transfer family manifest at {}",
            manifest_path.display()
        )
    })?;
    let expected_hash = hex::encode(Sha256::digest(&manifest_bytes));

    let mut stale_files = Vec::new();
    for (relative_path, is_json_manifest) in TRANSFER_FAMILY_GENERATED_SOURCES {
        let path = repo_root.join(relative_path);
        let actual_hash = generated_manifest_hash(&path, *is_json_manifest)?;
        if actual_hash != expected_hash {
            stale_files.push(path);
        }
    }

    if !stale_files.is_empty() {
        let stale_list = stale_files
            .iter()
            .map(|path| format!("  - {}", path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!(
            "transfer-family generated sources are stale or incomplete:\n{stale_list}\n\
             rerun:\n  cd tools/gnark && GOCACHE=/tmp/penumbra-go-cache go run ./cmd/gen-transfer-families"
        );
    }

    Ok(())
}

fn generated_manifest_hash(path: &Path, is_json_manifest: bool) -> anyhow::Result<String> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("read generated transfer-family file {}", path.display()))?;
    if is_json_manifest {
        json_manifest_hash(&contents).with_context(|| {
            format!(
                "extract manifest_sha256 from generated transfer-family json {}",
                path.display()
            )
        })
    } else {
        comment_manifest_hash(&contents).with_context(|| {
            format!(
                "extract Manifest SHA256 comment from generated transfer-family source {}",
                path.display()
            )
        })
    }
}

fn comment_manifest_hash(contents: &str) -> anyhow::Result<String> {
    const PREFIX: &str = "Manifest SHA256:";
    contents
        .lines()
        .find_map(|line| {
            line.split_once(PREFIX)
                .map(|(_, hash)| hash.trim().to_owned())
        })
        .ok_or_else(|| anyhow::anyhow!("missing `{PREFIX}` comment"))
}

fn json_manifest_hash(contents: &str) -> anyhow::Result<String> {
    const KEY: &str = "\"manifest_sha256\"";
    let (_, rest) = contents
        .split_once(KEY)
        .ok_or_else(|| anyhow::anyhow!("missing {KEY} field"))?;
    let (_, rest) = rest
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("missing colon after {KEY}"))?;
    let rest = rest.trim_start();
    let quoted = rest
        .strip_prefix('"')
        .ok_or_else(|| anyhow::anyhow!("{KEY} value is not a JSON string"))?;
    let end = quoted
        .find('"')
        .ok_or_else(|| anyhow::anyhow!("unterminated {KEY} string"))?;
    Ok(quoted[..end].to_owned())
}

fn write_bundled_gnark_runtime_paths() -> anyhow::Result<()> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").context("OUT_DIR is set by cargo")?);
    let include_path = out_dir.join("gnark_bundled.rs");

    if !cfg!(feature = "bundled-proving-keys") {
        write_empty_gnark_runtime_include(&include_path)
            .context("write empty gnark runtime include file")?;
        return Ok(());
    }

    let repo_root = repo_root()?;
    let gnark_dir = repo_root.join("tools/gnark");
    if !gnark_dir.exists() {
        anyhow::bail!(
            "bundled-proving-keys requires gnark runtime sources at {}",
            gnark_dir.display()
        );
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").context("CARGO_CFG_TARGET_OS is set")?;
    // Gnark is a native shared library — skip for wasm and other non-native targets.
    if target_os == "unknown" {
        write_empty_gnark_runtime_include(&include_path)
            .context("write empty gnark runtime include file for non-native target")?;
        return Ok(());
    }
    let target_arch =
        std::env::var("CARGO_CFG_TARGET_ARCH").context("CARGO_CFG_TARGET_ARCH is set")?;
    let goos = map_goos(&target_os)?;
    let goarch = map_goarch(&target_arch)?;
    let lib_ext = shared_lib_extension(&target_os)?;

    let gnark_out_dir = out_dir
        .join("gnark")
        .join(format!("{target_os}-{target_arch}"));
    std::fs::create_dir_all(&gnark_out_dir).context("create bundled gnark output directory")?;

    let spend_lib_path = gnark_out_dir.join(format!("libpenumbra_gnark_spend.{lib_ext}"));
    let output_lib_path = gnark_out_dir.join(format!("libpenumbra_gnark_output.{lib_ext}"));
    let transfer_lib_path = gnark_out_dir.join(format!("libpenumbra_gnark_transfer.{lib_ext}"));

    build_gnark_library(&gnark_dir, "./cmd/spendlib", &spend_lib_path, goos, goarch)
        .context("build bundled gnark spend library")?;
    build_gnark_library(
        &gnark_dir,
        "./cmd/outputlib",
        &output_lib_path,
        goos,
        goarch,
    )
    .context("build bundled gnark output library")?;
    build_gnark_library(
        &gnark_dir,
        "./cmd/transferlib",
        &transfer_lib_path,
        goos,
        goarch,
    )
    .context("build bundled gnark transfer library")?;

    let include_body = format!(
        "pub const GNARK_SPEND_BUNDLED_LIBRARY_PATH: Option<&str> = Some(r#\"{}\"#);\n\
         pub const GNARK_OUTPUT_BUNDLED_LIBRARY_PATH: Option<&str> = Some(r#\"{}\"#);\n\
         pub const GNARK_TRANSFER_BUNDLED_LIBRARY_PATH: Option<&str> = Some(r#\"{}\"#);\n",
        spend_lib_path.display(),
        output_lib_path.display(),
        transfer_lib_path.display(),
    );
    let _ = GENERATED_TRANSFER_FAMILIES;
    std::fs::write(&include_path, include_body).context("write gnark runtime include file")?;

    Ok(())
}

fn write_empty_gnark_runtime_include(include_path: &Path) -> anyhow::Result<()> {
    let include_body = String::from(
        "pub const GNARK_SPEND_BUNDLED_LIBRARY_PATH: Option<&str> = None;\n\
         pub const GNARK_OUTPUT_BUNDLED_LIBRARY_PATH: Option<&str> = None;\n\
         pub const GNARK_TRANSFER_BUNDLED_LIBRARY_PATH: Option<&str> = None;\n",
    );
    let _ = GENERATED_TRANSFER_FAMILIES;
    std::fs::write(include_path, include_body)?;
    Ok(())
}

fn repo_root() -> anyhow::Result<PathBuf> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR is set")?);
    manifest_dir
        .join("../../..")
        .canonicalize()
        .context("resolve repository root from proof-params crate")
}

fn map_goos(target_os: &str) -> anyhow::Result<&'static str> {
    match target_os {
        "macos" => Ok("darwin"),
        "linux" => Ok("linux"),
        "windows" => Ok("windows"),
        other => anyhow::bail!("unsupported target OS for bundled gnark runtime: {other}"),
    }
}

fn map_goarch(target_arch: &str) -> anyhow::Result<&'static str> {
    match target_arch {
        "x86_64" => Ok("amd64"),
        "aarch64" => Ok("arm64"),
        other => {
            anyhow::bail!("unsupported target architecture for bundled gnark runtime: {other}")
        }
    }
}

fn shared_lib_extension(target_os: &str) -> anyhow::Result<&'static str> {
    match target_os {
        "macos" => Ok("dylib"),
        "linux" => Ok("so"),
        "windows" => Ok("dll"),
        other => anyhow::bail!("unsupported shared library target OS: {other}"),
    }
}

fn build_gnark_library(
    gnark_dir: &Path,
    package: &str,
    output_path: &Path,
    goos: &str,
    goarch: &str,
) -> anyhow::Result<()> {
    let mut command = Command::new("go");
    command
        .current_dir(gnark_dir)
        .env("CGO_ENABLED", "1")
        .env("GOOS", goos)
        .env("GOARCH", goarch)
        .arg("build")
        .arg("-buildmode=c-shared")
        .arg("-o")
        .arg(output_path)
        .arg(package);

    let output = command.output().with_context(|| {
        format!("run `go build` for bundled gnark runtime (install Go to use bundled-proving-keys)")
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!("go build failed for {package}:\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }

    Ok(())
}

/// Inspect keyfiles, to figure out whether they're git-lfs pointers.
/// If so, and if the `download-proving-keys` feature is set, then fetch
/// the key material over the network via Github API. Otherwise, error
/// out with an informative message.
fn handle_proving_key(file: &str) -> anyhow::Result<()> {
    let r = ProvingKeyFilepath::new(file);
    match r {
        ProvingKeyFilepath::Present(_f) => {}
        ProvingKeyFilepath::Absent(f) => {
            println!(
                "cargo:warning=proving key file is missing: {} this should not happen",
                f
            );
            anyhow::bail!(
                "proving key file not found; at least lfs pointers were expected; path={}",
                f
            );
        }
        ProvingKeyFilepath::Pointer(f) => {
            #[cfg(feature = "download-proving-keys")]
            download_proving_key(&f)?;
            #[cfg(not(feature = "download-proving-keys"))]
            println!(
                "cargo:warning=proving key file is lfs pointer: {} enable 'download-proving-keys' feature to obtain key files",
                f
            );
        }
    }
    Ok(())
}

/// The states that a proving key filepath can be in.
enum ProvingKeyFilepath {
    /// The filepath does not exist.
    ///
    /// `Absent` is the expected state when building from crates.io,
    /// because the binary keyfiles are excluded from the crate manifest, due to filesize.
    /// If the keyfiles were bundled into the crate, it'd be ~100MB, far too large for crates.io.
    Absent(String),

    /// The filepath was found, but appears to be a git-lfs pointer.
    ///
    /// `Pointer` is the expected state when:
    ///
    ///   * building from source, via a local git checkout, but without git-lfs being configured;
    ///   * building from crates.io, because only the git-lfs pointers were uploaded
    ///
    /// If the `download-proving-keys` feature is set, then the proving keys will be fetched
    /// via the Github LFS API and written in place in the source checkout. Otherwise,
    /// an error is thrown.
    Pointer(String),

    /// The filepath was found, and appears to be a fully-fleged binary key file.
    ///
    /// `Present` is the expected state when building from source, via a local git checkout,
    /// with git-lfs properly configured.
    Present(String),
}

impl ProvingKeyFilepath {
    fn new(filepath: &str) -> Self {
        if std::fs::metadata(filepath).is_ok() {
            let bytes = file_to_bytes(filepath).expect("failed to read filepath as bytes");
            // If the file is smaller than 500 bytes, we'll assume it's an LFS pointer.
            if bytes.len() < 500 {
                ProvingKeyFilepath::Pointer(filepath.into())
            } else {
                ProvingKeyFilepath::Present(filepath.into())
            }
        } else {
            ProvingKeyFilepath::Absent(filepath.into())
        }
    }
}

/// Read filepath to byte array.
fn file_to_bytes(filepath: &str) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let f = std::fs::File::open(filepath)
        .with_context(|| "can open proving key file from local source")?;
    let mut reader = std::io::BufReader::new(f);
    reader
        .read_to_end(&mut bytes)
        .with_context(|| "can read proving key file")?;
    Ok(bytes)
}

#[cfg(feature = "download-proving-keys")]
pub fn download_proving_key(filepath: &str) -> anyhow::Result<()> {
    use std::io::Write;

    let bytes = file_to_bytes(filepath)?;
    let pointer =
        downloads::GitLFSPointer::parse(&bytes[..]).with_context(|| "can parse pointer")?;
    let downloaded_bytes = pointer
        .resolve()
        .with_context(|| "can download proving key from git-lfs")?;

    // Save downloaded bytes to file.
    let f =
        std::fs::File::create(filepath).with_context(|| "can open downloaded proving key file")?;
    let mut writer = std::io::BufWriter::new(f);
    writer
        .write_all(&downloaded_bytes[..])
        .with_context(|| "can write downloaded proving key to local file")?;
    Ok(())
}

#[cfg(feature = "download-proving-keys")]
mod downloads {
    use anyhow::Context;
    use regex::Regex;
    use reqwest::blocking::Client;

    /// The Git LFS server to use.
    static GIT_LFS_SERVER: &str =
        "https://github.com/penumbra-zone/penumbra.git/info/lfs/objects/batch";

    /// Represents a Git LFS pointer.
    pub struct GitLFSPointer {
        /// The unique object ID.
        oid: String,
        /// The hash algorithm used to compute the OID. Only `sha256` is supported.
        hash_algo: String,
        /// The size of the object in bytes.
        size: usize,
    }

    impl GitLFSPointer {
        /// Parses a Git LFS pointer from raw bytes.
        pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
            let pointer_utf8 =
                std::str::from_utf8(bytes).with_context(|| "git LFS should be valid UTF-8")?;

            // `oid sha256:digest`
            let oid_re = Regex::new(r"oid [\w,:]*").unwrap();
            let caps = oid_re
                .captures(pointer_utf8)
                .with_context(|| "git LFS pointers should have oid field")?;
            let oid_line: Vec<String> = caps
                .get(0)
                .with_context(|| "hash algorithm should be in oid field")?
                .as_str()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            let hash_and_oid: Vec<String> = oid_line[1].split(':').map(str::to_owned).collect();
            let hash_algo = hash_and_oid[0].clone();
            let oid = hash_and_oid[1].clone();

            // `size 12345`
            let size_re = Regex::new(r"size [0-9]*").unwrap();
            let caps = size_re
                .captures(pointer_utf8)
                .with_context(|| "git LFS pointers have size field")?;
            let size_line: Vec<String> = caps
                .get(0)
                .with_context(|| "size in bytes should be in git LFS pointer")?
                .as_str()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            let size = size_line[1]
                .parse()
                .with_context(|| "size should be a number")?;

            Ok(Self {
                oid,
                hash_algo,
                size,
            })
        }

        /// Resolves the pointer using the Git LFS Batch API.
        /// https://github.com/git-lfs/git-lfs/blob/main/docs/api/batch.md
        pub fn resolve(&self) -> anyhow::Result<Vec<u8>> {
            // Download using Git LFS Batch API
            let request_body = format!(
                r#"{{"operation": "download", "transfer": ["basic"], "objects": [{{"oid": "{}", "size": {}}}]}}"#,
                self.oid, self.size
            );
            let client = Client::new();
            let res = client
                .post(GIT_LFS_SERVER)
                .header("Accept", "application/vnd.git-lfs+json")
                .header("Content-type", "application/vnd.git-lfs+json")
                .body(request_body)
                .send()
                .with_context(|| "can get response from Git LFS server")?;

            // JSON response contains "objects" array -> 0 -> "actions" -> "download" -> "href" which has the
            // actual location of the file.
            let json_res = res
                .json::<serde_json::Value>()
                .with_context(|| "result is JSON formatted")?;

            let href = json_res
                .get("objects")
                .with_context(|| "objects key exists")?
                .get(0)
                .with_context(|| "has at least one entry")?
                .get("actions")
                .with_context(|| "has actions key")?
                .get("download")
                .with_context(|| "has download key")?
                .get("href")
                .with_context(|| "has href key")?
                .as_str()
                .with_context(|| "can get href from Git LFS response")?;

            // Actually download that file using the provided URL.
            let res = client.get(href).send().with_context(|| "can get file")?;
            let bytes = res.bytes().with_context(|| "can get bytes from file")?;

            // Check hash locally.
            if self.hash_algo != "sha256" {
                unimplemented!("only sha256 is supported");
            } else {
                use sha2::{Digest, Sha256};
                let sha256_digest = Sha256::digest(&bytes);
                let sha256_str = hex::encode(sha256_digest);
                assert_eq!(sha256_str, self.oid);
            }

            Ok(bytes.into())
        }
    }
}
