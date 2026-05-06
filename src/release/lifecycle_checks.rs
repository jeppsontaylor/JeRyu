use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseLock {
    pub schema: u32,
    pub release_version: String,
    pub product_sha: String,
    pub certifying_pipeline_id: i64,
    pub upstream_pipeline_id: i64,
    pub build_job_id: Option<i64>,
    pub image_ref: Option<String>,
    pub release_tool_sha: String,
    pub created_at: String,
}

pub(crate) fn release_lock_path(version: &str) -> PathBuf {
    release_dir(version).join("release-lock.json")
}

pub(crate) fn write_release_lock(version: &str, lock: &ReleaseLock) {
    let path = release_lock_path(version);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(lock) {
        Ok(json) => {
            if let Err(e) = fs::write(&path, json) {
                warn!(version, error = %e, "failed to write release-lock.json");
            } else {
                info!(version, path = %path.display(), "wrote release-lock.json");
            }
        }
        Err(e) => warn!(version, error = %e, "failed to serialize release-lock.json"),
    }
}

// ── Release preflight ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightBlocker {
    pub code: String,
    pub component: String,
    pub detail: String,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightReport {
    pub ok: bool,
    pub blockers: Vec<PreflightBlocker>,
    pub checks: std::collections::HashMap<String, String>,
    pub generated_at: String,
}

pub async fn release_preflight(ssh_host: Option<&str>) -> PreflightReport {
    let mut blockers = Vec::new();
    let mut checks = std::collections::HashMap::new();
    let target = ssh_host.unwrap_or("atomicsoul");

    // SSH check
    let ssh_ok = tokio::process::Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=5",
            "-o",
            "StrictHostKeyChecking=no",
            target,
            "echo",
            "ci-preflight-ok",
        ])
        .output()
        .await
        .map(|o| {
            o.status.success() && String::from_utf8_lossy(&o.stdout).contains("ci-preflight-ok")
        })
        .unwrap_or(false);
    checks.insert(
        "ssh".to_string(),
        if ssh_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !ssh_ok {
        blockers.push(PreflightBlocker {
            code: "SSH_UNREACHABLE".to_string(),
            component: "canary-target".to_string(),
            detail: format!("SSH to {target} failed (ConnectTimeout=5)"),
            recommended_action: format!(
                "verify {target} is powered on and reachable from this host"
            ),
        });
    }

    // Vault check
    let vault_port = crate::config::VAULT_HTTP_PORT;
    let vault_url = format!("http://127.0.0.1:{vault_port}/v1/sys/health");
    let vault_ok = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(client) => client
            .get(&vault_url)
            .send()
            .await
            .map(|r| r.status().as_u16() < 500)
            .unwrap_or(false),
        Err(_) => false,
    };
    checks.insert(
        "vault".to_string(),
        if vault_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !vault_ok {
        blockers.push(PreflightBlocker {
            code: "VAULT_UNREACHABLE".to_string(),
            component: "vault".to_string(),
            detail: format!("Vault health check failed at {vault_url}"),
            recommended_action: "run: jeryu cache doctor; check vault container is running"
                .to_string(),
        });
    }

    // Registry check (TCP connect to local registry mirror)
    let registry_port = crate::settings::get().cache.registry_port;
    let registry_ok = tokio::net::TcpStream::connect(format!("127.0.0.1:{registry_port}"))
        .await
        .is_ok();
    checks.insert(
        "registry".to_string(),
        if registry_ok {
            "ok".to_string()
        } else {
            "failed".to_string()
        },
    );
    if !registry_ok {
        blockers.push(PreflightBlocker {
            code: "REGISTRY_UNREACHABLE".to_string(),
            component: "registry-mirror".to_string(),
            detail: format!("registry mirror TCP connect to 127.0.0.1:{registry_port} failed"),
            recommended_action: "run: jeryu serve (starts registry mirror)".to_string(),
        });
    }

    // Disk check
    const DISK_EMERGENCY_FREE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
    const DISK_CRITICAL_FREE_BYTES: u64 = 50 * 1024 * 1024 * 1024;
    const DISK_WARNING_FREE_BYTES: u64 = 75 * 1024 * 1024 * 1024;
    let disk_status = match crate::cache::df_usage("/").await {
        Ok(usage) => {
            if usage.available_bytes < DISK_EMERGENCY_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "emergency ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                blockers.push(PreflightBlocker {
                    code: "DISK_EMERGENCY".to_string(),
                    component: "host-disk".to_string(),
                    detail: format!(
                        "root disk only has {} free",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                    recommended_action: "run: jeryu cache status --json; then jeryu cache gc --json --keep-active-managers=false --max-cache-gb 20".to_string(),
                });
                false
            } else if usage.available_bytes < DISK_CRITICAL_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "critical ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                blockers.push(PreflightBlocker {
                    code: "DISK_CRITICAL".to_string(),
                    component: "host-disk".to_string(),
                    detail: format!(
                        "root disk only has {} free",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                    recommended_action: "run: jeryu cache status --json; then jeryu cache gc --dry-run --json --older-than 12h --max-cache-gb 20".to_string(),
                });
                false
            } else if usage.available_bytes < DISK_WARNING_FREE_BYTES {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "warning ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                true
            } else {
                checks.insert(
                    "disk".to_string(),
                    format!(
                        "ok ({} free on /)",
                        crate::cache::human_bytes(usage.available_bytes)
                    ),
                );
                true
            }
        }
        Err(_) => {
            checks.insert("disk".to_string(), "unknown".to_string());
            true
        }
    };
    let _ = disk_status; // disk warning doesn't block

    PreflightReport {
        ok: blockers.is_empty(),
        blockers,
        checks,
        generated_at: chrono::Utc::now().to_rfc3339(),
    }
}

