//! Background engine tasks extracted from engine.rs to keep the main engine
//! module focused on webhook handling and startup wiring.

use anyhow::Result;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use std::collections::BTreeSet;
use tracing::{debug, error, info, warn};

use super::{EngineState, SharedState};
use crate::pool;
use crate::release;
use crate::state::Pool as RunnerPool;

pub(crate) async fn check_scale_up(state: &EngineState) -> Result<()> {
    let pools = state.db.list_pools().await?;
    let queued = state.db.count_queued_jobs().await?;
    let running = state.db.count_running_jobs().await?;

    for p in &pools {
        if p.paused {
            continue;
        }

        let active = state.db.count_active_managers(&p.name).await?;
        let target = desired_manager_target(p, queued, running);

        if active < target {
            info!(
                pool = %p.name,
                active,
                target,
                queued,
                running,
                min_warm = p.min_warm,
                "scaling up to meet queue demand"
            );
            pool::scale_pool_to(
                &state.db,
                &state.docker,
                &state.client,
                &p.name,
                target as usize,
            )
            .await?;
        }
    }

    Ok(())
}

pub(crate) async fn system_health_loop(state: SharedState) {
    use std::sync::atomic::{AtomicBool, Ordering};

    static GC_RUNNING: AtomicBool = AtomicBool::new(false);

    // RAII guard to guarantee the lock is released even if an async task panics
    struct GcGuard;
    impl Drop for GcGuard {
        fn drop(&mut self) {
            GC_RUNNING.store(false, Ordering::SeqCst);
        }
    }

    let mut auto_paused_pools: BTreeSet<String> = BTreeSet::new();
    let mut consecutive_zero_freed = 0u32;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300)); // every 5 mins

    // Pre-flight disk check: if pressure is already critical, skip the settle delay
    if let Ok(fs) = crate::cache::df_usage("/").await {
        if fs.available_bytes >= crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES {
            // Safe to let engine settle
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        } else {
            warn!(
                root_free = %crate::cache::human_bytes(fs.available_bytes),
                required_free = %crate::cache::human_bytes(crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES),
                "startup pre-flight check detected disk pressure, bypassing settle delay"
            );
        }
    }

    loop {
        interval.tick().await;

        // Unconditional every-tick cleanup: kill orphaned Python forkserver workers
        // (processes reparented to init after their parent was SIGKILL'd).
        // Not gated on disk pressure — these consume RAM, not disk.
        let workers_killed = crate::reclaim::gc_orphaned_workers().await;
        if workers_killed > 0 {
            warn!("gc_orphaned_workers: killed {workers_killed} orphaned forkserver processes");
        }

        // Memory pressure check — escalates to active GC, not just logging
        let mem_gb = crate::reclaim::mem_available_gb();
        if mem_gb < 8.0 {
            error!("CRITICAL memory: {mem_gb:.1}GB available — forcing emergency GC");
            let _ = crate::reclaim::run_auto_gc(&state.docker, true, true).await;
        } else if mem_gb < 15.0 {
            warn!("memory pressure: {mem_gb:.1}GB available — triggering GC");
            let _ = crate::reclaim::run_auto_gc(&state.docker, false, false).await;
        }

        match crate::cache::df_usage("/").await {
            Ok(fs) => {
                let pressure = crate::cache::root_disk_pressure_level(fs.available_bytes);
                let root_free = fs.available_bytes;
                let root_used = fs.used_percent;

                if pressure == crate::cache::DiskPressureLevel::Nominal {
                    debug!(
                        root_free = %crate::cache::human_bytes(root_free),
                        root_used = root_used,
                        "disk pressure nominal"
                    );
                    consecutive_zero_freed = 0;

                    if !auto_paused_pools.is_empty() {
                        let paused: Vec<String> = auto_paused_pools.iter().cloned().collect();
                        for pool_name in paused {
                            if let Err(e) =
                                pool::resume_pool(&state.db, &state.client, &pool_name).await
                            {
                                error!(
                                    error = %e,
                                    pool = %pool_name,
                                    "failed to resume pool after disk pressure relief"
                                );
                            } else {
                                info!(
                                    pool = %pool_name,
                                    "resumed pool after disk pressure relief"
                                );
                                auto_paused_pools.remove(&pool_name);
                            }
                        }
                    }

                    let manager = crate::cache::CacheManager;
                    if let Err(e) = manager.gc_disk_cache().await {
                        error!(error = %e, "background GC failed");
                    }
                    // Proactively prune expired builder cache every cycle to prevent overlay2 accumulation.
                    let _ = tokio::process::Command::new("docker")
                        .args(["builder", "prune", "--force", "--filter", "until=2h"])
                        .output()
                        .await;
                    continue;
                }

                let is_critical = matches!(
                    pressure,
                    crate::cache::DiskPressureLevel::Critical
                        | crate::cache::DiskPressureLevel::Emergency
                );
                let is_emergency = pressure == crate::cache::DiskPressureLevel::Emergency;
                let is_warning = true;

                if GC_RUNNING.swap(true, Ordering::SeqCst) {
                    warn!("GC already in progress, skipping this cycle");
                    continue;
                }

                let _guard = GcGuard; // Will automatically reset GC_RUNNING to false on drop/panic

                if is_emergency {
                    warn!(
                        root_free = %crate::cache::human_bytes(root_free),
                        required_free = %crate::cache::human_bytes(
                            crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES
                        ),
                        "disk pressure emergency: pausing build/default pools and draining managers"
                    );

                    let pressure_pools = ["build", "default"];
                    for pool_name in pressure_pools {
                        if auto_paused_pools.contains(pool_name) {
                            continue;
                        }
                        if let Err(e) =
                            pool::drain_pool(&state.db, &state.docker, &state.client, pool_name)
                                .await
                        {
                            error!(
                                error = %e,
                                pool = pool_name,
                                "failed to drain pool during disk pressure emergency"
                            );
                        } else {
                            auto_paused_pools.insert(pool_name.to_string());
                            info!(pool = pool_name, "drained pool for emergency disk relief");
                        }
                    }

                    let _ = state
                        .db
                        .append_event(
                            "disk_pressure_emergency",
                            None,
                            None,
                            "system_health_loop",
                            &serde_json::json!({
                                "root_free_bytes": root_free,
                                "root_free_human": crate::cache::human_bytes(root_free),
                                "warning_floor_bytes": crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
                                "emergency_floor_bytes": crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
                                "paused_pools": ["build", "default"],
                            })
                            .to_string(),
                        )
                        .await;
                } else {
                    warn!(
                        root_free = %crate::cache::human_bytes(root_free),
                        warning_floor = %crate::cache::human_bytes(
                            crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES
                        ),
                        "disk pressure warning: running cache GC"
                    );
                }

                let manager = crate::cache::CacheManager;
                if let Err(e) = manager
                    .gc_disk_cache_with_pressure(is_warning, is_critical, is_emergency)
                    .await
                {
                    error!(error = %e, "cache GC failed");
                }

                let mut current_free = root_free;
                let target_free_bytes = crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES;
                let mut pass = 0u32;
                let mut last_freed_bytes = u64::MAX;
                let usage_before = root_free;

                while current_free < target_free_bytes && pass < 20 {
                    pass += 1;
                    let escalated = pass > 1 || is_critical;
                    let free_before_pass = current_free;

                    warn!(
                        root_free = %crate::cache::human_bytes(current_free),
                        pass,
                        critical = escalated,
                        emergency = is_emergency,
                        "disk pressure: running GC pass"
                    );

                    let _ = state
                        .db
                        .append_event(
                            "disk_pressure_gc",
                            None,
                            None,
                            "system_health_loop",
                            &serde_json::json!({
                                "root_free_bytes": current_free,
                                "root_free_human": crate::cache::human_bytes(current_free),
                                "pass": pass,
                                "critical": escalated,
                                "emergency": is_emergency,
                                "warning_floor_bytes": crate::cache::ROOT_DISK_WARNING_MIN_FREE_BYTES,
                                "emergency_floor_bytes": crate::cache::ROOT_DISK_EMERGENCY_MIN_FREE_BYTES,
                            })
                            .to_string(),
                        )
                        .await;

                    if let Err(e) =
                        crate::reclaim::run_auto_gc(&state.docker, escalated, is_emergency).await
                    {
                        error!(error = %e, "auto_gc failed");
                        break;
                    }

                    let manager = crate::cache::CacheManager;
                    if let Err(e) = manager
                        .gc_disk_cache_with_pressure(is_warning, is_critical, is_emergency)
                        .await
                    {
                        error!(error = %e, "cache GC failed");
                    }

                    match crate::cache::df_usage("/").await {
                        Ok(fs) => current_free = fs.available_bytes,
                        Err(e) => {
                            warn!(error = %e, "failed to refresh disk usage after GC pass");
                            break;
                        }
                    }

                    let pass_freed = current_free.saturating_sub(free_before_pass);
                    if pass > 2
                        && pass_freed < 512 * 1024 * 1024
                        && last_freed_bytes < 512 * 1024 * 1024
                    {
                        warn!(
                            pass,
                            root_free = %crate::cache::human_bytes(current_free),
                            "GC stalled — two consecutive passes freed < 512MiB, stopping early"
                        );
                        break;
                    }
                    last_freed_bytes = pass_freed;

                    let pass_sleep = if is_emergency {
                        10
                    } else if is_critical {
                        20
                    } else {
                        30
                    };
                    tokio::time::sleep(std::time::Duration::from_secs(pass_sleep)).await;
                }

                let freed_bytes = current_free.saturating_sub(usage_before);
                let _ = state
                    .db
                    .append_event(
                        "disk_pressure_gc_complete",
                        None,
                        None,
                        "system_health_loop",
                        &serde_json::json!({
                            "root_free_before_bytes": usage_before,
                            "root_free_after_bytes": current_free,
                            "freed_bytes": freed_bytes,
                            "passes": pass,
                        })
                        .to_string(),
                    )
                    .await;

                if freed_bytes == 0 {
                    consecutive_zero_freed += 1;
                    if consecutive_zero_freed >= 3 {
                        error!(
                            consecutive_stalls = consecutive_zero_freed,
                            root_free = %crate::cache::human_bytes(current_free),
                            "disk GC stalled: 3 consecutive cycles freed near-zero space — manual intervention needed"
                        );
                        let _ = state
                            .db
                            .append_event(
                                "disk_gc_stalled",
                                None,
                                None,
                                "system_health_loop",
                                &serde_json::json!({
                                    "root_free_bytes": current_free,
                                    "consecutive_stalls": consecutive_zero_freed,
                                })
                                .to_string(),
                            )
                            .await;
                    }
                } else {
                    consecutive_zero_freed = 0;
                    info!(
                        freed_bytes,
                        root_free_after = %crate::cache::human_bytes(current_free),
                        "disk pressure relieved"
                    );
                }

                tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                continue;
            }
            Err(e) => {
                warn!(error = %e, "failed to check disk usage");
                continue;
            }
        };
    }
}

