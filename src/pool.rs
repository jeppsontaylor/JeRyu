//! Owner: Runner Fleet / Pool Management
//! Proof: `cargo test -p jeryu -- pool`
//! Invariants: Pool→Manager is 1:N; SIGQUIT for graceful drain; SIGHUP for token hot-reload
//!
//! A pool is a logical runner configuration in GitLab backed by
//! 0-N runner-manager containers on the local Docker host.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use tracing::{info, warn};

use crate::config;
use crate::docker::DockerCtl;
use crate::gitlab_client::GitlabClient;
use crate::state::{Db, Manager, Pool};
use tokio::time::{Duration, Instant, sleep};

// ---------------------------------------------------------------------------
// Scale: bring manager count to target
// ---------------------------------------------------------------------------

fn manager_state_counts_as_active(state: &str) -> bool {
    matches!(state, "starting" | "online")
}

fn manager_has_running_container(
    manager: &Manager,
    running_container_ids: &BTreeSet<String>,
) -> bool {
    running_container_ids.contains(&manager.docker_container_id)
}

pub async fn reconcile_manager_runtime_state(
    db: &Db,
    docker: &DockerCtl,
    pool_name: Option<&str>,
) -> Result<usize> {
    let running_container_ids = docker.running_managed_container_ids().await?;
    let managers = db.list_managers(pool_name).await?;
    let mut stopped = 0;

    for manager in managers
        .iter()
        .filter(|manager| manager_state_counts_as_active(&manager.state))
        .filter(|manager| !manager_has_running_container(manager, &running_container_ids))
    {
        warn!(
            manager_id = %manager.id,
            pool = %manager.pool_name,
            container_id = %manager.docker_container_id,
            previous_state = %manager.state,
            "marking stale runner manager stopped; Docker container is not running"
        );
        db.update_manager_state(&manager.id, "stopped").await?;
        stopped += 1;
    }

    Ok(stopped)
}

pub async fn count_running_managers(db: &Db, docker: &DockerCtl, pool_name: &str) -> Result<i64> {
    let running_container_ids = docker.running_managed_container_ids().await?;
    let managers = db.list_managers(Some(pool_name)).await?;
    Ok(managers
        .iter()
        .filter(|manager| manager_state_counts_as_active(&manager.state))
        .filter(|manager| manager_has_running_container(manager, &running_container_ids))
        .count() as i64)
}

async fn remove_manager_cache_dir(docker: &DockerCtl, manager_id: &str) {
    let cache_dir = config::manager_cache_dir(manager_id);
    if !cache_dir.exists() {
        return;
    }
    if let Err(err) = docker.remove_cache_dir_as_root(&cache_dir).await {
        warn!(manager_id, path = %cache_dir.display(), error = %err, "failed to remove manager cache dir");
    }
}

async fn start_manager(db: &Db, docker: &DockerCtl, pool: &Pool, pool_name: &str) -> Result<()> {
    let manager_id = uuid::Uuid::new_v4().to_string();
    let config_dir = config::runners_dir()
        .join(&manager_id)
        .display()
        .to_string();
    let manager_cache_dir = config::manager_cache_dir(&manager_id);
    let pool_cache_dir = config::pool_cache_root(pool_name);
    let pool_targets_dir = config::pool_cargo_targets_root(pool_name);
    let pool_sccache_dir = config::pool_cargo_sccache_dir(pool_name);

    fs::create_dir_all(&config_dir)
        .with_context(|| format!("creating config dir: {config_dir}"))?;
    fs::create_dir_all(&manager_cache_dir)
        .with_context(|| format!("creating cache dir: {}", manager_cache_dir.display()))?;
    fs::create_dir_all(&pool_targets_dir)
        .with_context(|| format!("creating pool targets dir: {}", pool_targets_dir.display()))?;
    fs::create_dir_all(&pool_sccache_dir)
        .with_context(|| format!("creating pool sccache dir: {}", pool_sccache_dir.display()))?;

    let gitlab_url = format!(
        "http://{}:{}",
        config::GITLAB_HOSTNAME,
        config::GITLAB_HTTP_PORT
    );
    let config_content = config::render_runner_config(
        pool_name,
        &manager_id,
        &gitlab_url,
        &pool.auth_token,
        &pool.executor,
        &pool_cache_dir.display().to_string(),
        pool.concurrent,
        pool.request_concurrency,
    );
    fs::write(format!("{config_dir}/config.toml"), &config_content)?;

    let container_id = docker
        .start_runner_manager(
            &manager_id,
            &config_dir,
            &manager_cache_dir.display().to_string(),
            &pool_cache_dir.display().to_string(),
            &pool.executor,
            None,
        )
        .await
        .with_context(|| format!("starting manager for pool '{pool_name}'"))?;

    let manager = Manager {
        id: manager_id.clone(),
        pool_name: pool_name.to_string(),
        docker_container_id: container_id,
        system_id: None,
        state: "starting".to_string(),
        config_dir,
        started_at: Some(chrono::Utc::now().to_rfc3339()),
        last_contact_at: None,
    };
    db.insert_manager(&manager).await?;

    info!(manager_id, pool = pool_name, "started new manager");
    Ok(())
}

