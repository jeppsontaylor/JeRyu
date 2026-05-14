use crate::cli::ReleaseCommands;
use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::{release, state};
use std::path::PathBuf;

pub(crate) async fn execute_release_commands(subcmd: ReleaseCommands) -> Result<()> {
    match subcmd {
        ReleaseCommands::Status {
            project_id,
            ref_name,
            sha,
            limit,
            json,
        } => {
            let db = state::Db::open().await?;
            let report = release::build_release_status_report(
                &db,
                release::ReleaseStatusQuery {
                    project_id: Some(project_id),
                    ref_name: Some(ref_name),
                    sha,
                    limit,
                },
            )
            .await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_release_status_text(&report));
            }
        }
        ReleaseCommands::Watch {
            project_id,
            ref_name,
            sha,
            limit,
            interval_secs,
            json,
        } => {
            let db = state::Db::open().await?;
            release::watch_release_status(
                &db,
                release::ReleaseStatusQuery {
                    project_id: Some(project_id),
                    ref_name: Some(ref_name),
                    sha,
                    limit,
                },
                json,
                interval_secs,
            )
            .await?;
        }
        ReleaseCommands::Reconcile {
            project_id,
            ref_name,
            json,
        } => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let report =
                release::reconcile_release_for_ref(&db, &client, project_id, &ref_name).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_release_status_text(&report));
            }
        }
        ReleaseCommands::PromoteProd {
            project_id,
            ref_name,
            version,
        } => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let pipeline_id =
                release::trigger_production_promotion(&db, &client, project_id, &ref_name, version)
                    .await?;
            println!("Triggered production-promotion pipeline {pipeline_id}");
        }
        ReleaseCommands::Preflight { ssh_host, json } => {
            let report = release::release_preflight(ssh_host.as_deref()).await;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let status = if report.ok { "PASS" } else { "FAIL" };
                println!("Preflight: {status}");
                for (k, v) in &report.checks {
                    println!("  {k}: {v}");
                }
                if !report.blockers.is_empty() {
                    println!("\nBlockers:");
                    for b in &report.blockers {
                        println!("  [{}] {} — {}", b.code, b.detail, b.recommended_action);
                    }
                }
            }
            if !report.ok {
                std::process::exit(1);
            }
        }
        ReleaseCommands::Doctor {
            version,
            preflight,
            json,
        } => {
            let db = state::Db::open().await?;
            let ver = if let Some(v) = version {
                v
            } else {
                // Use latest known version from release status
                let report = release::build_release_status_report(
                    &db,
                    release::ReleaseStatusQuery {
                        project_id: Some(release::DEFAULT_RELEASE_PROJECT_ID),
                        ref_name: Some("main".into()),
                        sha: None,
                        limit: 1,
                    },
                )
                .await?;
                if let Some(latest) = report.latest.as_ref() {
                    latest.attempt.version.clone()
                } else {
                    return Err(anyhow::anyhow!("no known release version; use --version"));
                }
            };
            let report = release::release_doctor(&ver, preflight).await;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                let status = if report.blockers.is_empty() {
                    "OK"
                } else {
                    "BLOCKED"
                };
                println!("Doctor [{status}]: {}", report.version);
                println!("  next_action: {}", report.next_action);
                println!("  canary_complete: {}", report.canary_complete);
                println!("  prod_complete: {}", report.prod_complete);
                println!("  safe_to_reconcile: {}", report.safe_to_reconcile);
                if !report.preflight.is_empty() {
                    println!("\nPreflight:");
                    for (k, v) in &report.preflight {
                        println!("  {k}: {v}");
                    }
                }
                println!("\nGates:");
                for (k, v) in &report.gates {
                    println!("  {k}: {}", if *v { "present" } else { "MISSING" });
                }
                if !report.blockers.is_empty() {
                    println!("\nBlockers:");
                    for b in &report.blockers {
                        println!("  [{}] {} — {}", b.code, b.detail, b.recommended_action);
                    }
                }
            }
        }
        ReleaseCommands::Ready {
            pr,
            emit_status,
            dry_run,
            json,
        } => {
            let gate = release::compose_gate(pr, dry_run);
            if json {
                println!("{}", serde_json::to_string_pretty(&gate)?);
            } else {
                print!("{}", release::render_gate_text(&gate));
            }
            if emit_status && !dry_run {
                let repo = std::env::var("GITHUB_REPOSITORY")
                    .map_err(|_| anyhow::anyhow!("GITHUB_REPOSITORY env not set"))?;
                let sha = std::env::var("GITHUB_SHA")
                    .map_err(|_| anyhow::anyhow!("GITHUB_SHA env not set"))?;
                let resp = release::post_check_run(&gate, &repo, &sha)?;
                if !json {
                    println!(
                        "\nCheck Run posted. Response head: {}",
                        trim_head(&resp, 200)
                    );
                }
            }
            if !gate.is_pass() && !dry_run {
                std::process::exit(1);
            }
        }
        ReleaseCommands::DryRun { version, json } => {
            let report = run_release_dry_run(&version).await;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("Release dry-run for {}", report.version);
                for (k, v) in &report.checks {
                    println!("  {k}: {v}");
                }
                if !report.blockers.is_empty() {
                    println!("\nBlockers:");
                    for b in &report.blockers {
                        println!("  - {b}");
                    }
                }
            }
            if !report.blockers.is_empty() {
                std::process::exit(1);
            }
        }
        ReleaseCommands::Submit {
            version,
            force,
            dry_run,
        } => {
            run_release_submit(&version, force, dry_run).await?;
        }
        ReleaseCommands::Approve {
            pr,
            as_user,
            dry_run,
        } => {
            run_release_approve(pr, as_user, dry_run).await?;
        }
        ReleaseCommands::Rollback {
            version,
            reason,
            dry_run,
            json,
        } => {
            let report = release::build_report(&version, &reason, dry_run);
            let dir = PathBuf::from(format!("ops/releases/{version}"));
            let written = release::write_evidence(&report, dir)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!(
                    "Rollback {} for {} — {}",
                    report.final_status, report.version, report.reason
                );
                println!("Evidence: {}", written.display());
                for s in &report.steps {
                    let suffix = s.detail.as_deref().unwrap_or("");
                    println!("  [{}] {} — {} ({})", s.n, s.kind, s.description, suffix);
                }
            }
        }
    }
    Ok(())
}