pub(crate) async fn reconciliation_loop(state: SharedState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));

    loop {
        interval.tick().await;
        debug!("running reconciliation");

        // Check each pool's desired vs actual manager count
        if let Err(e) = reconcile(&state).await {
            error!(error = %e, "reconciliation failed");
        }
    }
}

async fn reconcile(state: &EngineState) -> Result<()> {
    let pools = state.db.list_pools().await?;
    let queued = state.db.count_queued_jobs().await?;
    let running = state.db.count_running_jobs().await?;

    for p in &pools {
        if p.paused {
            continue;
        }

        let _stale_managers =
            pool::reconcile_manager_runtime_state(&state.db, &state.docker, Some(&p.name)).await?;
        let active = state.db.count_active_managers(&p.name).await?;

        let target = desired_manager_target(p, queued, running);

        // Scale to our exact target without ignoring running jobs.
        if active != target {
            info!(
                pool = %p.name,
                active,
                target,
                queued,
                running,
                "reconciler: scaling pool"
            );
            pool::scale_pool_to(
                &state.db,
                &state.docker,
                &state.client,
                &p.name,
                target as usize,
            )
            .await?;
        }

        // Sync system_ids from disk for managers that don't have one yet
        let managers = state.db.list_managers(Some(&p.name)).await?;
        for m in &managers {
            if m.system_id.is_none() && (m.state == "starting" || m.state == "online") {
                let system_id_path = format!("{}/.runner_system_id", m.config_dir);
                if let Ok(sid) = std::fs::read_to_string(&system_id_path) {
                    let sid = sid.trim().to_string();
                    if !sid.is_empty() {
                        info!(manager_id = %m.id, system_id = %sid, "discovered system_id");
                        state.db.update_manager_system_id(&m.id, &sid).await?;
                        // Also mark as online since it's clearly registered
                        state.db.update_manager_state(&m.id, "online").await?;
                    }
                }
            }
        }
    }

    if let Err(err) = release::reconcile_release_for_ref(
        &state.db,
        &state.client,
        release::DEFAULT_RELEASE_PROJECT_ID,
        "main",
    )
    .await
    {
        warn!(error = %err, "release reconciliation failed");
    }

    Ok(())
}

