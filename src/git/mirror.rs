//! Owner: Git mirror helper
//! Proof: `cargo test -p jeryu -- git_mirror`
//! Invariants: Mirror failures are recorded and only become fatal in strict mode.

use anyhow::Result;
use std::path::Path;

use crate::git::system::SystemGit;

pub fn mirror_push(
    cwd: &Path,
    remote_name: &str,
    branch: Option<&str>,
    mirror: bool,
) -> Result<bool> {
    let git = SystemGit::resolve()?;
    let mut args = vec!["push"];
    if mirror {
        args.push("--mirror");
        args.push(remote_name);
    } else {
        args.push(remote_name);
        if let Some(branch) = branch {
            args.push(branch);
        } else {
            args.push("HEAD");
        }
    }
    let status = git.status(cwd, &args)?;
    Ok(status.success())
}
