//! Build-time validation and native-library preparation for gnark proof
//! parameters. Large proving artifacts are materialized explicitly before a
//! bundled-prover build; ordinary builds require only files kept in Git.
use anyhow::Context;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[path = "src/gnark_artifact_validation.rs"]
mod gnark_artifact_validation;
#[path = "src/production_approval.rs"]
mod production_approval;

include!("src/gen/gnark/transfer_families_build.rs");
include!("src/gen/gnark/note_reshape_families_build.rs");
include!("src/gen/gnark/shielded_withdrawal_families_build.rs");

fn main() {
    println!("cargo:rustc-check-cfg=cfg(approved_proof_artifacts)");
    println!("cargo:rerun-if-changed=production_keys.json");
    let approvals = include_bytes!("production_keys.json");
    let mut all_approved = true;
    emit_family_rerun_hints().expect("emit proof-family rerun-if-changed hints");
    if cfg!(feature = "bundled-proving-keys") {
        emit_gnark_runtime_rerun_hints().expect("emit gnark runtime rerun-if-changed hints");
    }

    let generated_roster = generated_deployed_family_roster();
    gnark_artifact_validation::validate_deployed_family_roster(&generated_roster)
        .expect("generated proof-family roster matches the exact deployed families");

    let artifact_root = repo_root()
        .expect("resolve repository root")
        .join("tools/gnark/artifacts");
    for family in gnark_artifact_validation::DEPLOYED_FAMILIES {
        for path in gnark_artifact_validation::artifact_paths(&artifact_root, family) {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }

    for family in gnark_artifact_validation::DEPLOYED_FAMILIES {
        let metadata: gnark_artifact_validation::CircuitMetadataJson = serde_json::from_slice(
            &std::fs::read(
                artifact_root
                    .join(family.label)
                    .join("circuit_metadata.json"),
            )
            .expect("read proof metadata for production approval"),
        )
        .expect("parse proof metadata for production approval");
        let key_hash = &metadata.verifying_key_binary_sha256_hex;
        all_approved &= production_approval::approved(
            approvals,
            family.label,
            key_hash,
            &metadata.verifying_key_json_sha256_hex,
        )
        .expect("validate production proof-key approvals");
        gnark_artifact_validation::validate_repository_artifacts(&artifact_root, family)
            .unwrap_or_else(|error| {
                panic!(
                    "deployed {} repository artifacts failed intrinsic build validation: {error:#}",
                    family.label
                )
            });
        if cfg!(feature = "bundled-proving-keys") {
            gnark_artifact_validation::validate_materialized_proving_key(&artifact_root, family)
                .unwrap_or_else(|error| {
                    panic!(
                        "deployed {} proving key is not materialized or invalid: {error:#}\nrun `python3 scripts/proof_artifacts.py materialize --bundle runtime` from the repository root",
                        family.label
                    )
                });
        }
    }

    std::fs::write(
        PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"))
            .join("proof_artifact_provenance.json"),
        serde_json::to_vec(&serde_json::json!({
            "approved": all_approved,
            "debug_assertions": std::env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some(),
        }))
        .expect("serialize proof artifact provenance"),
    )
    .expect("write proof artifact provenance");

    if all_approved {
        println!("cargo:rustc-cfg=approved_proof_artifacts");
    }

    write_bundled_gnark_runtime_paths().expect("failed while preparing bundled gnark runtime");
}

fn generated_deployed_family_roster() -> Vec<gnark_artifact_validation::DeployedFamily> {
    use gnark_artifact_validation::{DeployedFamily, FamilyKind, InputPadding, OutputPadding};

    let mut roster = Vec::with_capacity(
        GENERATED_TRANSFER_FAMILIES.len()
            + GENERATED_NOTE_RESHAPE_FAMILIES.len()
            + GENERATED_SHIELDED_WITHDRAWAL_FAMILIES.len()
            + 1,
    );
    roster.extend(
        GENERATED_TRANSFER_FAMILIES
            .iter()
            .map(|family| DeployedFamily {
                kind: FamilyKind::Transfer,
                id: None,
                label: family.label,
                artifact_name: family.artifact_name,
                n_in: family.n_in,
                n_out: family.n_out,
                input_padding: InputPadding::Fixed,
                output_padding: OutputPadding::Fixed,
                min_real_inputs: family.n_in,
                max_real_inputs: family.n_in,
                min_real_outputs: family.n_out,
                max_real_outputs: family.n_out,
            }),
    );
    for family in GENERATED_NOTE_RESHAPE_FAMILIES {
        let input_padding = match family.input_padding {
            InputPaddingPolicy::Fixed => InputPadding::Fixed,
            InputPaddingPolicy::SyntheticPrivate => InputPadding::SyntheticPrivate,
        };
        let output_padding = match family.output_padding {
            OutputPaddingPolicy::Fixed => OutputPadding::Fixed,
            OutputPaddingPolicy::ZeroNote => OutputPadding::ZeroNote,
        };
        roster.push(DeployedFamily {
            kind: FamilyKind::NoteReshape,
            id: Some(family.id),
            label: family.label,
            artifact_name: family.artifact_name,
            n_in: family.n_in,
            n_out: family.n_out,
            input_padding,
            output_padding,
            min_real_inputs: family.min_real_inputs,
            max_real_inputs: family.max_real_inputs,
            min_real_outputs: family.min_real_outputs,
            max_real_outputs: family.max_real_outputs,
        });
    }
    roster.extend(
        GENERATED_SHIELDED_WITHDRAWAL_FAMILIES
            .iter()
            .map(|family| DeployedFamily {
                kind: FamilyKind::ShieldedWithdrawal,
                id: Some(family.id),
                label: family.label,
                artifact_name: family.artifact_name,
                n_in: family.n_in,
                n_out: family.n_out,
                input_padding: InputPadding::Fixed,
                output_padding: OutputPadding::Fixed,
                min_real_inputs: family.n_in,
                max_real_inputs: family.n_in,
                min_real_outputs: family.n_out,
                max_real_outputs: family.n_out,
            }),
    );
    roster.push(DeployedFamily {
        kind: FamilyKind::NoteSeizure,
        id: None,
        label: "note_seizure",
        artifact_name: "note_seizure",
        n_in: 1,
        n_out: 0,
        input_padding: InputPadding::Fixed,
        output_padding: OutputPadding::Fixed,
        min_real_inputs: 1,
        max_real_inputs: 1,
        min_real_outputs: 0,
        max_real_outputs: 0,
    });
    roster
}

fn emit_gnark_runtime_rerun_hints() -> anyhow::Result<()> {
    let repo_root = repo_root()?;
    let gnark_root = repo_root.join("tools/gnark");
    emit_rerun_hints_recursive(&gnark_root)?;
    Ok(())
}

fn emit_rerun_hints_recursive(path: &Path) -> anyhow::Result<()> {
    if path.is_dir() {
        for entry in
            std::fs::read_dir(path).with_context(|| format!("read directory {}", path.display()))?
        {
            let entry = entry.with_context(|| format!("read entry in {}", path.display()))?;
            emit_rerun_hints_recursive(&entry.path())?;
        }
        return Ok(());
    }

    let should_watch = matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("go") | Some("json")
    ) || matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("go.mod") | Some("go.sum")
    );
    if should_watch {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    Ok(())
}

