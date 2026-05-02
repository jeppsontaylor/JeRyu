//! Owner: Shadow Remote Mirroring
//! Proof: `cargo test -p vgit -- shadow`
//! Invariants: Mirror operations are idempotent; push failures do not block the primary pipeline; git2 errors are always surfaced via anyhow context

use crate::gitlab_client::GitlabClient;
use crate::state::{Db, ShadowSyncConfig};
use anyhow::{Context, Result, bail};
use git2::{Cred, PushOptions, RemoteCallbacks, Repository};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tracing::{error, warn};

type ShadowPushOutcome = Option<(String, Option<Result<(), String>>)>;

#[derive(Debug, Clone)]
pub struct RemoteStatus {
    pub name: String,
    pub fetch_url: Option<String>,
    pub push_url: String,
}

#[derive(Debug, Clone)]
pub struct ShadowStatus {
    pub repo_root: PathBuf,
    pub head_branch: Option<String>,
    pub target_remote: String,
    pub target_exists: bool,
    pub remotes: Vec<RemoteStatus>,
}

pub fn status(repo: Option<&Path>, target_remote: &str) -> Result<ShadowStatus> {
    let repo = open_repo(repo)?;
    let repo_root = repo
        .workdir()
        .or_else(|| repo.path().parent())
        .context("failed to resolve repository root")?
        .to_path_buf();
    let head_branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(str::to_string));
    let remotes = repo
        .remotes()
        .context("failed to list remotes")?
        .iter()
        .flatten()
        .map(|name| {
            let remote = repo
                .find_remote(name)
                .with_context(|| format!("failed to inspect remote '{name}'"))?;
            Ok(RemoteStatus {
                name: name.to_string(),
                fetch_url: remote.url().map(str::to_string),
                push_url: remote
                    .pushurl()
                    .or_else(|| remote.url())
                    .unwrap_or("(none)")
                    .to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let target_exists = remotes.iter().any(|remote| remote.name == target_remote);

    Ok(ShadowStatus {
        repo_root,
        head_branch,
        target_remote: target_remote.to_string(),
        target_exists,
        remotes,
    })
}

pub fn ensure_remote(repo: Option<&Path>, name: &str, url: &str) -> Result<()> {
    let repo = open_repo(repo)?;
    let repo_root = repo
        .workdir()
        .or_else(|| repo.path().parent())
        .context("failed to resolve repository root")?;
    if repo.find_remote(name).is_ok() {
        run_git(repo_root, ["remote", "set-url", name, url])?;
    } else {
        run_git(repo_root, ["remote", "add", name, url])?;
    }
    run_git(repo_root, ["remote", "set-url", "--push", name, url])?;
    Ok(())
}

pub fn push_remote(
    repo: Option<&Path>,
    name: &str,
    branch: Option<&str>,
    mirror: bool,
) -> Result<()> {
    let repo = open_repo(repo)?;
    let repo_root = repo
        .workdir()
        .or_else(|| repo.path().parent())
        .context("failed to resolve repository root")?;
    if mirror {
        run_git(repo_root, ["push", "--mirror", name])?;
        return Ok(());
    }

    let branch_name = match branch {
        Some(branch) => branch.to_string(),
        None => repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(str::to_string))
            .context("detached HEAD; pass --branch explicitly")?,
    };
    let refspec = format!("HEAD:refs/heads/{branch_name}");
    run_git(repo_root, ["push", name, &refspec])?;
    Ok(())
}

fn open_repo(repo: Option<&Path>) -> Result<Repository> {
    let path = repo.unwrap_or_else(|| Path::new("."));
    Repository::discover(path)
        .with_context(|| format!("failed to discover git repository from {}", path.display()))
}

fn run_git<const N: usize>(repo_root: &Path, args: [&str; N]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .status()
        .with_context(|| format!("failed to run git in {}", repo_root.display()))?;
    if !status.success() {
        bail!("git command failed in {}", repo_root.display());
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct ShadowSyncSummary {
    pub enabled_count: usize,
    pub syncing_count: usize,
    pub error_count: usize,
    pub display_text: String,
    pub upstream_url: Option<String>,
    pub upstream_status: String,
    pub upstream_gap: Option<usize>,
}

pub async fn run_shadow_loop(db: Db, client: GitlabClient) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        let configs = match db.list_shadow_sync_configs().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to list shadow sync configs: {}", e);
                continue;
            }
        };

        for mut config in configs {
            if !config.enabled {
                continue;
            }

            // Honour immediate sync request — skip backoff entirely.
            let forced = config.status == "sync_requested";

            // Exponential backoff (skipped when forced)
            if !forced && config.consecutive_failures > 0 {
                let backoff_secs = 2_u64.pow(config.consecutive_failures.min(4) as u32);
                let last_attempt = config
                    .last_attempt_at
                    .as_deref()
                    .unwrap_or("1970-01-01T00:00:00Z");
                if let Ok(last) = chrono::DateTime::parse_from_rfc3339(last_attempt) {
                    let now = chrono::Utc::now();
                    if now.signed_duration_since(last).num_seconds() < backoff_secs as i64 {
                        continue;
                    }
                }
            }

            config.last_attempt_at = Some(chrono::Utc::now().to_rfc3339());
            let source_dir = config.source_dir.clone();

            match sync_once(&db, &client, &mut config).await {
                Ok(true) => {
                    config.status = "idle".to_string();
                    config.error_msg = None;
                    config.consecutive_failures = 0;
                    config.last_success_at = Some(chrono::Utc::now().to_rfc3339());
                }
                Ok(false) => {
                    // No new commits, stay ok
                }
                Err(e) => {
                    warn!("Shadow sync failed for {}: {:#}", source_dir, e);
                    config.status = "error".to_string();
                    config.error_msg = Some(e.to_string());
                    config.consecutive_failures += 1;
                }
            }

            let _ = db.upsert_shadow_sync_config(&config).await;
        }
    }
}

