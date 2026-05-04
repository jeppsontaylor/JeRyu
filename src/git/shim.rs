//! Owner: Git shim helpers
//! Proof: `cargo test -p jeryu -- git_shim`
//! Invariants: Shims are opt-in and resolve to the configured system git path.

use anyhow::Result;
use std::path::Path;

pub fn render_git_shim(system_git: &Path) -> String {
    format!(
        "#!/usr/bin/env sh\nexec {} \"$@\"\n",
        shell_escape::unix::escape(system_git.display().to_string().into())
    )
}

pub fn install_git_shim(path: &Path, system_git: &Path) -> Result<()> {
    std::fs::write(path, render_git_shim(system_git))?;
    Ok(())
}

mod shell_escape {
    pub mod unix {
        pub fn escape(input: String) -> String {
            if input
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
            {
                input
            } else {
                format!("'{}'", input.replace('\'', "'\"'\"'"))
            }
        }
    }
}