fn emit_family_rerun_hints() -> anyhow::Result<()> {
    let root = repo_root()?;
    for family in ["transfer", "note_reshape", "shielded_withdrawal"] {
        for relative in [
            format!("tools/gnark/{family}_families.json"),
            format!("crates/crypto/proof-params/src/gen/gnark/{family}_families_build.rs"),
            format!("crates/crypto/proof-params/src/gen/gnark/{family}_registry.rs"),
        ] {
            println!("cargo:rerun-if-changed={}", root.join(relative).display());
        }
    }
    Ok(())
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

    let transfer_lib_path = gnark_out_dir.join(format!(
        "{}.{lib_ext}",
        GENERATED_TRANSFER_FAMILIES[0].bundled_lib_basename
    ));
    let note_reshape_lib_path =
        gnark_out_dir.join(format!("libshieldd_gnark_note_reshape.{lib_ext}"));
    let shielded_withdrawal_lib_path = gnark_out_dir.join(format!(
        "{}.{lib_ext}",
        GENERATED_SHIELDED_WITHDRAWAL_FAMILIES[0].bundled_lib_basename
    ));

    build_gnark_library(
        &gnark_dir,
        "./cmd/transferlib",
        &transfer_lib_path,
        goos,
        goarch,
    )
    .context("build bundled gnark transfer library")?;
    build_gnark_library(
        &gnark_dir,
        "./cmd/note_reshapelib",
        &note_reshape_lib_path,
        goos,
        goarch,
    )
    .context("build bundled gnark note reshape library")?;
    build_gnark_library(
        &gnark_dir,
        "./cmd/shieldedwithdrawallib",
        &shielded_withdrawal_lib_path,
        goos,
        goarch,
    )
    .context("build bundled gnark shielded withdrawal library")?;

    let include_body = format!(
        "pub const GNARK_TRANSFER_BUNDLED_LIBRARY_PATH: Option<&str> = Some(r#\"{}\"#);\n\
         pub const GNARK_NOTE_RESHAPE_BUNDLED_LIBRARY_PATH: Option<&str> = Some(r#\"{}\"#);\n\
         pub const GNARK_SHIELDED_WITHDRAWAL_BUNDLED_LIBRARY_PATH: Option<&str> = Some(r#\"{}\"#);\n",
        transfer_lib_path.file_name().context("gnark library filename")?.to_string_lossy(),
        note_reshape_lib_path.file_name().context("gnark library filename")?.to_string_lossy(),
        shielded_withdrawal_lib_path.file_name().context("gnark library filename")?.to_string_lossy(),
    );
    std::fs::write(&include_path, include_body).context("write gnark runtime include file")?;

    Ok(())
}

fn write_empty_gnark_runtime_include(include_path: &Path) -> anyhow::Result<()> {
    let include_body = String::from(
        "pub const GNARK_TRANSFER_BUNDLED_LIBRARY_PATH: Option<&str> = None;\n\
         pub const GNARK_NOTE_RESHAPE_BUNDLED_LIBRARY_PATH: Option<&str> = None;\n\
         pub const GNARK_SHIELDED_WITHDRAWAL_BUNDLED_LIBRARY_PATH: Option<&str> = None;\n",
    );
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
        .arg("-buildvcs=false")
        .arg("-buildmode=c-shared")
        .arg("-o")
        .arg(output_path)
        .arg(package);

    let output = command.output().with_context(|| {
        "run `go build` for bundled gnark runtime (install Go to use bundled-proving-keys)"
            .to_owned()
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!("go build failed for {package}:\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }

    Ok(())
}
