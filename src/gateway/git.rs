//! Owner: Cache Gateway subsystem — Git objects proxy
//! Proof: `cargo nextest run -p jeryu -- gateway::git`
//! Invariants: Git object reuse remains content-addressed and scoped to the requesting namespace.
use anyhow::Result;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Git cache adapter for maintaining bare mirrors and performing fast reference clones.
#[derive(Clone)]
pub struct GitAdapter {
    cache_dir: PathBuf,
}

impl GitAdapter {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    fn mirror_path(&self, repo_url: &str) -> PathBuf {
        // Sanitize URL for directory name
        let sanitized = repo_url
            .replace("https://", "")
            .replace("http://", "")
            .replace("git@", "")
            .replace([':', '/'], "_");
        self.cache_dir.join(format!("{}.git", sanitized))
    }

    /// Refresh or initialize a bare mirror for a repository.
    pub async fn refresh_mirror(&self, repo_url: &str) -> Result<PathBuf> {
        let mirror_dir = self.mirror_path(repo_url);

        if mirror_dir.exists() {
            tracing::info!("Updating existing Git mirror for {}", repo_url);
            let status = Command::new("git")
                .arg("--git-dir")
                .arg(&mirror_dir)
                .arg("fetch")
                .arg("--prune")
                .arg("origin")
                .arg("+refs/heads/*:refs/heads/*")
                .status()
                .await?;

            if !status.success() {
                anyhow::bail!("Failed to fetch mirror: {}", status);
            }
        } else {
            tracing::info!("Initializing new Git mirror for {}", repo_url);
            std::fs::create_dir_all(&self.cache_dir)?;
            let status = Command::new("git")
                .arg("clone")
                .arg("--bare")
                .arg(repo_url)
                .arg(&mirror_dir)
                .status()
                .await?;

            if !status.success() {
                anyhow::bail!("Failed to clone mirror: {}", status);
            }
        }

        Ok(mirror_dir)
    }

    /// Fast clone into a target directory using `--reference-if-able`.
    pub async fn reference_clone(&self, repo_url: &str, target_dir: &Path) -> Result<()> {
        let mirror_dir = self.refresh_mirror(repo_url).await?;

        tracing::info!("Performing reference clone to {}", target_dir.display());
        let status = Command::new("git")
            .arg("clone")
            .arg("--reference-if-able")
            .arg(&mirror_dir)
            .arg("--dissociate") // Important for untrusted sandboxes so they don't corrupt the mirror
            .arg(repo_url)
            .arg(target_dir)
            .status()
            .await?;

        if !status.success() {
            anyhow::bail!("Failed reference clone: {}", status);
        }

        Ok(())
    }
}
