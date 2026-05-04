//! Owner: System Git resolution
//! Proof: `cargo test -p jeryu -- git_system`
//! Invariants: The resolver never falls back to a hard-coded `/usr/bin/git`.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct SystemGit {
    pub path: PathBuf,
}

static RESOLVED: OnceLock<PathBuf> = OnceLock::new();

impl SystemGit {
    pub fn resolve() -> Result<Self> {
        if let Some(path) = RESOLVED.get() {
            return Ok(Self { path: path.clone() });
        }

        let candidate = std::env::var("JERYU_SYSTEM_GIT")
            .ok()
            .or_else(|| crate::settings::get().git.system_git.clone())
            .or_else(find_git_on_path);

        let path = candidate
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("unable to resolve system git binary"))?;
        let _ = RESOLVED.set(path.clone());
        Ok(Self { path })
    }

    pub fn command(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(&self.path);
        command.current_dir(cwd);
        command.args(args);
        command.env("JERYU_GIT_RECURSION_GUARD", "1");
        command.stdin(Stdio::inherit());
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
        command
    }

    pub fn output(&self, cwd: &Path, args: &[&str]) -> Result<Output> {
        self.command(cwd, args)
            .output()
            .with_context(|| format!("running git {:?}", args))
    }

    pub fn status(&self, cwd: &Path, args: &[&str]) -> Result<std::process::ExitStatus> {
        self.command(cwd, args)
            .status()
            .with_context(|| format!("running git {:?}", args))
    }
}

fn find_git_on_path() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("git");
        if candidate.is_file() {
            return Some(candidate.display().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_lookup_is_optional() {
        let _ = find_git_on_path();
    }
}
