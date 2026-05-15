use std::{
    ffi::OsStr,
    process::{Command, Stdio},
};

use anyhow::{bail, Context, Result};

const REDACTED: &str = "<redacted>";

pub fn run_command(command: &mut Command) -> Result<()> {
    let description = describe_command(command);
    let status = command
        .status()
        .with_context(|| format!("failed to run {description}"))?;
    if status.success() {
        Ok(())
    } else {
        bail!("command failed with status {status}: {description}")
    }
}

pub fn command_output(command: &mut Command) -> Result<std::process::Output> {
    let description = describe_command(command);
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run {description}"))
}

pub fn collect_args<I, S>(args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter()
        .map(|arg| arg.as_ref().to_string_lossy().into_owned())
        .collect()
}

pub fn render_args<S>(args: &[S]) -> String
where
    S: AsRef<str>,
{
    redact_args(args.iter().map(AsRef::as_ref))
        .iter()
        .map(|arg| shell_escape(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn format_captured_output(output: &std::process::Output) -> String {
    let stdout = redact_text(String::from_utf8_lossy(&output.stdout).trim());
    let stderr = redact_text(String::from_utf8_lossy(&output.stderr).trim());
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("stdout:\n{stdout}\n\nstderr:\n{stderr}"),
        (false, true) => format!("stdout:\n{stdout}"),
        (true, false) => format!("stderr:\n{stderr}"),
        (true, true) => String::from("<no captured output>"),
    }
}

fn describe_command(command: &Command) -> String {
    let program = shell_escape(&command.get_program().to_string_lossy());
    let args = render_args(
        &command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    );
    let envs = command
        .get_envs()
        .filter_map(|(key, value)| {
            value.map(|value| {
                let key = key.to_string_lossy();
                let value = value.to_string_lossy();
                if is_sensitive_key(&key) {
                    format!("{key}={REDACTED}")
                } else {
                    format!("{key}={}", shell_escape(&value))
                }
            })
        })
        .collect::<Vec<_>>()
        .join(" ");

    match (envs.is_empty(), args.is_empty()) {
        (true, true) => program,
        (true, false) => format!("{program} {args}"),
        (false, true) => format!("{envs} {program}"),
        (false, false) => format!("{envs} {program} {args}"),
    }
}

fn redact_args<'a>(args: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut redact_next = false;
    args.map(|arg| {
        if redact_next {
            redact_next = false;
            return REDACTED.to_string();
        }
        if let Some((key, _)) = arg.split_once('=') {
            if is_sensitive_key(key) {
                return format!("{key}={REDACTED}");
            }
        }
        if is_sensitive_key(arg) {
            redact_next = true;
            return arg.to_string();
        }
        if looks_like_secret(arg) {
            REDACTED.to_string()
        } else {
            arg.to_string()
        }
    })
    .collect()
}

fn redact_text(text: impl AsRef<str>) -> String {
    text.as_ref()
        .split_whitespace()
        .map(|word| {
            if looks_like_secret(word) {
                REDACTED
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .trim_start_matches('-')
        .replace('-', "_")
        .to_ascii_uppercase();
    normalized.contains("JWT")
        || normalized.contains("AUTH")
        || normalized.contains("TOKEN")
        || normalized.contains("SECRET")
        || normalized.contains("PASSWORD")
        || normalized.ends_with("_SK")
        || normalized.ends_with("_SK_HEX")
        || normalized.contains("SK_HEX")
        || normalized.contains("DK_HEX")
        || normalized == "DK"
        || normalized == "DK_HEX"
}

fn looks_like_secret(value: &str) -> bool {
    let trimmed = value.trim_matches(|ch| ch == '"' || ch == '\'');
    trimmed.len() >= 48 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn shell_escape(arg: &str) -> String {
    if arg
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "/._:-=<>".contains(ch))
    {
        arg.to_string()
    } else {
        format!("{arg:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::render_args;

    #[test]
    fn redacts_secret_flag_values() {
        let rendered = render_args(&[
            "--dk-hex",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--node",
            "http://127.0.0.1:8080",
        ]);
        assert!(rendered.contains("--dk-hex <redacted>"));
        assert!(rendered.contains("--node http://127.0.0.1:8080"));
    }

    #[test]
    fn redacts_secret_assignments() {
        let rendered = render_args(&[
            "COMPLIANCE_DEV_AUTHORITY_SK_HEX=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "public=value",
        ]);
        assert!(rendered.contains("COMPLIANCE_DEV_AUTHORITY_SK_HEX=<redacted>"));
        assert!(rendered.contains("public=value"));
    }
}
