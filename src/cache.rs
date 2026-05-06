//! Owner: SmartCache & Disk Management
//! Proof: `cargo test -p jeryu -- cache`
//! Invariants: LRU GC every 30 min; active-manager caches never collected; CAS atomic store

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tracing::info;
use walkdir::WalkDir;

/// Typed errors for SmartCache lifecycle.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("warp-registry failed to start: {0}")]
    RegistryFailed(String),
    #[error("Invalid Docker configuration written, rollback applied")]
    DockerConfigInvalid,
    #[error("Docker restart failed, rollback applied")]
    DockerRestartFailed,
    #[error("Docker daemon configuration validation failed")]
    DockerValidationFailed,
    #[error("df failed: {0}")]
    DfFailed(String),
    #[error("unexpected df output: {0}")]
    UnexpectedDfOutput(String),
    #[error("empty age")]
    EmptyAge,
    #[error("unsupported age '{0}'; use suffix m, h, or d")]
    UnsupportedAge(String),
    #[error("SmartCache health checks failed: proxy={0}, reg={1}, disk={2}")]
    HealthCheckFailed(bool, bool, bool),
    #[error("Corrupted data during CAS ingestion: mismatched hashes. given={0} computed={1}")]
    CasHashMismatch(String, String),
    #[error("docker system df failed: {0}")]
    DockerDfFailed(String),
    #[error("manager cache cleanup failed: {0}")]
    CleanupFailed(String),
}

const MIN_GITLAB_ARTIFACT_SIZE_MB: u64 = 4096;
const POOL_TARGET_LEASE_RECOVERY_TTL_SECS: u64 = 2 * 60 * 60;
const NEXTEST_EXTRACT_FALLBACK_TTL_SECS: u64 = 2 * 60 * 60;
pub const ROOT_DISK_HEADROOM_MIN_FREE_BYTES: u64 = 80 * 1024 * 1024 * 1024;
pub const ROOT_DISK_WARNING_MIN_FREE_BYTES: u64 = ROOT_DISK_HEADROOM_MIN_FREE_BYTES;
pub const ROOT_DISK_CRITICAL_MIN_FREE_BYTES: u64 = 60 * 1024 * 1024 * 1024;
pub const ROOT_DISK_EMERGENCY_MIN_FREE_BYTES: u64 = 40 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiskPressureLevel {
    Nominal,
    Warning,
    Critical,
    Emergency,
}

pub fn root_disk_pressure_level(available_bytes: u64) -> DiskPressureLevel {
    if available_bytes < ROOT_DISK_EMERGENCY_MIN_FREE_BYTES {
        DiskPressureLevel::Emergency
    } else if available_bytes < ROOT_DISK_CRITICAL_MIN_FREE_BYTES {
        DiskPressureLevel::Critical
    } else if available_bytes < ROOT_DISK_WARNING_MIN_FREE_BYTES {
        DiskPressureLevel::Warning
    } else {
        DiskPressureLevel::Nominal
    }
}

pub struct SmartCache {
    db: crate::state::Db,
    proxy_port: u16,
    registry_port: u16,
}

#[derive(Clone, Default)]
pub struct CacheManager;

#[derive(Clone, Debug)]
pub struct GcOptions {
    pub dry_run: bool,
    pub json: bool,
    pub keep_active_managers: bool,
    pub older_than: Option<String>,
    pub max_cache_gb: Option<f64>,
    pub quiet: bool,
}