fn trim_head(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

#[derive(Debug, serde::Serialize)]
struct ReleaseDryRunReport {
    version: String,
    checks: Vec<(String, String)>,
    blockers: Vec<String>,
}

async fn run_release_dry_run(version: &str) -> ReleaseDryRunReport {
    let mut checks: Vec<(String, String)> = Vec::new();
    let mut blockers: Vec<String> = Vec::new();

    // Version consistency: VERSION, version.json, Cargo.toml workspace package.version.
    match std::fs::read_to_string("VERSION") {
        Ok(s) => {
            let trimmed = s.trim().to_string();
            checks.push(("VERSION".into(), trimmed.clone()));
            if !version.starts_with(&trimmed) && trimmed != *version {
                blockers.push(format!(
                    "VERSION ({trimmed}) does not match requested release version ({version})"
                ));
            }
        }
        Err(e) => blockers.push(format!("read VERSION: {e}")),
    }
    if !std::path::Path::new("CHANGELOG.md").exists() {
        blockers.push("CHANGELOG.md missing".into());
    } else {
        checks.push(("CHANGELOG.md".into(), "present".into()));
    }
    if !std::path::Path::new("release.policy.toml").exists() {
        blockers.push("release.policy.toml missing".into());
    } else {
        checks.push(("release.policy.toml".into(), "present".into()));
    }

    let preflight = release::release_preflight(None).await;
    checks.push((
        "preflight".into(),
        if preflight.ok {
            "PASS".into()
        } else {
            "FAIL".into()
        },
    ));
    if !preflight.ok {
        blockers.push(format!(
            "preflight failed with {} blocker(s)",
            preflight.blockers.len()
        ));
    }

    ReleaseDryRunReport {
        version: version.to_string(),
        checks,
        blockers,
    }
}

async fn run_release_submit(version: &str, force: bool, dry_run: bool) -> Result<()> {
    // Require a clean working tree.
    let out = tokio::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .await?;
    let dirty = !out.stdout.is_empty();
    if dirty {
        return Err(anyhow::anyhow!(
            "working tree is not clean; commit or stash before `release submit`"
        ));
    }

    // Require a recent successful dry-run (within 30 min) unless --force.
    if !force {
        let cached_path = format!(".jeryu/release-submit-cache/{version}.ok");
        let fresh = std::fs::metadata(&cached_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| t.elapsed().map(|e| e.as_secs() < 1800).unwrap_or(false))
            .unwrap_or(false);
        if !fresh {
            return Err(anyhow::anyhow!(
                "no fresh `release dry-run` result found at {} \
                 (re-run `jeryu release dry-run --version {}` first, or pass --force)",
                cached_path,
                version
            ));
        }
    }

    if dry_run {
        println!("--dry-run: would tag v{version}, push, and trigger release.yml");
        return Ok(());
    }

    // Annotated tag + push + workflow run via gh.
    let tag = format!("v{version}");
    run("git", &["tag", "-a", &tag, "-m", &format!("Release {tag}")]).await?;
    run("git", &["push", "origin", &tag]).await?;
    run(
        "gh",
        &[
            "workflow",
            "run",
            "release.yml",
            "-f",
            &format!("version={version}"),
        ],
    )
    .await?;
    println!("✓ Submitted release {tag}. Track via `jeryu release watch`.");
    Ok(())
}

async fn run_release_approve(pr: u64, as_user: Option<String>, dry_run: bool) -> Result<()> {
    // Refuse self-approval. We compare `as_user` (or `gh api user`) against the PR author.
    let approver = if let Some(u) = as_user {
        u
    } else {
        let out = tokio::process::Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("gh api user failed: {e}"))?;
        if !out.status.success() {
            return Err(anyhow::anyhow!(
                "gh api user failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let pr_str = pr.to_string();
    let author_out = tokio::process::Command::new("gh")
        .args([
            "pr",
            "view",
            &pr_str,
            "--json",
            "author",
            "--jq",
            ".author.login",
        ])
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("gh pr view failed: {e}"))?;
    if !author_out.status.success() {
        return Err(anyhow::anyhow!(
            "gh pr view {pr} failed: {}",
            String::from_utf8_lossy(&author_out.stderr)
        ));
    }
    let author = String::from_utf8_lossy(&author_out.stdout)
        .trim()
        .to_string();

    if !approver.is_empty() && approver == author {
        return Err(anyhow::anyhow!(
            "self-approval refused: PR author and approver are both `{approver}`"
        ));
    }

    // Require CI green.
    let state_out = tokio::process::Command::new("gh")
        .args(["pr", "view", &pr_str, "--json", "statusCheckRollup"])
        .output()
        .await?;
    if state_out.status.success() {
        let body = String::from_utf8_lossy(&state_out.stdout);
        if body.contains("FAILURE") || body.contains("ERROR") {
            return Err(anyhow::anyhow!(
                "CI is not green for PR {pr}; refusing to approve"
            ));
        }
    }

    if dry_run {
        println!("--dry-run: would approve PR {pr} as `{approver}` (author={author})");
        return Ok(());
    }

    run("gh", &["pr", "review", &pr_str, "--approve"]).await?;
    println!("✓ Approved PR #{pr} as `{approver}`.");
    Ok(())
}

async fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let out = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await?;
    if !out.status.success() {
        return Err(anyhow::anyhow!(
            "{} {} failed (exit={:?}): {}",
            cmd,
            args.join(" "),
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}
