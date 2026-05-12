use anyhow::Result;
use std::env;
use tracing::info;

use super::support::{
    ensure_custom_executor_tools, env_bool_or_default, env_i64_or_default, env_string_or_default,
};

/// Handles `jeryu exec prepare`
/// Provisions the actual job container sandbox.
pub async fn run_prepare() -> Result<()> {
    let job_id = env_string_or_default("CUSTOM_ENV_CI_JOB_ID", "unknown");
    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");

    info!(
        job_id,
        project_dir, "Driver: preparing custom execution sandbox"
    );

    let sandbox_path = format!("{}-sandbox", project_dir);

    if super::support::fast_clone(&project_dir, &sandbox_path).is_err() {
        let _ = std::fs::create_dir_all(&sandbox_path);
    }

    crate::honeypot::seed_sandbox(&sandbox_path);

    Ok(())
}

/// Handles `jeryu exec run`
/// Executes a specific stage of the pipeline (step_script, build_script, etc.)
pub async fn run_stage(script_path: &str, stage: &str) -> Result<()> {
    let job_id = env_i64_or_default("CUSTOM_ENV_CI_JOB_ID", 0);
    let project_id_str = env_string_or_default("CUSTOM_ENV_CI_PROJECT_ID", "");
    let project_id = project_id_str.parse::<i64>().ok();

    super::validate_script_path(script_path)?;

    info!(job_id, stage, script_path, "Driver: running job stage");

    if stage == "build_script" {
        ensure_custom_executor_tools().await?;
    }

    let project_dir = env_string_or_default("CUSTOM_ENV_CI_PROJECT_DIR", "/tmp/jeryu-job");
    let sandbox_path = format!("{}-sandbox", project_dir);

    let db = crate::state::Db::open().await?;
    let epoch_manager = crate::epoch::EpochManager::with_backend(db.pool(), db.backend());
    let taint_manager = crate::taint::TaintManager::with_backend(db.pool(), db.backend());
    let store = cache_brain_adapter::create_action_store(
        db.pool(),
        match db.backend() {
            crate::state::StateBackend::Sqlite => cache_brain_adapter::AdapterBackend::Sqlite,
            crate::state::StateBackend::Postgres => cache_brain_adapter::AdapterBackend::Postgres,
        },
    );
    let cache_brain =
        crate::cache_brain::CacheBrain::with_store(epoch_manager, taint_manager, store);

    let mut build_unit: Option<crate::cache_brain::BuildUnit> = None;

    if stage == "build_script"
        && env::var("CUSTOM_ENV_JERYU_FORCE_REFRESH").ok().as_deref() != Some("1")
    {
        let is_rust_build = script_path.contains("cargo build")
            && !script_path.contains("cargo test")
            && !script_path.contains("cargo check")
            && !script_path.contains("cargo clippy");
        let dockerfile = std::path::Path::new(&sandbox_path).join("Dockerfile");

        build_unit = if dockerfile.exists() {
            if let Ok(witness) = crate::witness::WitnessBuilder::docker_build_witness(
                project_id.unwrap_or(0),
                &dockerfile,
                std::path::Path::new(&sandbox_path),
            )
            .await
            {
                Some(crate::cache_brain::BuildUnit {
                    unit_type: crate::cache_brain::BuildUnitType::DockerBuild {
                        stage: "build".into(),
                    },
                    input_signature: witness.key,
                    environment_signature: env_string_or_default("DOCKER_DEFAULT_PLATFORM", ""),
                    scope: format!("project:{}", project_id.unwrap_or(0)),
                    trust_tier: crate::policy::TrustTier::Untrusted,
                })
            } else {
                None
            }
        } else if is_rust_build {
            let cargo_lock = std::path::Path::new(&sandbox_path).join("Cargo.lock");
            if let Ok(witness) = crate::witness::WitnessBuilder::rust_build_witness(
                project_id.unwrap_or(0),
                &cargo_lock,
                "1.74.0",
                Some("cargo-witness"),
                "x86_64-unknown-linux-gnu",
                "release",
                "",
            )
            .await
            {
                Some(crate::cache_brain::BuildUnit {
                    unit_type: crate::cache_brain::BuildUnitType::CargoBuild {
                        target: "x86_64-unknown-linux-gnu".into(),
                        profile: "release".into(),
                        features: "".into(),
                    },
                    input_signature: witness.key,
                    environment_signature: env_string_or_default("RUSTFLAGS", ""),
                    scope: format!("project:{}", project_id.unwrap_or(0)),
                    trust_tier: crate::policy::TrustTier::Untrusted,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(ref unit) = build_unit {
            let verdict = cache_brain.plan_step(unit).await?;
            tracing::info!("CacheBrain Verdict: {:?}", verdict);

            let verdict_str = format!("{:?}", verdict);
            let reasons_str = match serde_json::to_string(&verdict) {
                Ok(s) => s,
                Err(_) => String::new(),
            };
            let _ = db
                .store_test_verdict(
                    job_id,
                    &unit.input_signature,
                    &unit.input_signature,
                    &unit.input_signature,
                    &verdict_str,
                    &format!("{:?}", unit.trust_tier),
                    &reasons_str,
                )
                .await;

            if verdict.is_hit() {
                let cas_path = crate::config::data_dir()
                    .join("cas")
                    .join(&unit.input_signature);
                let manifest_path = cas_path.join("manifest.json");
                let payload_path = cas_path.join("payload.tar.zst");
                let manifest_exists = tokio::fs::try_exists(&manifest_path).await.unwrap_or(false);
                let payload_exists = tokio::fs::try_exists(&payload_path).await.unwrap_or(false);

                if manifest_exists && payload_exists {
                    let extract_status = tokio::process::Command::new("tar")
                        .arg("-I")
                        .arg("zstd")
                        .arg("-xf")
                        .arg(&payload_path)
                        .arg("-C")
                        .arg(&sandbox_path)
                        .status()
                        .await;

                    match extract_status {
                        Ok(s) if s.success() => {
                            tracing::info!(
                                "✅ Exact-Hit: extracted CAS payload {:?} into {}. Skipping execution.",
                                payload_path,
                                sandbox_path
                            );
                            std::process::exit(0);
                        }
                        Ok(s) => {
                            tracing::warn!(
                                "CAS extraction failed with exit code {:?}; falling back to cold execution.",
                                s.code()
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                "CAS extraction error: {:?}; falling back to cold execution.",
                                e
                            );
                        }
                    }
                } else {
                    tracing::warn!(
                        "Exact-hit verdict but CAS payload incomplete (manifest={}, payload={}); falling back to cold execution.",
                        manifest_exists,
                        payload_exists
                    );
                }
            } else {
                tracing::info!(
                    "Cache Brain produced Miss/Deny verdict, falling to cold execution."
                );
            }
        }
    }

    let buildkit_mgr = crate::buildkit::BuildKitManager::new("untrusted");
    let mut extra_envs = buildkit_mgr.inject_env();
    let pool_cache_root = env_string_or_default("JERYU_CARGO_CACHE_ROOT", "/pool-cache");
    let pip_cache_dir = std::path::Path::new(&pool_cache_root).join("pip-cache");
    let _ = std::fs::create_dir_all(&pip_cache_dir);

    extra_envs.push(("PIP_BREAK_SYSTEM_PACKAGES".to_string(), "1".to_string()));
    extra_envs.push(("PIP_ROOT_USER_ACTION".to_string(), "ignore".to_string()));
    extra_envs.push((
        "PIP_CACHE_DIR".to_string(),
        pip_cache_dir.display().to_string(),
    ));

    let cargo_available = std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .is_ok();
    let rustc_available = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .is_ok();

    if cargo_available && rustc_available {
        let cargo_cache_enabled = env_bool_or_default("JERYU_CARGO_CACHE", true);
        let project_scope = match env::var("CUSTOM_ENV_CI_PROJECT_PATH_SLUG") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => match env::var("CUSTOM_ENV_CI_PROJECT_DIR") {
                Ok(project_dir) => {
                    match crate::cargo_cache::canonical_repo_key(std::path::Path::new(&project_dir))
                    {
                        Ok(value) if !value.trim().is_empty() => value,
                        _ => "unknown-project".to_string(),
                    }
                }
                Err(_) => "unknown-project".to_string(),
            },
        };
        let isolate_job =
            if std::env::var("JERYU_CARGO_TARGET_ISOLATE").ok().as_deref() == Some("job") {
                std::env::var("CUSTOM_ENV_CI_JOB_ID").ok()
            } else {
                None
            };
        let incremental_override = std::env::var("JERYU_CARGO_INCREMENTAL").ok();
        let cargo_layout = crate::cargo_cache::runner_cargo_layout(
            std::path::Path::new(&pool_cache_root),
            &project_scope,
            cargo_cache_enabled,
            isolate_job.as_deref(),
            incremental_override.as_deref(),
        )?;
        if let Some(target_dir) = cargo_layout.env.get("CARGO_TARGET_DIR") {
            let _ = std::fs::create_dir_all(target_dir);
        }
        if let Some(sccache_dir) = cargo_layout.env.get("SCCACHE_DIR") {
            let _ = std::fs::create_dir_all(sccache_dir);
        }
        extra_envs.extend(cargo_layout.env.into_iter());
    }

    let cargo_dir = std::path::Path::new(&sandbox_path).join(".cargo");
    let _ = std::fs::create_dir_all(&cargo_dir);
    let cargo_toml = r#"
[source.crates-io]
replace-with = "jeryu-proxy"

[source.jeryu-proxy]
registry = "sparse+http://127.0.0.1:19800/api/v1/crates"
"#
    .to_string();
    let _ = std::fs::write(cargo_dir.join("config.toml"), cargo_toml);

    tracing::info!("Injecting sandbox environment variables ({:?})", extra_envs);

    let sandbox = crate::sandbox::ExecutorSandbox::new(crate::sandbox::SandboxConfig {
        use_strict_network_isolation: crate::settings::get().sandbox.strict_network_isolation,
        proxy_host: String::new(),
        proxy_port: 0,
        bind_workspace: sandbox_path.clone(),
        extra_envs,
    });

    let mut child = sandbox.spawn_script(script_path)?;

    let _tripwire = if let Some(pid) = child.id() {
        crate::honeypot::start_tripwire(
            pid,
            crate::honeypot::get_tokens(&sandbox_path),
            sandbox_path.clone(),
        )
        .ok()
    } else {
        None
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let log_buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::with_capacity(4096)));
    let log_buffer_cloned = log_buffer.clone();

    let stdout_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();
        while let Ok(n) = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
            if n == 0 {
                break;
            }
            print!("{}", line);
            let mut buf = log_buffer.lock().unwrap();
            if buf.len() > 3000 {
                buf.drain(0..1000);
            }
            buf.extend_from_slice(line.as_bytes());
            line.clear();
        }
    });

    let log_buffer_cloned_stderr = log_buffer_cloned.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr);
        let mut line = String::new();
        while let Ok(n) = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await {
            if n == 0 {
                break;
            }
            eprint!("{}", line);
            let mut buf = log_buffer_cloned_stderr.lock().unwrap();
            if buf.len() > 3000 {
                buf.drain(0..1000);
            }
            buf.extend_from_slice(line.as_bytes());
            line.clear();
        }
    });

    let status = child.wait().await?;
    let _ = tokio::join!(stdout_task, stderr_task);

    let exit_code = status.code().unwrap_or(1);
    let quarantine_marker = std::path::Path::new(&sandbox_path).join(".jeryu_quarantine");
    let is_quarantined = quarantine_marker.exists();

    if is_quarantined {
        let reason = match std::fs::read_to_string(&quarantine_marker) {
            Ok(text) => text,
            Err(_) => String::new(),
        };
        let log_snippet = String::from_utf8_lossy(&log_buffer_cloned.lock().unwrap()).to_string();
        let capsule = crate::capsule::FailureCapsule::capture(
            job_id,
            project_id.unwrap_or(0),
            stage,
            999,
            format!("🚨 QUARANTINED: {}\n\nLogs:\n{}", reason, log_snippet),
            &format!("bash {}", script_path),
        );
        db.insert_evidence_capsule("quarantine_capsule", &capsule)
            .await?;
        db.append_event(
            "quarantine_capsule",
            project_id,
            Some(job_id),
            "jeryu-exec",
            &capsule.to_json(),
        )
        .await?;
        std::process::exit(1);
    }

    if !status.success() {
        let log_snippet = String::from_utf8_lossy(&log_buffer_cloned.lock().unwrap()).to_string();
        let capsule = crate::capsule::FailureCapsule::capture(
            job_id,
            project_id.unwrap_or(0),
            stage,
            exit_code,
            log_snippet,
            &format!("bash {}", script_path),
        );

        db.insert_evidence_capsule("failure_capsule", &capsule)
            .await?;
        db.append_event(
            "failure_capsule",
            project_id,
            Some(job_id),
            "jeryu-exec",
            &capsule.to_json(),
        )
        .await?;

        std::process::exit(exit_code);
    }

    let payload = serde_json::json!({
        "stage": stage,
        "script_path": script_path,
        "exit_code": exit_code,
    });

    db.append_event(
        "stage_execution",
        project_id,
        Some(job_id),
        "jeryu-exec",
        &payload.to_string(),
    )
    .await?;

    if stage == "build_script"
        && let Some(ref unit) = build_unit
    {
        let namespace = match unit.trust_tier {
            crate::policy::TrustTier::Trusted => "trusted",
            crate::policy::TrustTier::Untrusted => "untrusted",
            crate::policy::TrustTier::Quarantine => "quarantine",
        };
        let manifest = serde_json::json!({
            "unit_type": unit.unit_type,
            "environment_signature": unit.environment_signature,
            "scope": unit.scope,
            "job_id": job_id,
            "project_id": project_id,
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        let _ = db
            .upsert_action_cache(&unit.input_signature, &manifest.to_string(), namespace)
            .await;
        tracing::info!(
            "Populated action_cache for signature {} in namespace {}",
            unit.input_signature,
            namespace
        );

        let cas_dir = crate::config::data_dir()
            .join("cas")
            .join(&unit.input_signature);
        if let Ok(()) = tokio::fs::create_dir_all(&cas_dir).await {
            let payload_path = cas_dir.join("payload.tar.zst");
            let manifest_path = cas_dir.join("manifest.json");
            let archive_status = tokio::process::Command::new("tar")
                .arg("-I")
                .arg("zstd")
                .arg("-cf")
                .arg(&payload_path)
                .arg("-C")
                .arg(&sandbox_path)
                .arg(".")
                .status()
                .await;
            match archive_status {
                Ok(s) if s.success() => {
                    let _ = tokio::fs::write(&manifest_path, manifest.to_string()).await;
                    tracing::info!("Archived build output to CAS: {:?}", cas_dir);
                }
                _ => {
                    tracing::warn!(
                        "Failed to archive build output to CAS; future exact-hit will miss."
                    );
                }
            }
        }
    }

    Ok(())
}