fn desired_manager_target(pool: &RunnerPool, queued: i64, running: i64) -> i64 {
    let queue_target = pool.min_warm.saturating_add(queued).max(pool.min_warm);
    let active_work_target = queue_target.max(running);
    active_work_target.clamp(pool.min_warm, pool.max_managers)
}

pub(crate) async fn cache_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    let token = headers
        .get("X-Jeryu-Token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token != state.webhook_secret {
        warn!("cache_summary rejected: missing or invalid X-Jeryu-Token");
        return Err(StatusCode::UNAUTHORIZED);
    }
    let metrics = match state.db.get_cache_metrics().await {
        Ok(m) => m,
        Err(_) => Default::default(),
    };
    Ok(axum::Json(serde_json::json!({
        "bytes_served": metrics.bytes_served,
        "hits": metrics.hit_count,
        "objects": metrics.object_count,
        "status": "healthy"
    })))
}

pub(crate) async fn docker_event_loop(state: SharedState) {
    use futures_util::StreamExt;

    debug!("starting docker event listener");
    let mut stream = state.docker.events();

    while let Some(msg) = stream.next().await {
        if let Ok(event) = msg
            && let Some(typ) = event.typ
            && typ == bollard::models::EventMessageTypeEnum::CONTAINER
            && let Some(action) = event.action
        {
            // Check if it's a manager container dying or OOM
            if (action == "die" || action == "oom")
                && let Some(actor) = event.actor
                && let Some(attrs) = actor.attributes
                && attrs.get("jeryu.managed").map(|s| s.as_str()) == Some("true")
            {
                let name = match attrs.get("name").cloned() {
                    Some(n) => n,
                    None => String::new(),
                };
                warn!(%name, action, "jeryu manager container terminated unexpectedly");
                if let Some(manager_id) = attrs.get("jeryu.manager_id")
                    && let Err(error) = state.db.update_manager_state(manager_id, "stopped").await
                {
                    error!(%manager_id, %error, "failed to mark dead runner manager stopped");
                }
                // Run a full reconciliation immediately to replace it
                if let Err(e) = reconcile(&state).await {
                    error!(error = %e, "reconciliation failed after container death");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(min_warm: i64, max_managers: i64) -> RunnerPool {
        RunnerPool {
            name: "build".into(),
            gitlab_runner_id: 1,
            auth_token: "token".into(),
            tags: "build".into(),
            executor: "docker".into(),
            min_warm,
            max_managers,
            concurrent: 8,
            request_concurrency: 4,
            paused: false,
            trust_tier: "trusted".into(),
        }
    }

    #[test]
    fn desired_manager_target_accounts_for_running_jobs() {
        let pool = pool(1, 3);

        assert_eq!(desired_manager_target(&pool, 0, 0), 1);
        assert_eq!(desired_manager_target(&pool, 1, 0), 2);
        assert_eq!(desired_manager_target(&pool, 0, 3), 3);
        assert_eq!(desired_manager_target(&pool, 4, 4), 3);
    }
}
