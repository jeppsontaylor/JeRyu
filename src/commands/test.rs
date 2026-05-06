use crate::cli::TestCommands;
use crate::dispatch::load_client;
use anyhow::Result;
use jeryu::{state, test_intel, test_runner};
use std::path::PathBuf;

#[path = "test_back.rs"]
mod test_back;
use test_back::{
    build_audit_report, current_commit_sha, git_diff_changed_paths, parse_tag_list,
    write_json_artifact,
};

pub(crate) async fn execute_test_commands(subcmd: TestCommands) -> Result<()> {
    let (client, _) = load_client()?;
    let db = state::Db::open().await?;

    match subcmd {
        TestCommands::Run {
            command,
            project_id,
            image,
            tags,
            timeout,
            force,
        } => {
            let opts = test_runner::TestRunOpts {
                project_id,
                test_command: command,
                job_name: None,
                image,
                tags: parse_tag_list(tags),
                timeout_secs: timeout,
                force,
                commit_sha: current_commit_sha(),
            };
            println!("━━━ jeryu test run ━━━\n");
            println!("  Project ID: {}", opts.project_id);
            println!("  Command:    {}", opts.test_command);
            let plan = test_runner::plan_test_run(&opts);
            println!("  Inferred Routing:");
            println!("    Risk Class: {}", plan.risk_class);
            println!("    Tags:       {:?}", plan.tags);
            for reason in &plan.rationale {
                println!("      - {}", reason);
            }
            println!("\nExecuting pipeline...");

            let result = test_runner::run_test(&db, &client, &opts).await?;
            println!(
                "\nResult: {}",
                if result.passed {
                    "✅ Passed"
                } else {
                    "❌ Failed"
                }
            );
            if let Some(dur) = result.duration_secs {
                println!("Duration: {:.1}s", dur);
            }
            if !result.trace_tail.is_empty() {
                println!("\nTrace tail:\n{}", result.trace_tail);
            }
        }
        TestCommands::Plan {
            command,
            project_id,
            image,
            tags,
            timeout,
        } => {
            let opts = test_runner::TestRunOpts {
                project_id,
                test_command: command,
                job_name: None,
                image,
                tags: parse_tag_list(tags),
                timeout_secs: timeout,
                force: false,
                commit_sha: String::new(),
            };
            println!("━━━ jeryu test plan ━━━\n");
            let plan = test_runner::plan_test_run(&opts);
            println!("  Command:      {}", plan.command);
            println!("  Risk Class:   {}", plan.risk_class);
            println!("  Tags:         {:?}", plan.tags);
            println!("  Timeout:      {}s", plan.timeout_secs);
            println!("  Rationale:");
            for reason in &plan.rationale {
                println!("    - {}", reason);
            }
        }
        TestCommands::Batch {
            commands,
            project_id,
            image,
            tags,
            timeout,
            max_parallel,
            force,
        } => {
            let opts = test_runner::TestBatchOpts {
                project_id,
                test_commands: commands.clone(),
                job_name_prefix: Some("batch-test".to_string()),
                image,
                tags: parse_tag_list(tags),
                timeout_secs: timeout,
                max_parallel,
                force,
                commit_sha: current_commit_sha(),
            };
            println!("🧪 Starting batched test run...");
            println!("   Commands:  {}", opts.test_commands.len());
            println!("   Image:     {}", opts.image);
            let tags_label = match opts.tags.as_ref() {
                Some(tags) => format!("{:?}", tags),
                None => "smart-inferred".to_string(),
            };
            println!("   Tags:      {}", tags_label);
            println!("   Parallel:  {}", opts.max_parallel);
            println!();
            let results = test_runner::run_test_batch(&db, &client, &opts).await?;
            let passed = results.iter().filter(|r| r.passed).count();
            let failed = results.iter().filter(|r| !r.passed).count();
            println!("✅ Batch complete: {} passed, {} failed", passed, failed);
            for r in &results {
                let icon = if r.passed { "✅" } else { "❌" };
                println!(
                    "  {} {:<34} {:<10} pipeline={}",
                    icon, r.job_name, r.status, r.pipeline_id
                );
            }
        }
        TestCommands::Results {
            pipeline_id,
            project_id,
        } => {
            let results = test_runner::pipeline_results(&client, project_id, pipeline_id).await?;

            let passed = results.iter().filter(|r| r.passed).count();
            let failed = results.iter().filter(|r| r.status == "failed").count();
            let skipped = results.iter().filter(|r| r.status == "skipped").count();
            let other = results.len() - passed - failed - skipped;

            println!("Pipeline {} — {} jobs", pipeline_id, results.len());
            println!(
                "  ✅ {} passed  ❌ {} failed  ⏭ {} skipped  ⏳ {} other",
                passed, failed, skipped, other
            );
            println!();

            for r in &results {
                let icon = match r.status.as_str() {
                    "success" => "✅",
                    "failed" => "❌",
                    "skipped" => "⏭ ",
                    "running" => "🔄",
                    "pending" | "created" => "⏳",
                    _ => "❓",
                };
                let dur = match r.duration_secs {
                    Some(d) => format!("{:.0}s", d),
                    None => String::new(),
                };
                println!("  {} {:<40} {:>8} {}", icon, r.job_name, r.status, dur);
            }
        }
        TestCommands::Requeue {
            pipeline_id,
            job_name,
            project_id,
        } => {
            println!(
                "🔄 Requeuing job '{}' in pipeline {}...",
                job_name, pipeline_id
            );
            let result =
                test_runner::requeue_job_by_name(&client, project_id, pipeline_id, &job_name)
                    .await?;

            if result.passed {
                println!("✅ Job '{}' passed after requeue!", job_name);
            } else {
                println!("❌ Job '{}' still failing: {}", job_name, result.status);
            }
        }
        TestCommands::Failed {
            pipeline_id,
            project_id,
        } => {
            let results = test_runner::pipeline_results(&client, project_id, pipeline_id).await?;

            let failed: Vec<_> = results
                .into_iter()
                .filter(|r| r.status == "failed")
                .collect();

            if failed.is_empty() {
                println!("✅ No failed jobs in pipeline {}!", pipeline_id);
            } else {
                println!(
                    "❌ {} failed job(s) in pipeline {}:\n",
                    failed.len(),
                    pipeline_id
                );
                for r in &failed {
                    println!("━━━ {} (id={:?}) ━━━", r.job_name, r.job_id);
                    if !r.trace_tail.is_empty() {
                        let lines: Vec<&str> = r.trace_tail.lines().collect();
                        let start = lines.len().saturating_sub(20);
                        for line in &lines[start..] {
                            println!("  {}", line);
                        }
                    }
                    println!();
                }
            }
        }
        TestCommands::Impact {
            base,
            head,
            repo_root,
            json,
        } => {
            let output = tokio::process::Command::new("cargo")
                .current_dir(&repo_root)
                .args([
                    "run",
                    "-q",
                    "-p",
                    "veox-testctl",
                    "--",
                    "ci-impact",
                    "--base",
                    &base,
                    "--head",
                    &head,
                    "--json",
                ])
                .output()
                .await?;
            if !output.status.success() {
                anyhow::bail!(
                    "ci-impact failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            if json {
                print!("{}", String::from_utf8_lossy(&output.stdout));
            } else {
                let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
                println!("━━━ jeryu test impact ━━━\n");
                println!("  Base/head:          {base}..{head}");
                let release_impacting = match value["release_impacting"].as_bool() {
                    Some(v) => v,
                    None => true,
                };
                let full_build_required = match value["full_build_required"].as_bool() {
                    Some(v) => v,
                    None => true,
                };
                println!("  Release impacting:  {}", release_impacting);
                println!("  Full build:         {}", full_build_required);
                let jobs = match value["jobs"].as_array() {
                    Some(items) => items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    None => String::new(),
                };
                println!("  Jobs:               {jobs}");
                let rules = match value["matched_rules"].as_array() {
                    Some(items) => items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    None => String::new(),
                };
                println!("  Matched rules:      {rules}");
            }
        }
        TestCommands::Choose {
            base,
            head,
            repo_root,
            explain,
            json,
            emit_gitlab,
            emit_plan,
            emit_receipt,
        } => {
            let cwd = match repo_root {
                Some(path) => path,
                None => match std::env::current_dir() {
                    Ok(dir) => dir,
                    Err(_) => PathBuf::from("."),
                },
            };

            let changed_paths = git_diff_changed_paths(&cwd, &base, &head)?;

            // Run the VTI planner
            let plan = test_intel::planner::plan_tests(&changed_paths);
            let receipt = plan.receipt(Some(&base), Some(&head));

            // Output
            if json {
                let json_value = test_intel::explain::explain_json(&plan);
                println!("{}", serde_json::to_string_pretty(&json_value)?);
            } else if explain {
                print!("{}", test_intel::explain::explain(&plan));
            } else {
                println!("━━━ jeryu smart test pick ━━━\n");
                println!("  Base:       {}", base);
                println!("  Head:       {}", head);
                println!("  Changed:    {} files", changed_paths.len());
                println!("  Mode:       {:?}", plan.mode);
                println!("  Confidence: {:.2}", plan.confidence);
                println!("  Receipt:    {}", receipt.receipt_id);
                println!("  Selected:   {} test commands", plan.selected_tests.len());
                println!("  Skipped:    {} subsystems", plan.skipped_subsystems.len());
                if let Some(reason) = plan.repair_reason() {
                    println!("  Repair:     {}", reason);
                }
                println!();
                for test in &plan.selected_tests {
                    println!("  ✓ [{}] {}", test.subsystem, test.command);
                }
            }

            // Emit artifacts
            if let Some(gitlab_path) = emit_gitlab {
                let yaml = test_intel::ci_gen::emit_gitlab_child_yaml(&plan);
                if let Some(parent) = gitlab_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&gitlab_path, &yaml)?;
                eprintln!("Wrote GitLab child pipeline to {}", gitlab_path.display());
            }

            if let Some(plan_path) = emit_plan {
                let json_value = test_intel::explain::explain_json(&plan);
                write_json_artifact(&plan_path, &json_value, "test plan")?;
            }

            if let Some(receipt_path) = emit_receipt {
                write_json_artifact(&receipt_path, &receipt, "VTI receipt")?;
            }
        }
        TestCommands::ExplainPlan { plan_path } => {
            let contents = std::fs::read_to_string(&plan_path)?;
            let plan: test_intel::planner::TestPlan = serde_json::from_str(&contents)?;
            print!("{}", test_intel::explain::explain(&plan));
        }
        other => return test_back::run(other, &db).await,
    }
    Ok(())
}