impl Default for GcOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            json: false,
            keep_active_managers: true,
            older_than: None,
            max_cache_gb: None,
            quiet: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheStatusReport {
    pub generated_at: String,
    pub root_fs: FsUsage,
    pub jeryu_cache_bytes: u64,
    pub manager_cache_bytes: u64,
    pub manager_cache_budget_bytes: Option<u64>,
    pub manager_caches: Vec<ManagerCacheStatus>,
    pub local_cargo_target_bytes: u64,
    pub local_cargo_sccache_bytes: u64,
    pub pool_cargo_target_bytes: u64,
    pub pool_cargo_sccache_bytes: u64,
    pub cargo_target_caches: Vec<CargoTargetCacheStatus>,
    pub docker: DockerStorageSummary,
    pub proxy_up: bool,
    pub registry_up: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FsUsage {
    pub path: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ManagerCacheStatus {
    pub manager_id: String,
    pub path: String,
    pub bytes: u64,
    pub sccache_bytes: u64,
    pub active: bool,
    pub age_seconds: Option<u64>,
    pub gc_candidate: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CargoTargetCacheStatus {
    pub scope: String,
    pub path: String,
    pub bytes: u64,
    pub active: bool,
    pub lease_observed: bool,
    pub age_seconds: Option<u64>,
    pub gc_candidate: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DockerStorageSummary {
    pub images: Option<DockerStorageClass>,
    pub containers: Option<DockerStorageClass>,
    pub local_volumes: Option<DockerStorageClass>,
    pub build_cache: Option<DockerStorageClass>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DockerStorageClass {
    pub total_count: Option<u64>,
    pub active_count: Option<u64>,
    pub size: String,
    pub reclaimable: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheGcReport {
    pub dry_run: bool,
    pub deleted_manager_caches: Vec<String>,
    pub candidate_manager_caches: Vec<ManagerCacheStatus>,
    pub deleted_cargo_targets: Vec<String>,
    pub candidate_cargo_targets: Vec<CargoTargetCacheStatus>,
    pub reclaimed_cache_request_rows: u64,
    pub errors: Vec<String>,
}

pub async fn ensure_root_disk_headroom(required_free_bytes: u64, operation: &str) -> Result<()> {
    let usage = df_usage("/").await?;
    if usage.available_bytes < required_free_bytes {
        anyhow::bail!(
            "{operation} blocked: {} has {} free, need at least {}",
            usage.path,
            human_bytes(usage.available_bytes),
            human_bytes(required_free_bytes)
        );
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostDoctorReport {
    pub generated_at: String,
    pub ok: bool,
    pub checks: Vec<HostDoctorCheck>,
    pub cache: CacheStatusReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostDoctorCheck {
    pub id: String,
    pub ok: bool,
    pub detail: String,
}

impl SmartCache {
    pub fn new(db: crate::state::Db) -> Self {
        Self {
            db,
            proxy_port: crate::config::CACHE_PROXY_PORT,
            registry_port: crate::config::CACHE_REGISTRY_PORT,
        }
    }

    pub async fn start(self) -> Result<()> {
        info!("Starting SmartCache supervisor...");
        self.start_warp_registry().await?;

        let proxy = std::sync::Arc::new(crate::cache_proxy::CacheProxy::new(
            self.proxy_port,
            self.db.clone(),
        ));
        tokio::spawn(async move {
            if let Err(e) = proxy.start().await {
                tracing::error!("warp-proxy failed: {:?}", e);
            }
        });

        Ok(())
    }

    async fn start_warp_registry(&self) -> Result<()> {
        info!(
            "Ensuring warp-registry container is running on 127.0.0.1:{}",
            self.registry_port
        );

        // Stop and remove existing to be clean, or just check if it exists.
        // For simplicity, we just run docker run with --rm or --restart unless-stopped.
        let output = tokio::process::Command::new("docker")
            .args(["ps", "-q", "-f", "name=warp-registry"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            info!("warp-registry is already running");
            return Ok(());
        }

        let output = tokio::process::Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                "warp-registry",
                &format!("-p=0.0.0.0:{}:5000", self.registry_port),
                "--restart",
                "always",
                "-e",
                "REGISTRY_PROXY_REMOTEURL=https://registry-1.docker.io",
                "registry:2",
            ])
            .output()
            .await
            .context("Failed to start warp-registry")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(CacheError::RegistryFailed(err.into_owned()).into());
        }

        info!("Started warp-registry container");
        Ok(())
    }

    pub async fn enable(&self) -> Result<()> {
        println!("🔧 Enabling SmartCache Docker mirror...");
        let daemon_json = std::path::Path::new("/etc/docker/daemon.json");
        let mut config = if daemon_json.exists() {
            let content = std::fs::read_to_string(daemon_json)?;
            std::fs::write("/etc/docker/daemon.json.bak", &content)?;
            serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if let Some(obj) = config.as_object_mut() {
            let mirror = serde_json::json!([format!("http://127.0.0.1:{}", self.registry_port)]);
            obj.insert("registry-mirrors".to_string(), mirror);
        }

        std::fs::write(daemon_json, serde_json::to_string_pretty(&config)?)?;

        let valid = tokio::process::Command::new("sudo")
            .args([
                "dockerd",
                "--validate",
                "--config-file",
                daemon_json.to_str().unwrap(),
            ])
            .status()
            .await?;

        if !valid.success() {
            println!("Docker config validation failed, rolling back...");
            if std::path::Path::new("/etc/docker/daemon.json.bak").exists() {
                std::fs::copy("/etc/docker/daemon.json.bak", "/etc/docker/daemon.json")?;
            }
            return Err(CacheError::DockerConfigInvalid.into());
        }

        println!("Restarting Docker daemon...");
        let status = tokio::process::Command::new("sudo")
            .args(["systemctl", "restart", "docker"])
            .status()
            .await?;

        if !status.success() {
            println!("Docker failed to start, rolling back...");
            if std::path::Path::new("/etc/docker/daemon.json.bak").exists() {
                std::fs::copy("/etc/docker/daemon.json.bak", "/etc/docker/daemon.json")?;
                let _ = tokio::process::Command::new("sudo")
                    .args(["systemctl", "restart", "docker"])
                    .status()
                    .await;
            }
            return Err(CacheError::DockerRestartFailed.into());
        }

        println!("✅ SmartCache Docker mirror enabled");
        Ok(())
    }

    pub async fn doctor(&self) -> Result<()> {
        println!("🩺 Running SmartCache doctor...");
        println!("Checking proxy reachability ({})...", self.proxy_port);
        let proxy_up = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", self.proxy_port))
            .await
            .is_ok();
        println!("Checking registry mirror ({})...", self.registry_port);
        let reg_up = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", self.registry_port))
            .await
            .is_ok();

        println!("Checking local cache directory writeability...");
        let cache_dir = crate::config::data_dir().join("cache");
        std::fs::create_dir_all(&cache_dir)?;
        let test_file = cache_dir.join(".doctor_test");
        let disk_ok = std::fs::write(&test_file, b"ok").is_ok();
        let _ = std::fs::remove_file(test_file);

        if proxy_up && reg_up && disk_ok {
            println!("✅ SmartCache is healthy");
        } else {
            return Err(CacheError::HealthCheckFailed(proxy_up, reg_up, disk_ok).into());
        }
        Ok(())
    }

    pub async fn status(&self) -> Result<()> {
        self.status_with_options(false).await
    }

    pub async fn status_with_options(&self, json: bool) -> Result<()> {
        let report = self.status_report(None).await?;
        if json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            print_cache_status_report(&report);
        }
        Ok(())
    }

    pub async fn gc(&self) -> Result<()> {
        self.gc_with_options(GcOptions::default()).await.map(|_| ())
    }

    pub async fn gc_with_options(&self, options: GcOptions) -> Result<CacheGcReport> {
        let budget_bytes = options.max_cache_gb.map(gb_to_bytes);
        let mut status = self.status_report(budget_bytes).await?;
        let max_age = match options.older_than.as_deref() {
            Some(raw) => Some(parse_age(raw)?),
            None => None,
        };
        let total_cache_bytes = status.manager_cache_bytes
            + status.local_cargo_target_bytes
            + status.pool_cargo_target_bytes
            + status.local_cargo_sccache_bytes
            + status.pool_cargo_sccache_bytes;
        let over_budget = budget_bytes
            .map(|budget| total_cache_bytes > budget)
            .unwrap_or(false);

        for cache in &mut status.manager_caches {
            // Only skip active managers when explicitly asked to preserve them.
            // When keep_active_managers=false (emergency/critical pressure), fall through
            // to normal age/budget logic so active caches can be evicted.
            if cache.active && options.keep_active_managers {
                cache.gc_candidate = false;
                cache.reason = "active manager cache preserved".to_string();
                continue;
            }
            let old_enough = max_age
                .and_then(|age| cache.age_seconds.map(|seconds| seconds >= age.as_secs()))
                .unwrap_or(false);
            if max_age.is_none() || old_enough || over_budget {
                cache.gc_candidate = true;
                cache.reason = if cache.active {
                    if over_budget {
                        "active manager cache evicted: over global budget".to_string()
                    } else {
                        "active manager cache evicted: older than threshold".to_string()
                    }
                } else if over_budget {
                    "orphan manager cache selected because cache is over budget".to_string()
                } else if max_age.is_some() {
                    "orphan manager cache older than threshold".to_string()
                } else {
                    "orphan manager cache".to_string()
                };
            }
        }

        let candidates: Vec<ManagerCacheStatus> = status
            .manager_caches
            .iter()
            .filter(|cache| cache.gc_candidate)
            .cloned()
            .collect();
        for cache in &mut status.cargo_target_caches {
            if cache.active {
                cache.gc_candidate = false;
                cache.reason = "active cargo target cache preserved".to_string();
                continue;
            }
            let old_enough = max_age
                .and_then(|age| cache.age_seconds.map(|seconds| seconds >= age.as_secs()))
                .unwrap_or(false);
            if max_age.is_none() || old_enough || over_budget {
                cache.gc_candidate = true;
                cache.reason = if over_budget {
                    "cargo target cache selected because cache is over budget".to_string()
                } else if max_age.is_some() {
                    "cargo target cache older than threshold".to_string()
                } else {
                    "cargo target cache".to_string()
                };
            }
        }
        let cargo_candidates: Vec<CargoTargetCacheStatus> = status
            .cargo_target_caches
            .iter()
            .filter(|cache| cache.gc_candidate)
            .cloned()
            .collect();
        let mut deleted = Vec::new();
        let mut errors = Vec::new();
        let mut deleted_cargo = Vec::new();

        if !options.dry_run && !candidates.is_empty() {
            match remove_manager_cache_dirs_as_root(&candidates).await {
                Ok(removed) => deleted = removed,
                Err(err) => errors.push(err.to_string()),
            }
        }
        if !options.dry_run && !cargo_candidates.is_empty() {
            let paths: Vec<PathBuf> = cargo_candidates
                .iter()
                .map(|cache| PathBuf::from(&cache.path))
                .collect();
            match remove_cache_paths_as_root(&crate::config::cache_root_dir(), &paths).await {
                Ok(removed) => deleted_cargo = removed,
                Err(err) => errors.push(err.to_string()),
            }
        }

        let reclaimed = self.db.prune_cache_requests(7).await?;
        if !options.dry_run {
            let cutoff = (Utc::now() - ChronoDuration::days(7)).to_rfc3339();
            let _ = self.db.prune_test_verdicts(&cutoff).await?;
            let _ = self.db.prune_action_cache(&cutoff).await?;
        }
        let report = CacheGcReport {
            dry_run: options.dry_run,
            deleted_manager_caches: deleted,
            candidate_manager_caches: candidates,
            deleted_cargo_targets: deleted_cargo,
            candidate_cargo_targets: cargo_candidates,
            reclaimed_cache_request_rows: reclaimed,
            errors,
        };

        if !options.quiet {
            if options.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print_cache_gc_report(&report);
            }
        }
        Ok(report)
    }

    pub async fn host_doctor_report(&self) -> Result<HostDoctorReport> {
        let cache = self.status_report(Some(gb_to_bytes(400.0))).await?;
        let mut checks = Vec::new();

        checks.push(HostDoctorCheck {
            id: "root-disk-free".to_string(),
            ok: cache.root_fs.available_bytes >= ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
            detail: format!(
                "{} free on {}",
                human_bytes(cache.root_fs.available_bytes),
                cache.root_fs.path
            ),
        });
        checks.push(HostDoctorCheck {
            id: "runner-cache-budget".to_string(),
            ok: cache.manager_cache_bytes <= gb_to_bytes(400.0),
            detail: format!(
                "{} in manager caches",
                human_bytes(cache.manager_cache_bytes)
            ),
        });
        checks.push(HostDoctorCheck {
            id: "smartcache-proxy".to_string(),
            ok: cache.proxy_up,
            detail: format!(
                "proxy {}",
                if cache.proxy_up { "reachable" } else { "down" }
            ),
        });
        checks.push(HostDoctorCheck {
            id: "smartcache-registry".to_string(),
            ok: cache.registry_up,
            detail: format!(
                "registry mirror {}",
                if cache.registry_up {
                    "reachable"
                } else {
                    "down"
                }
            ),
        });
        checks.push(gitlab_redis_write_check().await);
        checks.push(gitlab_artifact_size_check().await);

        let ok = checks.iter().all(|check| check.ok);
        Ok(HostDoctorReport {
            generated_at: now_rfc3339(),
            ok,
            checks,
            cache,
        })
    }

    pub async fn status_report(&self, budget_bytes: Option<u64>) -> Result<CacheStatusReport> {
        let proxy_up = tcp_up(self.proxy_port).await;
        let registry_up = tcp_up(self.registry_port).await;
        let active_managers = active_runner_manager_ids().await;
        let mut active_pool_names: BTreeSet<String> = BTreeSet::new();
        for pool in self.db.list_pools().await? {
            if self.db.count_active_managers(&pool.name).await.unwrap_or(0) > 0 {
                active_pool_names.insert(pool.name);
            }
        }
        let manager_root = crate::config::cache_root_dir().join("managers");
        let mut manager_caches = Vec::new();

        if manager_root.is_dir() {
            for entry in std::fs::read_dir(&manager_root)
                .with_context(|| format!("reading {}", manager_root.display()))?
            {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let manager_id = entry.file_name().to_string_lossy().to_string();
                let path = entry.path();
                let active = active_managers.contains(&manager_id);
                let bytes = du_bytes(&path).await.unwrap_or(0);
                let sccache_bytes = du_bytes(&path.join("sccache")).await.unwrap_or(0);
                let age_seconds = path_age_seconds(&path);
                manager_caches.push(ManagerCacheStatus {
                    manager_id,
                    path: path.display().to_string(),
                    bytes,
                    sccache_bytes,
                    active,
                    age_seconds,
                    gc_candidate: false,
                    reason: if active {
                        "active manager cache".to_string()
                    } else {
                        "orphan manager cache".to_string()
                    },
                });
            }
        }

        manager_caches.sort_by(|a, b| {
            b.bytes
                .cmp(&a.bytes)
                .then_with(|| a.manager_id.cmp(&b.manager_id))
        });

        let local_cargo_targets =
            scan_cargo_target_dirs(&crate::config::local_cargo_targets_root(), "local").await?;
        let local_cargo_target_bytes = local_cargo_targets.iter().map(|cache| cache.bytes).sum();
        let local_cargo_sccache_bytes = du_bytes(&crate::config::local_cargo_sccache_dir())
            .await
            .unwrap_or(0);

        let mut cargo_target_caches = local_cargo_targets;
        let pool_root = crate::config::cache_root_dir().join("pools");
        let mut pool_cargo_target_bytes = 0_u64;
        let mut pool_cargo_sccache_bytes = 0_u64;
        if pool_root.is_dir() {
            for entry in std::fs::read_dir(&pool_root)
                .with_context(|| format!("reading {}", pool_root.display()))?
            {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let pool_name = entry.file_name().to_string_lossy().to_string();
                let pool_cache_dir = entry.path();
                let mut statuses = scan_cargo_target_dirs(
                    &pool_cache_dir.join("cargo-targets"),
                    &format!("pool:{pool_name}"),
                )
                .await?;
                if active_pool_names.contains(&pool_name) {
                    for status in &mut statuses {
                        if status.active {
                            continue;
                        }
                        let ttl = pool_target_lease_recovery_ttl().as_secs();
                        let recovery_active = !status.lease_observed
                            && status.age_seconds.map(|age| age <= ttl).unwrap_or(true);
                        if recovery_active {
                            status.active = true;
                            status.reason =
                                "active pool cargo cache recovery path (lease absent)".to_string();
                        } else if !status.lease_observed {
                            status.reason = "pool cargo cache without lease".to_string();
                        }
                    }
                }
                pool_cargo_target_bytes += statuses.iter().map(|status| status.bytes).sum::<u64>();
                pool_cargo_sccache_bytes +=
                    du_bytes(&pool_cache_dir.join("sccache")).await.unwrap_or(0);
                cargo_target_caches.extend(statuses);
            }
        }
        cargo_target_caches.sort_by(|a, b| {
            b.bytes
                .cmp(&a.bytes)
                .then_with(|| a.scope.cmp(&b.scope))
                .then_with(|| a.path.cmp(&b.path))
        });

        let manager_cache_bytes = manager_caches.iter().map(|cache| cache.bytes).sum();
        Ok(CacheStatusReport {
            generated_at: now_rfc3339(),
            root_fs: df_usage("/").await?,
            jeryu_cache_bytes: du_bytes(&crate::config::cache_root_dir())
                .await
                .unwrap_or(0),
            manager_cache_bytes,
            manager_cache_budget_bytes: budget_bytes,
            manager_caches,
            local_cargo_target_bytes,
            local_cargo_sccache_bytes,
            pool_cargo_target_bytes,
            pool_cargo_sccache_bytes,
            cargo_target_caches,
            docker: match docker_storage_summary().await {
                Ok(summary) => summary,
                Err(_) => DockerStorageSummary::default(),
            },
            proxy_up,
            registry_up,
        })
    }

    /// Store a blob in CAS with Scratch -> Hash -> Fsync -> Rename safety pattern
    pub async fn atomic_store_cas(data: &[u8], digest: &str) -> Result<()> {
        let cas_dir = crate::config::data_dir().join("cas");
        tokio::fs::create_dir_all(&cas_dir).await?;

        let path = cas_dir.join(digest);
        if path.exists() {
            return Ok(());
        }

        let scratch_path = cas_dir.join(format!("{}.tmp", digest));

        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&scratch_path)
            .await?;

        // Verify hash actually matches the content before saving
        use sha2::Digest;
        let result = sha2::Sha256::digest(data);
        let computed_digest = hex::encode(result);
        if computed_digest != digest {
            return Err(CacheError::CasHashMismatch(digest.to_string(), computed_digest).into());
        }

        file.write_all(data).await?;
        // fsync to persist before directory entry
        file.sync_all().await?;

        // Atomic rename
        tokio::fs::rename(scratch_path, path).await?;
        Ok(())
    }
}
#[path = "cache_reports.rs"]
mod cache_reports;
pub use cache_reports::*;