async fn sync_once(_db: &Db, client: &GitlabClient, config: &mut ShadowSyncConfig) -> Result<bool> {
    // We use tokio::task::spawn_blocking because git2 blocks the thread.
    let source_dir = config.source_dir.clone();
    let target_branch = config.target_branch.clone();
    let target_project_id = config.target_project_id;
    let pat_opt = client.pat_value_for_clone();
    let project = client.get_project(target_project_id).await?;
    let remote_url =
        format!("{}.git", project.web_url).replace(crate::config::GITLAB_HOSTNAME, "localhost");

    let last_pushed_sha = config.last_pushed_sha.clone();
    let status = config.status.clone();

    tokio::task::spawn_blocking(move || -> Result<ShadowPushOutcome> {
        let repo = Repository::open(&source_dir).context("failed to open local repository")?;

        // Resolve HEAD
        let head = repo.head().context("failed to resolve HEAD")?;
        let head_commit = head
            .peel_to_commit()
            .context("failed to peel HEAD to commit")?;
        let head_sha = head_commit.id().to_string();

        if let Some(ref pushed_sha) = last_pushed_sha
            && pushed_sha == &head_sha
            && status != "error"
        {
            // No changes
            return Ok(None);
        }

        // We need to push to shadow server
        let mut remote = repo.remote_anonymous(&remote_url)?;

        let mut callbacks = RemoteCallbacks::new();
        if let Some(pat) = &pat_opt {
            let pat_clone = pat.clone();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("oauth2", &pat_clone)
            });
        }

        let mut push_options = PushOptions::new();
        push_options.remote_callbacks(callbacks);

        let refspec = format!(
            "+{}:refs/heads/{}",
            head.name().unwrap_or("HEAD"),
            target_branch
        );

        remote
            .push(&[&refspec], Some(&mut push_options))
            .context("failed to push to remote")?;

        // Also push upstream if configured!
        let mut upstream_res = None;
        if let Some(upstream_url) = crate::settings::get().shadow.upstream_url.clone() {
            let out = std::process::Command::new("git")
                .args(["push", &upstream_url, &refspec])
                .current_dir(&source_dir)
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    upstream_res = Some(Ok(()));
                }
                Ok(o) => {
                    upstream_res = Some(Err(String::from_utf8_lossy(&o.stderr).into_owned()));
                }
                Err(e) => {
                    upstream_res = Some(Err(e.to_string()));
                }
            }
        }

        Ok(Some((head_sha, upstream_res)))
    })
    .await?
    .map(|opt_res| {
        if let Some((sha, up_res)) = opt_res {
            config.last_seen_head_sha = Some(sha.clone());
            config.last_pushed_sha = Some(sha.clone());

            // Handle upstream outcome
            if let Some(res) = up_res {
                match res {
                    Ok(_) => {
                        config.upstream_status = "ok".into();
                        config.upstream_last_pushed_sha = Some(sha);
                        config.upstream_error_msg = None;
                    }
                    Err(e) => {
                        config.upstream_status = "error".into();
                        config.upstream_error_msg = Some(e);
                    }
                }
            }
            return true;
        }
        false
    })
}

pub async fn compute_summary(db: &Db) -> Result<Option<ShadowSyncSummary>> {
    let configs = db.list_shadow_sync_configs().await?;
    let mut summary = ShadowSyncSummary::default();

    if configs.is_empty() {
        return Ok(None);
    }

    for c in &configs {
        if c.enabled {
            summary.enabled_count += 1;
        }
        if c.status == "syncing" {
            summary.syncing_count += 1;
        }
        if c.status == "error" {
            summary.error_count += 1;
        }
    }

    if configs.len() == 1 {
        let c = &configs[0];
        let sha = c.last_pushed_sha.as_deref().unwrap_or("empty");
        summary.display_text = format!(
            "[SHADOW {} @ {}]",
            c.target_branch,
            sha.chars().take(7).collect::<String>()
        );
        summary.upstream_status = c.upstream_status.clone();
        if let Some(url) = crate::settings::get().shadow.upstream_url.clone() {
            summary.upstream_url = Some(url);

            // Calculate commit gap heuristically (number of commits ahead of upstream).
            // We do a fast count if we have the local sha and upstream sha.
            if let (Some(head), Some(up)) = (&c.last_pushed_sha, &c.upstream_last_pushed_sha) {
                if head == up {
                    summary.upstream_gap = Some(0);
                } else {
                    let repo_root = std::path::Path::new(&c.source_dir);
                    if let Ok(output) = std::process::Command::new("git")
                        .args(["rev-list", "--count", &format!("{}...{}", up, head)])
                        .current_dir(repo_root)
                        .output()
                    {
                        let s = String::from_utf8_lossy(&output.stdout);
                        if let Ok(gap) = s.trim().parse::<usize>() {
                            summary.upstream_gap = Some(gap);
                        } else {
                            summary.upstream_gap = Some(999);
                        }
                    } else {
                        summary.upstream_gap = Some(999);
                    }
                }
            } else if c.last_pushed_sha.is_some() {
                summary.upstream_gap = Some(999);
            }
        }
    } else {
        summary.display_text = format!(
            "[SHADOW {} enabled | {} syncing | {} error]",
            summary.enabled_count, summary.syncing_count, summary.error_count
        );
        summary.upstream_status = "unconfigured".into();
    }

    Ok(Some(summary))
}
