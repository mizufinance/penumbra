use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DAEMON_READY_MAGIC: &str = "PGDR";
const DAEMON_READY_VERSION: u32 = 1;
const DAEMON_REQUEST_MAGIC: &[u8; 4] = b"PGRQ";
const DAEMON_RESPONSE_MAGIC: &[u8; 4] = b"PGRS";
const DAEMON_PROTOCOL_VERSION: u32 = 1;
const DAEMON_OP_PROVE: u32 = 1;
const DAEMON_OP_SHUTDOWN: u32 = 2;
// Gnark proofs are a few hundred bytes; cap response payloads at 1 MiB.
const MAX_PAYLOAD_LEN: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct GnarkDaemonReady {
    pub magic: String,
    pub version: u32,
    pub status: String,
    pub circuit: String,
    pub curve: String,
    pub metadata_sha256_hex: String,
    pub verifying_key_sha256_hex: String,
    pub proving_key_sha256_hex: String,
    #[serde(default)]
    pub verifying_key_id: Option<String>,
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
        if ready.version != DAEMON_READY_VERSION {
            bail!("unsupported gnark daemon ready version {}", ready.version);
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
    proving_key_hash: Option<&str>,
    verifying_key_hash: Option<&str>,
    verifying_key_id: Option<&str>,
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
    if let Some(expected) = proving_key_hash {
        if ready.proving_key_sha256_hex != expected {
            bail!(
                "gnark daemon proving key hash mismatch: expected {expected}, got {}",
                ready.proving_key_sha256_hex
            );
        }
    }
    if let Some(expected) = verifying_key_hash {
        if ready.verifying_key_sha256_hex != expected {
            bail!(
                "gnark daemon verifying key hash mismatch: expected {expected}, got {}",
                ready.verifying_key_sha256_hex
            );
        }
    }
    if let Some(expected) = verifying_key_id {
        match ready.verifying_key_id.as_deref() {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                bail!("gnark daemon verifying key id mismatch: expected {expected}, got {actual}");
            }
            None => bail!("gnark daemon omitted verifying key id"),
        }
    }
    Ok(())
}

pub(crate) fn sha256_hex_path(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn write_frame(mut writer: impl Write, op: u32, payload: &[u8]) -> Result<()> {
    let total_len =
        u32::try_from(16usize + payload.len()).context("gnark daemon request too large")?;
    writer.write_all(DAEMON_REQUEST_MAGIC)?;
    writer.write_all(&DAEMON_PROTOCOL_VERSION.to_le_bytes())?;
    writer.write_all(&total_len.to_le_bytes())?;
    writer.write_all(&op.to_le_bytes())?;
    writer.write_all(payload)?;
    Ok(())
}

fn read_frame(mut reader: impl Read) -> Result<(u32, Vec<u8>)> {
    let mut header = [0u8; 16];
    reader.read_exact(&mut header)?;
    if &header[..4] != DAEMON_RESPONSE_MAGIC {
        bail!("invalid gnark daemon response magic");
    }
    let version = u32::from_le_bytes(header[4..8].try_into().expect("slice length"));
    if version != DAEMON_PROTOCOL_VERSION {
        bail!("unsupported gnark daemon response version {version}");
    }
    let total_len = u32::from_le_bytes(header[8..12].try_into().expect("slice length")) as usize;
    if total_len < 16 {
        bail!("invalid gnark daemon response length {total_len}");
    }
    let status = u32::from_le_bytes(header[12..16].try_into().expect("slice length"));
    let payload_len = total_len - 16;
    if payload_len > MAX_PAYLOAD_LEN {
        bail!("gnark daemon response payload {payload_len} bytes exceeds limit {MAX_PAYLOAD_LEN}");
    }
    let mut payload = vec![0u8; payload_len];
    reader.read_exact(&mut payload)?;
    Ok((status, payload))
}