/// Scale a pool to exactly `target` active managers. Returns the number
/// of managers started (may be 0 if already at target or scaling down).
pub async fn scale_pool_to(
    db: &Db,
    docker: &DockerCtl,
    _client: &GitlabClient,
    pool_name: &str,
    target: usize,
) -> Result<usize> {
    let pool = db
        .get_pool(pool_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pool '{}' not found", pool_name))?;

    reconcile_manager_runtime_state(db, docker, Some(pool_name)).await?;
    let active = db.count_active_managers(pool_name).await? as usize;

    if active == target {
        info!(pool = pool_name, active, target, "pool already at target");
        return Ok(0);
    }

    if active > target {
        // Scale down: drain excess managers
        let excess = active - target;
        let managers = db.list_managers(Some(pool_name)).await?;
        let to_drain: Vec<_> = managers
            .iter()
            .filter(|m| m.state == "online" || m.state == "starting")
            .take(excess)
            .collect();

        for m in &to_drain {
            info!(manager_id = %m.id, pool = pool_name, "draining excess manager");
            db.update_manager_state(&m.id, "draining").await?;
            docker
                .cleanup_runner_cache(&m.docker_container_id)
                .await
                .ok();
            docker
                .drain_runner_manager(
                    &m.docker_container_id,
                    config::runner_shutdown_timeout_secs() as i64,
                )
                .await
                .ok(); // best-effort drain
            docker
                .cleanup_runner_cache(&m.docker_container_id)
                .await
                .ok();
            docker
                .remove_runner_manager(&m.docker_container_id)
                .await
                .ok();
            remove_manager_cache_dir(docker, &m.id).await;
            db.update_manager_state(&m.id, "stopped").await?;
        }

        let active_after_drain = db.count_active_managers(pool_name).await? as usize;
        if active_after_drain < target {
            for _ in 0..(target - active_after_drain) {
                start_manager(db, docker, &pool, pool_name).await?;
            }
        }
        wait_for_active_managers(db, pool_name, target as i64, Duration::from_secs(90)).await?;
        return Ok(0);
    }

    // Scale up: start new managers
    crate::cache::ensure_root_disk_headroom(
        crate::cache::ROOT_DISK_HEADROOM_MIN_FREE_BYTES,
        "runner fanout",
    )
    .await?;
    let to_start = target - active;
    let mut started = 0;

    for _ in 0..to_start {
        start_manager(db, docker, &pool, pool_name).await?;
        started += 1;
    }

    wait_for_active_managers(db, pool_name, target as i64, Duration::from_secs(90)).await?;
    Ok(started)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(state: &str, docker_container_id: &str) -> Manager {
        Manager {
            id: "manager-id".into(),
            pool_name: "default".into(),
            docker_container_id: docker_container_id.into(),
            system_id: None,
            state: state.into(),
            config_dir: "/tmp/manager".into(),
            started_at: None,
            last_contact_at: None,
        }
    }

    #[test]
    fn active_manager_requires_running_container() {
        let running = BTreeSet::from(["container-a".to_string()]);
        assert!(manager_has_running_container(
            &manager("online", "container-a"),
            &running
        ));
        assert!(!manager_has_running_container(
            &manager("online", "container-b"),
            &running
        ));
    }

    #[test]
    fn only_starting_and_online_count_as_active_states() {
        assert!(manager_state_counts_as_active("starting"));
        assert!(manager_state_counts_as_active("online"));
        assert!(!manager_state_counts_as_active("draining"));
        assert!(!manager_state_counts_as_active("stopped"));
        assert!(!manager_state_counts_as_active("failed"));
    }
}

// ---------------------------------------------------------------------------
// Pause / Resume
// ---------------------------------------------------------------------------

/// Pause a pool in GitLab (stops accepting new jobs) but keeps managers alive.
pub async fn pause_pool(db: &Db, client: &GitlabClient, pool_name: &str) -> Result<()> {
    let pool = db
        .get_pool(pool_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pool '{}' not found", pool_name))?;

    client
        .set_runner_paused(pool.gitlab_runner_id, true)
        .await?;
    db.update_pool_paused(pool_name, true).await?;

    info!(pool = pool_name, "paused pool");
    Ok(())
}

/// Resume a paused pool.
pub async fn resume_pool(db: &Db, client: &GitlabClient, pool_name: &str) -> Result<()> {
    let pool = db
        .get_pool(pool_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pool '{}' not found", pool_name))?;

    client
        .set_runner_paused(pool.gitlab_runner_id, false)
        .await?;
    db.update_pool_paused(pool_name, false).await?;

    info!(pool = pool_name, "resumed pool");
    Ok(())
}

// ---------------------------------------------------------------------------
// Drain
// ---------------------------------------------------------------------------

/// Drain a pool: pause in GitLab, then SIGQUIT all managers, wait for
/// current jobs to finish, then stop and remove all manager containers.
pub async fn drain_pool(
    db: &Db,
    docker: &DockerCtl,
    client: &GitlabClient,
    pool_name: &str,
) -> Result<()> {
    // First pause so no new jobs are assigned
    pause_pool(db, client, pool_name).await?;

    // Then drain all managers
    let managers = db.list_managers(Some(pool_name)).await?;
    for m in &managers {
        if m.state == "online" || m.state == "starting" {
            info!(manager_id = %m.id, "draining manager");
            db.update_manager_state(&m.id, "draining").await?;

            // SIGQUIT: stop accepting new builds, exit after current finish
            docker
                .cleanup_runner_cache(&m.docker_container_id)
                .await
                .ok();
            docker
                .drain_runner_manager(
                    &m.docker_container_id,
                    config::runner_shutdown_timeout_secs() as i64,
                )
                .await
                .ok();

            // Remove the container
            docker
                .cleanup_runner_cache(&m.docker_container_id)
                .await
                .ok();
            docker
                .remove_runner_manager(&m.docker_container_id)
                .await
                .ok();
            remove_manager_cache_dir(docker, &m.id).await;
            db.update_manager_state(&m.id, "stopped").await?;

            info!(manager_id = %m.id, "manager drained and stopped");
        }
    }

    wait_for_active_managers(db, pool_name, 0, Duration::from_secs(90)).await?;
    info!(pool = pool_name, "pool fully drained");
    Ok(())
}

/// Delete a pool after draining managers and removing the GitLab runner.
pub async fn delete_pool(
    db: &Db,
    docker: &DockerCtl,
    client: &GitlabClient,
    pool_name: &str,
) -> Result<()> {
    let pool = db
        .get_pool(pool_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pool '{}' not found", pool_name))?;

    drain_pool(db, docker, client, pool_name).await.ok();
    client.delete_runner(pool.gitlab_runner_id).await.ok();
    db.delete_pool(pool_name).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Token rotation
// ---------------------------------------------------------------------------

/// Rotate the auth token for a pool. This:
/// 1. Calls GitLab to reset the runner's auth token
/// 2. Rewrites all manager config.toml files with the new token
/// 3. Sends SIGHUP to all running managers for hot-reload
/// 4. Updates the database and jeryu.env
pub async fn rotate_pool_token(
    db: &Db,
    docker: &DockerCtl,
    client: &GitlabClient,
    pool_name: &str,
) -> Result<String> {
    let pool = db
        .get_pool(pool_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("pool '{}' not found", pool_name))?;

    // 1. Reset token in GitLab
    let new_token = client.reset_runner_token(pool.gitlab_runner_id).await?;
    info!(pool = pool_name, "got new runner auth token");

    // 2. Update all manager config.toml files
    let managers = db.list_managers(Some(pool_name)).await?;
    let gitlab_url = format!(
        "http://{}:{}",
        config::GITLAB_HOSTNAME,
        config::GITLAB_HTTP_PORT
    );
    for m in &managers {
        let config_content = config::render_runner_config(
            pool_name,
            &m.id,
            &gitlab_url,
            &new_token,
            &pool.executor,
            &config::pool_cache_root(pool_name).display().to_string(),
            pool.concurrent,
            pool.request_concurrency,
        );
        let config_path = format!("{}/config.toml", m.config_dir);
        fs::write(&config_path, &config_content)
            .with_context(|| format!("rewriting config for manager {}", m.id))?;

        // 3. Hot-reload if running
        if m.state == "online" || m.state == "starting" {
            docker
                .reload_runner_config(&m.docker_container_id)
                .await
                .ok();
        }
    }

    // 4. Update database
    db.update_pool_token(pool_name, &new_token).await?;
    let expected = db.count_active_managers(pool_name).await?;
    if expected > 0 {
        wait_for_active_managers(db, pool_name, expected, Duration::from_secs(90)).await?;
    }

    info!(
        pool = pool_name,
        "token rotation complete — all managers updated"
    );
    Ok(new_token)
}

async fn wait_for_active_managers(
    db: &Db,
    pool_name: &str,
    expected: i64,
    timeout: Duration,
) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        let active = db.count_active_managers(pool_name).await?;
        if active == expected {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for pool '{}' active managers to reach {} (current={})",
                pool_name,
                expected,
                active
            );
        }
        sleep(Duration::from_secs(2)).await;
    }
}
