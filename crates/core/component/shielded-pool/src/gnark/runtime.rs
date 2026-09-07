use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DAEMON_READY_MAGIC: &str = "PGDR";
const DAEMON_REQUEST_MAGIC: &[u8; 4] = b"PGRQ";
const DAEMON_RESPONSE_MAGIC: &[u8; 4] = b"PGRS";
const DAEMON_OP_PROVE: u32 = 1;
const DAEMON_OP_SHUTDOWN: u32 = 2;
// Witness payloads are a few KB; match the Go daemon cap.
pub(crate) const GNARK_MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
// Gnark proofs are a few hundred bytes; cap response payloads at 1 MiB.
pub(crate) const GNARK_MAX_RESULT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GnarkDaemonReady {
    pub magic: String,
    pub status: String,
    pub circuit: String,
    pub curve: String,
    pub metadata_sha256_hex: String,
    pub verifying_key_binary_sha256_hex: String,
    pub verifying_key_json_sha256_hex: String,
    pub proving_key_sha256_hex: String,
}

pub(crate) struct GnarkDaemonProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl GnarkDaemonProcess {
    pub(crate) fn spawn(
        binary: &Path,
        circuit: &str,
        artifact_dir: &Path,
    ) -> Result<(Self, GnarkDaemonReady)> {
        let mut child = Command::new(binary)
            .arg("--circuit")
            .arg(circuit)
            .arg("--artifact-dir")
            .arg(artifact_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawn gnark {circuit} daemon {}", binary.display()))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("gnark daemon stdin was not piped"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("gnark daemon stdout was not piped"))?;
        let mut stdout = BufReader::new(stdout);
        let mut ready_line = String::new();
        let read = stdout.read_line(&mut ready_line)?;
        if read == 0 {
            let status = child
                .try_wait()
                .context("poll gnark daemon readiness")?
                .map(|status| status.to_string())
                .unwrap_or_else(|| "still running".to_string());
            bail!("gnark daemon exited before readiness handshake ({status})");
        }
        let ready: GnarkDaemonReady =
            serde_json::from_str(ready_line.trim_end()).context("decode gnark daemon readiness")?;
        if ready.magic != DAEMON_READY_MAGIC {
            bail!("invalid gnark daemon ready magic {}", ready.magic);
        }
        if ready.status != "ready" {
            bail!("gnark daemon reported non-ready status {}", ready.status);
        }

        Ok((
            Self {
                child,
                stdin,
                stdout,
            },
            ready,
        ))
    }

    pub(crate) fn prove(&mut self, witness: &[u8]) -> Result<Vec<u8>> {
        write_frame(&mut self.stdin, DAEMON_OP_PROVE, witness)?;
        self.stdin.flush().context("flush gnark daemon request")?;
        let (status, payload) = read_frame(&mut self.stdout)?;
        if status != 0 {
            bail!(
                "gnark daemon prove failed: {}",
                String::from_utf8_lossy(&payload)
            );
        }
        Ok(payload)
    }

    pub(crate) fn shutdown(&mut self) {
        let _ = write_frame(&mut self.stdin, DAEMON_OP_SHUTDOWN, &[]);
        let _ = self.stdin.flush();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for GnarkDaemonProcess {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub(crate) fn validate_daemon_ready(
    ready: &GnarkDaemonReady,
    expected_circuit: &str,
    metadata_hash: &str,
    proving_key_hash: &str,
    verifying_key_binary_hash: &str,
    verifying_key_json_hash: &str,
) -> Result<()> {
    if ready.circuit != expected_circuit {
        bail!(
            "gnark daemon circuit mismatch: expected {expected_circuit}, got {}",
            ready.circuit
        );
    }
    if ready.curve != "bls12-377" {
        bail!("unsupported gnark daemon curve {}", ready.curve);
    }
    if ready.metadata_sha256_hex != metadata_hash {
        bail!(
            "gnark daemon metadata hash mismatch: expected {metadata_hash}, got {}",
            ready.metadata_sha256_hex
        );
    }
    if ready.proving_key_sha256_hex != proving_key_hash {
        bail!(
            "gnark daemon proving key hash mismatch: expected {proving_key_hash}, got {}",
            ready.proving_key_sha256_hex
        );
    }
    if ready.verifying_key_binary_sha256_hex != verifying_key_binary_hash {
        bail!(
            "gnark daemon binary verifying key hash mismatch: expected {verifying_key_binary_hash}, got {}",
            ready.verifying_key_binary_sha256_hex
        );
    }
    if ready.verifying_key_json_sha256_hex != verifying_key_json_hash {
        bail!(
            "gnark daemon JSON verifying key hash mismatch: expected {verifying_key_json_hash}, got {}",
            ready.verifying_key_json_sha256_hex
        );
    }
    Ok(())
}

pub(crate) fn sha256_hex_path(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn write_frame(mut writer: impl Write, op: u32, payload: &[u8]) -> Result<()> {
    if payload.len() > GNARK_MAX_REQUEST_BYTES {
        bail!(
            "gnark daemon request payload {} bytes exceeds limit {GNARK_MAX_REQUEST_BYTES}",
            payload.len()
        );
    }
    let total_len =
        u32::try_from(12usize + payload.len()).context("gnark daemon request too large")?;
    writer.write_all(DAEMON_REQUEST_MAGIC)?;
    writer.write_all(&total_len.to_le_bytes())?;
    writer.write_all(&op.to_le_bytes())?;
    writer.write_all(payload)?;
    Ok(())
}

fn read_frame(mut reader: impl Read) -> Result<(u32, Vec<u8>)> {
    let mut header = [0u8; 12];
    reader.read_exact(&mut header)?;
    if &header[..4] != DAEMON_RESPONSE_MAGIC {
        bail!("invalid gnark daemon response magic");
    }
    let total_len = u32::from_le_bytes(header[4..8].try_into().expect("slice length")) as usize;
    if total_len < 12 {
        bail!("invalid gnark daemon response length {total_len}");
    }
    let status = u32::from_le_bytes(header[8..12].try_into().expect("slice length"));
    let payload_len = total_len - 12;
    if payload_len > GNARK_MAX_RESULT_BYTES {
        bail!(
            "gnark daemon response payload {payload_len} bytes exceeds limit {GNARK_MAX_RESULT_BYTES}"
        );
    }
    let mut payload = vec![0u8; payload_len];
    reader.read_exact(&mut payload)?;
    Ok((status, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn write_frame_rejects_oversized_request() {
        let payload = vec![0u8; GNARK_MAX_REQUEST_BYTES + 1];
        let err = write_frame(Vec::new(), DAEMON_OP_PROVE, &payload)
            .expect_err("oversized daemon request should fail");

        assert!(
            err.to_string().contains("gnark daemon request payload"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn read_frame_rejects_oversized_response_before_payload_allocation() {
        let total_len = 12usize + GNARK_MAX_RESULT_BYTES + 1;
        let mut header = Vec::new();
        header.extend_from_slice(DAEMON_RESPONSE_MAGIC);
        header.extend_from_slice(&(total_len as u32).to_le_bytes());
        header.extend_from_slice(&0u32.to_le_bytes());

        let err = read_frame(Cursor::new(header)).expect_err("oversized response should fail");

        assert!(
            err.to_string().contains("gnark daemon response payload"),
            "unexpected error: {err:#}"
        );
    }
}