// ── Release doctor ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorBlocker {
    pub code: String,
    pub gate: Option<String>,
    pub detail: String,
    pub recommended_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub version: String,
    pub product_sha: String,
    pub next_action: String,
    pub blockers: Vec<DoctorBlocker>,
    pub preflight: std::collections::HashMap<String, String>,
    pub gates: std::collections::HashMap<String, bool>,
    pub canary_complete: bool,
    pub prod_complete: bool,
    pub safe_to_reconcile: bool,
    pub generated_at: String,
}

pub async fn release_doctor(version: &str, run_preflight: bool) -> DoctorReport {
    let mut blockers = Vec::new();
    let mut gates = std::collections::HashMap::new();

    // Check gate files
    let gate_files = canary_gate_files(version);
    let gate_prod = gate_prod_promotion_path(version).exists();
    gates.insert("remote".to_string(), gate_files.remote);
    gates.insert("telemetry".to_string(), gate_files.telemetry);
    gates.insert("e2e".to_string(), gate_files.e2e);
    gates.insert("prod".to_string(), gate_prod);
    gates.insert("c_validation".to_string(), gate_files.validation);

    let canary_complete = gate_files.canary_complete();
    let prod_complete = gate_prod;

    // Check missing gates
    for (name, present, path) in [
        (
            "gate-remote-canary",
            gate_files.remote,
            gate_remote_canary_path(version),
        ),
        (
            "gate-canary-telemetry",
            gate_files.telemetry,
            gate_canary_telemetry_path(version),
        ),
        (
            "gate-canary-e2e",
            gate_files.e2e,
            gate_canary_e2e_path(version),
        ),
        (
            "c-validation",
            gate_files.validation,
            c_validation_path(version),
        ),
    ] {
        if !present {
            blockers.push(DoctorBlocker {
                code: "GATE_MISSING".to_string(),
                gate: Some(name.to_string()),
                detail: format!("{} not found at {}", name, path.display()),
                recommended_action: "run: jeryu release reconcile (triggers new canary attempt)"
                    .to_string(),
            });
        }
    }

    // Check release-lock
    let lock_path = release_lock_path(version);
    if !lock_path.exists() {
        blockers.push(DoctorBlocker {
            code: "LOCK_MISSING".to_string(),
            gate: None,
            detail: format!("release-lock.json not found at {}", lock_path.display()),
            recommended_action:
                "run: jeryu release reconcile (generates lock on next canary trigger)".to_string(),
        });
    }

    // Run preflight checks
    let preflight_checks = if run_preflight {
        let pf = release_preflight(None).await;
        for b in &pf.blockers {
            blockers.push(DoctorBlocker {
                code: b.code.clone(),
                gate: None,
                detail: b.detail.clone(),
                recommended_action: b.recommended_action.clone(),
            });
        }
        pf.checks
    } else {
        let mut m = std::collections::HashMap::new();
        m.insert("ssh".to_string(), "not-checked".to_string());
        m.insert("vault".to_string(), "not-checked".to_string());
        m.insert("registry".to_string(), "not-checked".to_string());
        m.insert("disk".to_string(), "not-checked".to_string());
        m
    };

    // Determine next action
    let next_action = if prod_complete {
        "done"
    } else if canary_complete {
        "run_production_promotion"
    } else if !blockers.iter().any(|b| {
        matches!(
            b.code.as_str(),
            "SSH_UNREACHABLE" | "VAULT_UNREACHABLE" | "REGISTRY_UNREACHABLE" | "DISK_EMERGENCY"
        )
    }) {
        "run_canary_requeue"
    } else {
        "fix_preflight_blockers"
    };

    // Read product_sha from lock if available
    let product_sha = fs::read_to_string(lock_path)
        .ok()
        .and_then(|s| serde_json::from_str::<ReleaseLock>(&s).ok())
        .map(|l| l.product_sha)
        .unwrap_or("unknown".to_string());

    DoctorReport {
        version: version.to_string(),
        product_sha,
        next_action: next_action.to_string(),
        blockers,
        preflight: preflight_checks,
        gates,
        canary_complete,
        prod_complete,
        safe_to_reconcile: next_action != "fix_preflight_blockers",
        generated_at: chrono::Utc::now().to_rfc3339(),
    }
}
