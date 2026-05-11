use super::*;

pub(crate) async fn run(command: Commands) -> Result<i32> {
    match command {
        // ---- Cache -------------------------------------------------------
        Commands::Cache(subcmd) => {
            let db = state::Db::open().await?;
            let sc = cache::SmartCache::new(db);
            match subcmd {
                CacheCommands::Enable => {
                    sc.enable().await?;
                }
                CacheCommands::Doctor => {
                    sc.doctor().await?;
                }
                CacheCommands::Status { json } => {
                    sc.status_with_options(json).await?;
                }
                CacheCommands::Gc {
                    dry_run,
                    json,
                    keep_active_managers,
                    older_than,
                    max_cache_gb,
                } => {
                    sc.gc_with_options(cache::GcOptions {
                        dry_run,
                        json,
                        keep_active_managers,
                        older_than,
                        max_cache_gb,
                        quiet: false,
                    })
                    .await?;
                }
            }
        }

        // ---- Local ------------------------------------------------------
        Commands::Local(subcmd) => match subcmd {
            LocalCommands::Cargo { repo, cargo_args } => {
                local::run_cargo(repo, cargo_args).await?;
            }
            LocalCommands::CargoEnv { repo, json } => {
                let layout = local::cargo_env(repo)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&layout)?);
                } else {
                    let exports = cargo_cache::shell_exports(&layout);
                    if !exports.is_empty() {
                        println!("{}", exports.join("\n"));
                    }
                }
            }
        },

        // ---- Logs --------------------------------------------------------
        Commands::Logs { manager_id, lines } => {
            let db = state::Db::open().await?;
            let docker_ctl = docker::DockerCtl::connect()?;
            let log_lines = logs::tail_manager(&db, &docker_ctl, &manager_id, lines).await?;
            logs::print_manager_logs(&manager_id, &log_lines);
        }

        // ---- Agent -------------------------------------------------------
        Commands::Agent(subcmd) => {
            let (client, _) = load_client()?;

            match subcmd {
                AgentCommands::Spawn { project_id, task } => {
                    let agent_task = agent::spawn_agent(&client, project_id, &task).await?;
                    println!("🤖 Agent spawned!");
                    println!("   Project:  {}", agent_task.project_id);
                    println!("   Branch:   {}", agent_task.branch_name);
                    println!("   Issue:    #{}", agent_task.issue_iid.unwrap_or(0));
                    println!("   Task:     {}", agent_task.task_description);
                }
                AgentCommands::List { project_id } => {
                    let agents = agent::list_agents(&client, project_id).await?;
                    if agents.is_empty() {
                        println!("No active agents.");
                    } else {
                        for a in &agents {
                            println!("  #{:<5} [{}] {}", a.iid, a.labels.join(", "), a.title);
                        }
                    }
                }
                AgentCommands::Merge {
                    project_id,
                    mr_iid,
                    trust_tier,
                } => {
                    let trust_tier = trust_tier
                        .parse::<decision::TrustTier>()
                        .unwrap_or(decision::TrustTier::Trusted);
                    let evaluation =
                        agent::merge_agent_mr(&client, project_id, mr_iid, trust_tier).await?;
                    println!("Risk gate: {:?}", evaluation.decision);
                    println!("Reason:    {}", evaluation.reason);
                }
            }
        }

        // ---- Test --------------------------------------------------------
        Commands::Test(subcmd) => crate::commands::test::execute_test_commands(subcmd).await?,

        // ---- Settings ---------------------------------------------------
        Commands::Settings(subcmd) => {
            crate::commands::settings::execute_settings_commands(subcmd).await?
        }

        // ---- Release ----------------------------------------------------
        Commands::Release(subcmd) => {
            crate::commands::release::execute_release_commands(subcmd).await?
        }
        // ---- Secrets ----------------------------------------------------
        Commands::Secrets(subcmd) => {
            crate::commands::secrets::execute_secrets_commands(subcmd).await?
        }

        // ---- Progress ---------------------------------------------------
        Commands::Progress {
            project_id,
            ref_name,
            json,
        } => {
            let (client, _) = load_client()?;
            let db = state::Db::open().await?;
            let report =
                release::build_progress_report(&db, &client, project_id, &ref_name).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                print!("{}", release::render_progress_text(&report));
            }
        }

        // ---- Repo --------------------------------------------------------
        Commands::Repo(subcmd) => {
            return crate::commands::repo::execute_repo_commands(subcmd).await;
        }

        // ---- Host --------------------------------------------------------
        Commands::Host(subcmd) => {
            return crate::commands::host::execute_host_commands(subcmd).await;
        }

        // ---- Exec -------------------------------------------------------
        Commands::Exec(_) => unreachable!("exec command is handled in main"), // allowlist: typed clap subcommand; invocations stay typed

        // ---- Server Hooks ------------------------------------------------
        Commands::ServerHook(subcmd) => match subcmd {
            ServerHookCommands::PreReceive => {
                admission::run_pre_receive_hook().await?;
            }
        },

        // ---- Action list -------------------------------------------------
        Commands::Action(subcmd) => match subcmd {
            ActionCommands::List { json } => {
                use jeryu::tui::action_registry::{self, Surface};
                if json {
                    let entries: Vec<serde_json::Value> = action_registry::REGISTRY
                        .iter()
                        .map(|e| {
                            serde_json::json!({
                                "id": e.id,
                                "label": e.label,
                                "key_hint": e.key_hint,
                                "risk_tier": e.risk_tier.label(),
                                "dry_run": e.dry_run,
                                "description": e.description,
                                "surfaces": e.surfaces.iter().map(|s| match s {
                                    Surface::Cli => "cli",
                                    Surface::Tui => "tui",
                                    Surface::Capability => "capability",
                                }).collect::<Vec<_>>(),
                            })
                        })
                        .collect();
                    println!("{}", serde_json::to_string_pretty(&entries)?);
                } else {
                    println!("{:<24} {:<12} {:<10} DESCRIPTION", "ACTION", "RISK", "KEY");
                    println!("{}", "─".repeat(80));
                    for e in action_registry::REGISTRY {
                        println!(
                            "{:<24} {:<12} {:<10} {}",
                            e.id,
                            e.risk_tier.label(),
                            e.key_hint.unwrap_or(""),
                            e.description,
                        );
                    }
                }
            }
        },

        // ---- Capability Server -------------------------------------------
        Commands::Capability(subcmd) => match subcmd {
            CapabilityCommands::Serve { socket_path } => {
                let (client, _) = load_client()?;
                capability::start_capability_server(&socket_path, client).await?;
            }
        },

        // ---- MCP Adapter -------------------------------------------------
        Commands::Mcp(subcmd) => match subcmd {
            McpCommands::Serve => {
                let (client, _) = load_client()?;
                mcp::start_mcp_stdio(client).await?;
            }
            McpCommands::ServeHttp => {
                let (client, _) = load_client()?;
                let bind = settings::get().mcp.bind.clone();
                mcp::start_mcp_http(client, &bind).await?;
            }
            McpCommands::Tools { json } => {
                let manifest = mcp::tool_manifest();
                if json {
                    println!("{}", serde_json::to_string_pretty(&manifest)?);
                } else {
                    for tool in manifest {
                        println!(
                            "{:<28} {:<18} {}",
                            tool["name"].as_str().unwrap_or(""),
                            tool["title"].as_str().unwrap_or(""),
                            tool["description"].as_str().unwrap_or(""),
                        );
                    }
                }
            }
        },

        // ---- Next --------------------------------------------------------
        Commands::Next {
            project_id,
            ref_name,
        } => {
            let db = state::Db::open().await?;

            let pipelines = db
                .list_active_pipelines_for_ref(project_id, &ref_name)
                .await?;
            let release = db.latest_release_attempt(project_id, &ref_name).await?;
            let since = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
            let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);
            let evidence = db.list_evidence_for_ref(project_id, &ref_name, 1).await?;

            println!("━━━ jeryu next — {ref_name} ━━━\n");

            if let Some(rec) = evidence.first() {
                println!("  ● PRIORITY: pipeline failure detected");
                println!(
                    "    job={}  stage={}  kind={}  sha={}",
                    rec.job_id,
                    rec.stage,
                    rec.failure_kind,
                    &rec.commit_sha[..rec.commit_sha.len().min(12)]
                );
                println!(
                    "    Action: jeryu job explain --project-id {project_id} --job-id {}",
                    rec.job_id
                );
                println!();
            }

            if !pipelines.is_empty() {
                println!("  ● {} active pipeline(s) for {ref_name}", pipelines.len());
                for p in &pipelines {
                    println!(
                        "    pipeline={}  status={}  updated={}",
                        p.pipeline_id, p.status, p.updated_at
                    );
                }
                println!();
            }

            if let Some(rel) = &release {
                let upstream_ok = rel.upstream_status == "success";
                let canary_ok = rel.canary_status == "passed" || rel.canary_status == "skipped";
                if !upstream_ok {
                    println!(
                        "  ● Release blocked: upstream pipeline status={}",
                        rel.upstream_status
                    );
                    println!(
                        "    Action: jeryu release status --project-id {project_id} --ref-name {ref_name}"
                    );
                } else if !canary_ok {
                    println!(
                        "  ⟳ Release in progress: canary_status={}",
                        rel.canary_status
                    );
                    println!(
                        "    Action: jeryu release status --project-id {project_id} --ref-name {ref_name}"
                    );
                } else {
                    println!(
                        "  ✓ Release gate OK (sha={})",
                        &rel.sha[..rel.sha.len().min(12)]
                    );
                }
                println!();
            } else {
                println!("  ○ No release attempt tracked for {ref_name}");
                println!();
            }

            if miss_count > 0 {
                println!("  ● {miss_count} unrepaired selector miss(es) in last 7 days");
                println!(
                    "    Action: jeryu test audit --changed <files> --failed <tests> --sha HEAD"
                );
                println!();
            }

            if evidence.is_empty() && pipelines.is_empty() && miss_count == 0 {
                println!("  ✓ No active issues detected for {ref_name}.");
            }
        }

        // ---- ExplainBlocker ----------------------------------------------
        Commands::ExplainBlocker {
            entity_type,
            entity_id,
        } => {
            let db = state::Db::open().await?;

            println!("━━━ jeryu explain-blocker {entity_type}:{entity_id} ━━━\n");

            match entity_type.as_str() {
                "job" => {
                    if let Some(cap) = db.latest_evidence_by_job_id(entity_id).await? {
                        println!(
                            "  job={}  stage={}  ref={}",
                            cap.job_id, cap.stage, cap.ref_name
                        );
                        println!(
                            "  commit:       {}",
                            &cap.commit_sha[..cap.commit_sha.len().min(12)]
                        );
                        println!("  failure_kind: {}", cap.failure_kind);
                        println!("  exit_code:    {}", cap.exit_code);
                        println!("  classified:   {:?}", cap.classify());
                        println!("  recovery_advice: {:?}", cap.recommended_recovery());
                        println!("  summary:      {}", cap.summary);
                        if !cap.repro_script.is_empty() {
                            println!("\n  Repro script:\n    {}", cap.repro_script);
                        }
                        if !cap.log_snippet.is_empty() {
                            println!("\n  Log (last 10 lines):");
                            for line in cap
                                .log_snippet
                                .lines()
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .take(10)
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                            {
                                println!("    {}", line);
                            }
                        }
                        if let Some(sup) = &cap.superseded_by_sha {
                            println!(
                                "\n  Note: superseded by commit {}",
                                &sup[..sup.len().min(12)]
                            );
                        }
                    } else {
                        println!("  No failure capsule found for job {entity_id}.");
                        println!("  Try: jeryu job trace --project-id <id> --job-id {entity_id}");
                    }
                }
                "release" => {
                    let attempts = db.recent_release_attempts(None, None, 20).await?;
                    if let Some(rel) = attempts.iter().find(|r| r.id == entity_id) {
                        println!(
                            "  id={}  ref={}  sha={}",
                            rel.id,
                            rel.ref_name,
                            &rel.sha[..rel.sha.len().min(12)]
                        );
                        println!(
                            "  upstream:     {} (pipeline={:?})",
                            rel.upstream_status, rel.upstream_pipeline_id
                        );
                        println!(
                            "  canary:       {} (started={:?})",
                            rel.canary_status, rel.canary_started_at
                        );
                        println!(
                            "  release_pipe: {:?} status={:?}",
                            rel.release_pipeline_id, rel.release_pipeline_status
                        );
                        println!(
                            "  prod_pipe:    {:?} status={:?}",
                            rel.production_pipeline_id, rel.production_pipeline_status
                        );
                        println!();
                        if rel.upstream_status != "success" {
                            println!(
                                "  BLOCKER: upstream pipeline not green (status={})",
                                rel.upstream_status
                            );
                        }
                        if rel.canary_status == "running" {
                            println!("  WAITING: canary still running");
                        } else if rel.canary_status == "failed" {
                            println!("  BLOCKER: canary failed — {:?}", rel.canary_note);
                        }
                        if rel.production_pipeline_status.as_deref() == Some("failed") {
                            println!("  BLOCKER: production pipeline failed");
                        }
                        println!(
                            "\n  Action: jeryu release status --project-id {} --ref-name {}",
                            rel.project_id, rel.ref_name
                        );
                    } else {
                        println!("  No release attempt with id={entity_id} found.");
                    }
                }
                "merge" => {
                    let since = (chrono::Utc::now() - chrono::Duration::days(30)).to_rfc3339();
                    let miss_count = db.count_selector_misses_since(&since).await.unwrap_or(0);
                    println!("  MR iid: {entity_id}");
                    println!("  Selector misses (30d): {miss_count}");
                    if miss_count > 0 {
                        println!("  BLOCKER: {miss_count} unrepaired test selector miss(es).");
                        println!(
                            "  Action:  jeryu test audit --changed <files> --failed <tests> --sha HEAD"
                        );
                    } else {
                        println!("  ✓ No selector misses.");
                    }
                    println!("\n  For full pipeline/approval status:");
                    println!("    jeryu pipeline explain --project-id <id> --pipeline-id <id>");
                }
                other => {
                    println!("  Unknown entity type '{other}'. Supported: job | release | merge");
                }
            }
        }

        _ => unreachable!("dispatch_back handles cache and later commands"),
    }

    Ok(0)
}
