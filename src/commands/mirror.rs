//! Owner: Mirror command wrappers
//! Proof: `cargo test -p jeryu -- mirror`
//! Invariants: Mirror operations preserve the existing shadow remote compatibility path.

use anyhow::Result;

pub(crate) async fn execute_mirror_commands(cmd: crate::cli::MirrorCommands) -> Result<()> {
    match cmd {
        crate::cli::MirrorCommands::Status { repo, name } => {
            let status = jeryu::shadow::status(repo.as_deref(), &name)?;
            println!("━━━ jeryu mirror status ━━━\n");
            println!("  Repo:         {}", status.repo_root.display());
            println!(
                "  Head branch:  {}",
                status.head_branch.as_deref().unwrap_or("(detached)")
            );
            println!(
                "  Target remote {}: {}",
                status.target_remote,
                if status.target_exists {
                    "present"
                } else {
                    "missing"
                }
            );
            println!("\n  Remotes:");
            for remote in &status.remotes {
                println!(
                    "    {:<12} fetch={} push={}",
                    remote.name,
                    remote.fetch_url.as_deref().unwrap_or("(none)"),
                    remote.push_url
                );
            }
            println!();
        }
        crate::cli::MirrorCommands::Ensure { repo, name, url } => {
            jeryu::shadow::ensure_remote(repo.as_deref(), &name, &url)?;
            println!("✅ Mirror remote '{}' now points to {}", name, url);
        }
        crate::cli::MirrorCommands::Push {
            repo,
            name,
            branch,
            mirror,
        } => {
            jeryu::shadow::push_remote(repo.as_deref(), &name, branch.as_deref(), mirror)?;
            if mirror {
                println!("✅ Mirrored repository to remote '{}'", name);
            } else {
                println!(
                    "✅ Pushed HEAD to remote '{}'{}",
                    name,
                    branch
                        .as_deref()
                        .map(|branch| format!(" as {branch}"))
                        .unwrap_or_default()
                );
            }
        }
    }
    Ok(())
}
