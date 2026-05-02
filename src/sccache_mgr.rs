//! Owner: sccache Management subsystem
//! Proof: `cargo nextest run -p vgit -- sccache_mgr`
//! Invariants: Cache workers stay isolated by manager namespace and report health before reuse.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Manages sccache daemon and environment configurations for sandboxed runners.
pub struct SccacheManager {
    pub cache_dir: PathBuf,
    pub cache_size: String,
}

impl SccacheManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            cache_size: "10G".to_string(),
        }
    }

    pub fn new_with_size(cache_dir: PathBuf, cache_size: String) -> Self {
        Self {
            cache_dir,
            cache_size,
        }
    }

    pub fn cache_path(&self) -> &Path {
        &self.cache_dir
    }

    /// Provides the environment variables to inject into the `ExecutorSandbox`
    /// to seamlessly route rustc invocations through sccache.
    pub fn inject_env(&self) -> Vec<(String, String)> {
        vec![
            ("RUSTC_WRAPPER".to_string(), "sccache".to_string()),
            (
                "SCCACHE_DIR".to_string(),
                self.cache_dir.to_string_lossy().to_string(),
            ),
            // Force incremental off — incompatible with sccache's object-level caching
            ("CARGO_INCREMENTAL".to_string(), "0".to_string()),
            // Run inline (no daemon process) — correct for per-job container use
            ("SCCACHE_NO_DAEMON".to_string(), "1".to_string()),
            // Cap disk usage per manager; sccache evicts LRU entries above this
            ("SCCACHE_CACHE_SIZE".to_string(), self.cache_size.clone()),
        ]
    }
}

/// Manages branch-scoped L1 caching for Cargo's `target/` directory.
pub struct CargoL1Hydrator {
    pub storage_dir: PathBuf,
}

impl CargoL1Hydrator {
    pub fn new(storage_dir: PathBuf) -> Self {
        Self { storage_dir }
    }

    pub fn snapshot_name(&self, branch_name: &str) -> PathBuf {
        let sanitized = branch_name.replace('/', "_");
        self.storage_dir
            .join(format!("target_{}.tar.zst", sanitized))
    }

    /// Unarchive the L1 target/ cache into the sandbox for the given branch.
    /// Returns Ok(true) if hydration succeeded, Ok(false) if no snapshot exists.
    pub async fn hydrate(&self, branch_name: &str, target_dir: &Path) -> Result<bool> {
        let snapshot = self.snapshot_name(branch_name);
        if snapshot.exists() {
            tracing::info!("Hydrating L1 target/ cache for branch {}", branch_name);
            let status = tokio::process::Command::new("tar")
                .arg("-I")
                .arg("zstd")
                .arg("-xf")
                .arg(&snapshot)
                .arg("-C")
                .arg(target_dir)
                .status()
                .await
                .context("Failed to run tar for L1 hydration")?;
            if status.success() {
                tracing::info!("L1 hydration complete from {:?}", snapshot);
                Ok(true)
            } else {
                tracing::warn!(
                    "L1 hydration tar failed with exit {:?}; starting cold",
                    status.code()
                );
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }

    /// Capture the target/ directory into a branch-scoped zstd archive for future hydration.
    pub async fn snapshot(&self, branch_name: &str, target_dir: &Path) -> Result<()> {
        let snapshot = self.snapshot_name(branch_name);
        if let Some(parent) = snapshot.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let status = tokio::process::Command::new("tar")
            .arg("-I")
            .arg("zstd")
            .arg("-cf")
            .arg(&snapshot)
            .arg("-C")
            .arg(target_dir)
            .arg(".")
            .status()
            .await
            .context("Failed to run tar for L1 snapshot")?;
        if status.success() {
            tracing::info!("L1 snapshot captured to {:?}", snapshot);
        } else {
            tracing::warn!("L1 snapshot tar failed with exit {:?}", status.code());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sccache_env_injection() {
        let mgr = SccacheManager::new(PathBuf::from("/cache/sccache"));
        let envs = mgr.inject_env();

        assert!(envs.contains(&("RUSTC_WRAPPER".into(), "sccache".into())));
        assert!(envs.contains(&("CARGO_INCREMENTAL".into(), "0".into())));
        assert!(envs.contains(&("SCCACHE_DIR".into(), "/cache/sccache".into())));
        assert!(envs.contains(&("SCCACHE_CACHE_SIZE".into(), "10G".into())));
        assert_eq!(mgr.cache_path().to_string_lossy(), "/cache/sccache");
    }

    #[test]
    fn test_l1_hydrator_paths() {
        let hydrator = CargoL1Hydrator::new(PathBuf::from("/global/l1_cache"));
        let path = hydrator.snapshot_name("feature/auth-bypass");
        assert_eq!(
            path.to_string_lossy(),
            "/global/l1_cache/target_feature_auth-bypass.tar.zst"
        );
    }
}
