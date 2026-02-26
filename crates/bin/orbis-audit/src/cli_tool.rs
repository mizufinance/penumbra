//! Wrapper around the `cli-tool` binary for Orbis node interaction.
//!
//! Adapted from orbis-test's cli_tool.rs. Key differences:
//! - `store_secret()` stores dummy data and returns both object_id + enc_cmt
//! - `pre_xnc_only()` calls `pre --xnc-only` to get xnc_cmt without AES decrypt

use anyhow::{bail, Context, Result};
use std::process::Command;

pub struct CliTool {
    bin: String,
    endpoint: String,
}

impl CliTool {
    pub fn new(bin: &str, endpoint: String) -> Self {
        Self {
            bin: bin.to_string(),
            endpoint,
        }
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let output = Command::new(&self.bin)
            .args(args)
            .output()
            .with_context(|| format!("failed to run {} {}", self.bin, args.join(" ")))?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("{} failed (exit {}): {}", self.bin, output.status, stderr);
        }
        Ok(stdout)
    }

    /// Returns (decaf377 Element, ring_id, original_ring_pk_hex).
    /// The original hex must be used for cli-tool calls (different serialization than decaf377).
    pub fn get_latest_ring(&self) -> Result<(decaf377::Element, String, String)> {
        let output = self.run(&["get-latest-ring"])?;
        let mut ring_pk_hex = String::new();
        let mut ring_id = String::new();
        for line in output.lines() {
            if line.starts_with("RING_PK=") {
                ring_pk_hex = line.trim_start_matches("RING_PK=").to_string();
            } else if line.starts_with("RING_ID=") {
                ring_id = line.trim_start_matches("RING_ID=").to_string();
            }
        }
        if ring_pk_hex.is_empty() {
            bail!("no RING_PK in output");
        }
        let bytes = hex::decode(&ring_pk_hex)?;
        let bytes_arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("ring_pk should be 32 bytes"))?;
        let element = decaf377::Encoding(bytes_arr)
            .vartime_decompress()
            .map_err(|_| anyhow::anyhow!("invalid ring_pk encoding"))?;
        Ok((element, ring_id, ring_pk_hex))
    }

    pub fn add_policy(&self) -> Result<String> {
        let output = self.run(&["add-policy-to-chain"])?;
        output
            .lines()
            .find(|l| l.starts_with("POLICY_ID="))
            .map(|l| l.trim_start_matches("POLICY_ID=").to_string())
            .ok_or_else(|| anyhow::anyhow!("no POLICY_ID in output"))
    }

    /// Store a dummy secret in Orbis. Returns (object_id, enc_cmt_hex).
    /// The enc_cmt is the encryption commitment Orbis will use for PRE on this object.
    pub fn store_secret(
        &self,
        ring_pk_hex: &str,
        ring_id: &str,
        policy_id: &str,
        derivation_hex: &str,
    ) -> Result<(String, String)> {
        // Store 32 zero bytes as dummy secret
        let dummy_secret = "00".repeat(32);
        let output = self.run(&[
            "store-secret",
            "--endpoint",
            &self.endpoint,
            "--secret",
            &dummy_secret,
            "--ring-pk-hex",
            ring_pk_hex,
            "--ring-id",
            ring_id,
            "--namespace",
            "orbis",
            "--policy-id",
            policy_id,
            "--resource",
            "document",
            "--permission",
            "read",
            "--derivation",
            derivation_hex,
        ])?;
        let object_id = output
            .lines()
            .find(|l| l.contains("Object ID:"))
            .and_then(|l| l.split_whitespace().last())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no Object ID in store-secret output"))?;
        let enc_cmt_hex = output
            .lines()
            .find(|l| l.contains("enc_cmt:"))
            .and_then(|l| l.split_whitespace().last())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no enc_cmt in store-secret output"))?;
        Ok((object_id, enc_cmt_hex))
    }

    pub fn register_object(&self, policy_id: &str, object_id: &str) -> Result<()> {
        self.run(&[
            "register-object-to-chain",
            "--policy-id",
            policy_id,
            "--object-id",
            object_id,
            "--resource",
            "document",
        ])?;
        Ok(())
    }

    pub fn set_relationship(&self, policy_id: &str, object_id: &str) -> Result<()> {
        self.run(&[
            "set-relationship-on-chain",
            "--policy-id",
            policy_id,
            "--object-id",
            object_id,
            "--resource",
            "document",
            "--relation",
            "reader",
        ])?;
        Ok(())
    }

    /// Call `pre --xnc-only` to get the re-encryption commitment without AES decrypt.
    /// Returns xnc_cmt as hex string.
    pub fn pre_xnc_only(
        &self,
        ring_pk_hex: &str,
        reader_pk_hex: &str,
        object_id: &str,
        derivation_hex: &str,
    ) -> Result<String> {
        let output = self.run(&[
            "pre",
            "--endpoint",
            &self.endpoint,
            "--ring-pk",
            ring_pk_hex,
            "--reader-pk",
            reader_pk_hex,
            "--object-id",
            object_id,
            "--namespace",
            "orbis",
            "--derivation",
            derivation_hex,
            "--xnc-only",
        ])?;
        // Look for "Re-encrypted commitment (xnc_cmt): <hex>"
        output
            .lines()
            .find(|l| l.contains("xnc_cmt"))
            .and_then(|l| l.split(':').last())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("no xnc_cmt in PRE --xnc-only output"))
    }
}
