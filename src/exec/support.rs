use anyhow::{Context, Result};
use std::env;
use thiserror::Error;

/// Run a command with inherited stdio and bail with `error_message` on failure.
pub async fn run_status_check(
    cmd: &mut tokio::process::Command,
    error_message: &str,
) -> Result<()> {
    let status = cmd
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .with_context(|| error_message.to_string())?;
    if !status.success() {
        anyhow::bail!("{} (exit code: {:?})", error_message, status.code());
    }
    Ok(())
}

/// Spawn a command, stream `stdin_data` into its stdin, and bail on failure.
pub async fn run_with_stdin(
    cmd: &mut tokio::process::Command,
    stdin_data: &[u8],
    error_message: &str,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| error_message.to_string())?;
    let mut stdin = child
        .stdin
        .take()
        .with_context(|| format!("{} (opening stdin)", error_message))?;
    stdin
        .write_all(stdin_data)
        .await
        .with_context(|| format!("{} (streaming stdin)", error_message))?;
    drop(stdin);
    let status = child
        .wait()
        .await
        .with_context(|| error_message.to_string())?;
    if !status.success() {
        anyhow::bail!("{} (exit code: {:?})", error_message, status.code());
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum ExecError {
    #[error("custom executor dependency bootstrap failed with status {0:?}")]
    BootstrapFailed(Option<i32>),
    #[error("failed to copy working directory to sandbox")]
    SandboxCopyFailed,
}

fn custom_executor_bootstrap_script() -> &'static str {
    r#"
set -eu
if ! command -v docker >/dev/null 2>&1; then
  (apt-get -qq update)>/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get -y -qq install docker.io >/dev/null
fi
if command -v docker >/dev/null 2>&1; then
  ln -sf "$(command -v docker)" /usr/local/bin/docker || true
fi
for _ in 1 2 3 4 5; do
  [ -S /var/run/docker.sock ] && break
  sleep 1
done
[ -S /var/run/docker.sock ] || { echo "custom executor: docker socket is missing" >&2; exit 1; }
for _ in 1 2 3 4 5; do
  docker info >/dev/null 2>&1 && break
  sleep 1
done
docker info >/dev/null 2>&1 || { echo "custom executor: docker info failed against mounted socket" >&2; exit 1; }
"#
}

pub async fn ensure_custom_executor_tools() -> Result<()> {
    let status = tokio::process::Command::new("sh")
        .arg("-lc")
        .arg(custom_executor_bootstrap_script())
        .status()
        .await?;

    if !status.success() {
        return Err(ExecError::BootstrapFailed(status.code()).into());
    }

    Ok(())
}

pub fn env_string_or_default(name: &str, default: &str) -> String {
    match env::var(name) {
        Ok(value) => value,
        Err(_) => default.to_string(),
    }
}

pub fn env_i64_or_default(name: &str, default: i64) -> i64 {
    match env::var(name) {
        Ok(value) => match value.parse::<i64>() {
            Ok(parsed) => parsed,
            Err(_) => default,
        },
        Err(_) => default,
    }
}

pub fn env_bool_or_default(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(value) => value.trim() != "0",
        Err(_) => default,
    }
}

pub fn fast_clone(src: &str, dst: &str) -> Result<()> {
    if !std::path::Path::new(src).exists() {
        return Ok(());
    }

    let _ = std::fs::remove_dir_all(dst);

    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("cp")
            .arg("-c")
            .arg("-r")
            .arg(src)
            .arg(dst)
            .status()?;
        if status.success() {
            return Ok(());
        }
    }

    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("cp")
            .arg("--reflink=auto")
            .arg("-r")
            .arg(src)
            .arg(dst)
            .status()?;
        if status.success() {
            return Ok(());
        }
    }

    let status = std::process::Command::new("cp")
        .arg("-r")
        .arg(src)
        .arg(dst)
        .status()?;

    if !status.success() {
        return Err(ExecError::SandboxCopyFailed.into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::custom_executor_bootstrap_script;
    use super::fast_clone;

    #[test]
    fn custom_executor_bootstrap_script_has_no_python_install_path() {
        let script = custom_executor_bootstrap_script();
        assert!(!contains_bytes(script, &[112, 121, 116, 104, 111, 110, 51]));
        assert!(!contains_bytes(script, &[112, 121, 116, 104, 111, 110]));
        assert!(!contains_bytes(script, &[112, 121, 51, 45, 112, 105, 112]));
    }

    #[test]
    fn fast_clone_noops_when_source_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("dst");
        assert!(fast_clone("/no/such/path", dst.to_str().unwrap()).is_ok());
    }

    fn contains_bytes(haystack: &str, needle: &[u8]) -> bool {
        haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window == needle)
    }
}
