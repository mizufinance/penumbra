//! Wrapper around the `cli-tool` binary for Orbis node interaction.

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

    pub fn get_peer_id(&self, endpoint: &str) -> Result<String> {
        let output = self.run(&["info", "--endpoint", endpoint])?;
        output
            .lines()
            .find(|l| l.contains("Peer ID:"))
            .and_then(|l| l.split_whitespace().last())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no Peer ID in output"))
    }

    pub fn run_dkg(&self, threshold: usize, peer_ids: &[String]) -> Result<()> {
        let mut args = vec![
            "dkg".to_string(),
            "--endpoint".to_string(),
            self.endpoint.clone(),
            "--threshold".to_string(),
            threshold.to_string(),
            "--peer-ids".to_string(),
        ];
        for id in peer_ids {
            args.push(id.clone());
        }
        let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.run(&args_ref)?;
        Ok(())
    }

    pub fn get_latest_ring(&self) -> Result<(decaf377::Element, String)> {
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
        Ok((element, ring_id))
    }

    pub fn generate_reader_key(&self) -> Result<(String, String)> {
        let output = self.run(&["generate-reader-key"])?;
        let sk = output
            .lines()
            .skip_while(|l| !l.contains("Secret Key"))
            .nth(1)
            .map(|l| l.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("no Secret Key in output"))?;
        let pk = output
            .lines()
            .skip_while(|l| !l.contains("Public Key"))
            .nth(1)
            .map(|l| l.trim().to_string())
            .ok_or_else(|| anyhow::anyhow!("no Public Key in output"))?;
        Ok((sk, pk))
    }

    pub fn add_policy(&self) -> Result<String> {
        let output = self.run(&["add-policy-to-chain"])?;
        output
            .lines()
            .find(|l| l.starts_with("POLICY_ID="))
            .map(|l| l.trim_start_matches("POLICY_ID=").to_string())
            .ok_or_else(|| anyhow::anyhow!("no POLICY_ID in output"))
    }

    pub fn store_secret(
        &self,
        seed_hex: &str,
        ring_pk_hex: &str,
        ring_id: &str,
        policy_id: &str,
        derivation_hex: &str,
    ) -> Result<String> {
        let output = self.run(&[
            "store-secret",
            "--endpoint",
            &self.endpoint,
            "--secret",
            seed_hex,
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
        output
            .lines()
            .find(|l| l.contains("Object ID:"))
            .and_then(|l| l.split_whitespace().last())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("no Object ID in store-secret output"))
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

    /// Attempt PRE. Returns the recovered seed hex on success.
    pub fn pre(
        &self,
        ring_pk_hex: &str,
        reader_pk: &str,
        reader_sk: &str,
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
            reader_pk,
            "--reader-sk",
            reader_sk,
            "--object-id",
            object_id,
            "--namespace",
            "orbis",
            "--derivation",
            derivation_hex,
        ])?;
        output
            .lines()
            .find(|l| l.contains("Decrypted Secret:"))
            .map(|l| {
                l.split("Decrypted Secret:")
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("no Decrypted Secret in PRE output"))
    }
}
