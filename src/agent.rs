//! Owner: Autonomous Agent System
//! Proof: `cargo test -p jeryu -- agent`
//! Invariants: Agents always create a GitLab issue before branching; race hypotheses are independent branches; pipeline check (check_agent_pipeline) is mandatory before merge
//!
//! An agent is a Rust-spawned worker that:
//! 1. Creates a branch on a target repo
//! 2. Performs an automated task (refactor, test gen, lint fix, etc.)
//! 3. Commits and pushes
//! 4. Opens a Merge Request (which triggers CI automatically)
//! 5. Watches the pipeline result
//! 6. If CI fails: reads traces, analyzes errors, fixes, force-pushes
//! 7. If CI passes: can auto-merge or flag for review
//!
//! Agent tasks are tracked as GitLab Issues with labels:
//!   agent:pending, agent:running, agent:done, agent:failed

use anyhow::{Context, Result};
use tracing::info;

use crate::decision::{RequiredEvidencePolicy, RiskGateDecision, TrustTier, evaluate_risk_gate};
use crate::gitlab_client::GitlabClient;

// ---------------------------------------------------------------------------
// Agent definition
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AgentTask {
    pub project_id: i64,
    pub task_description: String,
    pub branch_name: String,
    pub target_branch: String,
    pub issue_iid: Option<i64>,
    pub bot_user_id: Option<i64>,
    pub bot_token: Option<String>,
}

/// Spawn an autonomous agent as a background task.
///
/// This creates a GitLab issue to track the work, creates a branch,
/// and returns immediately. The actual work is done asynchronously.
pub async fn spawn_agent(
    client: &GitlabClient,
    project_id: i64,
    task_description: &str,
) -> Result<AgentTask> {
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let slug = task_description
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    let branch_name = format!("agent/{}-{}", slug, timestamp);

    // 1. Provision Ephemeral Bot Identity
    let bot_name = format!(
        "@agent-{}-{}",
        slug,
        timestamp
            .to_string()
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
    );

    // Tokens expire tomorrow (auto-cleanup safety)
    let expires_at = (chrono::Utc::now() + chrono::Duration::try_days(2).unwrap())
        .format("%Y-%m-%d")
        .to_string();

    let bot = client
        .create_project_bot(
            project_id,
            &bot_name,
            &["api", "write_repository"],
            &expires_at,
            30, // Developer access (Least Privilege)
        )
        .await
        .context("provisioning ephemeral bot identity")?;

    // 2. Create tracking issue and assign it to the bot
    let issue = client
        .create_issue(
            project_id,
            &format!("[Agent] {}", task_description),
            &format!(
                "Autonomous agent task.\n\n\
                 **Task:** {}\n\
                 **Branch:** `{}`\n\
                 **Identity:** `{}`\n\
                 **Status:** Pending\n\n\
                 _This issue is managed by jeryu agent._",
                task_description, branch_name, bot.name
            ),
            &["agent:pending"],
            Some(bot.user_id),
        )
        .await
        .context("creating tracking issue")?;

    info!(
        project_id,
        issue_iid = issue.iid,
        branch = %branch_name,
        bot_id = bot.user_id,
        "agent spawned"
    );

    // Create branch from default branch
    let branch_result = client.create_branch(project_id, &branch_name, "main").await;

    if branch_result.is_err() {
        // Try "master" if "main" doesn't exist
        client
            .create_branch(project_id, &branch_name, "master")
            .await
            .context("creating agent branch (tried both 'main' and 'master')")?;
    }

    // Update issue to running
    client
        .update_issue_labels(project_id, issue.iid, &["agent:running"])
        .await
        .ok();

    let task = AgentTask {
        project_id,
        task_description: task_description.to_string(),
        branch_name,
        target_branch: "main".to_string(),
        issue_iid: Some(issue.iid),
        bot_user_id: Some(bot.user_id),
        bot_token: Some(bot.token),
    };

    Ok(task)
}

/// Spawns a Parallel Hypothesis Race.
/// Creates multiple branches and commits a different patch hypothesis to each.
pub async fn spawn_race(
    client: &GitlabClient,
    project_id: i64,
    task_description: &str,
    hypotheses: Vec<Vec<crate::capability::FileModification>>,
) -> Result<AgentTask> {
    // 1. Setup similar identity and issue to spawn_agent
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let slug = task_description
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .split_whitespace()
        .take(4)
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase();
    let base_branch_name = format!("agent/{}-{}", slug, timestamp);

    let bot_name = format!(
        "@agent-{}-{}",
        slug,
        timestamp
            .to_string()
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
    );

    let expires_at = (chrono::Utc::now() + chrono::Duration::try_days(2).unwrap())
        .format("%Y-%m-%d")
        .to_string();

    let bot = client
        .create_project_bot(
            project_id,
            &bot_name,
            &["api", "write_repository"],
            &expires_at,
            30,
        )
        .await?;

    let issue = client
        .create_issue(
            project_id,
            &format!("[Race] {}", task_description),
            &format!(
                "Autonomous agent racing {} hypotheses.\n\n\
                 **Task:** {}\n\
                 **Base Branch:** `{}`\n\
                 **Identity:** `{}`\n\n\
                 _This issue is managed by jeryu Parallel Hypothesis Racing._",
                hypotheses.len(),
                task_description,
                base_branch_name,
                bot.name
            ),
            &["agent:running", "agent:race"],
            Some(bot.user_id),
        )
        .await?;

    info!(
        project_id,
        issue_iid = issue.iid,
        "race spawned for {} hypotheses",
        hypotheses.len()
    );

    let mut attempt_base = "main";
    if client
        .create_branch(project_id, &base_branch_name, attempt_base)
        .await
        .is_err()
    {
        attempt_base = "master";
        let _ = client
            .create_branch(project_id, &base_branch_name, attempt_base)
            .await;
    }

    // Fan-out: Create a branch + commit for each hypothesis!
    for (idx, mods) in hypotheses.iter().enumerate() {
        let hypo_branch = format!("{}-hypo-{}", base_branch_name, idx);

        // Fork off the base branch we just created
        let _ = client
            .create_branch(project_id, &hypo_branch, &base_branch_name)
            .await;

        let files: Vec<(&str, &str, &str)> = mods
            .iter()
            .map(|m| ("update", m.file_path.as_str(), m.content.as_str()))
            .collect();
        let msg = format!("Apply patch hypothesis {}", idx);

        let _ = client
            .commit_actions(project_id, &hypo_branch, &msg, &files)
            .await;

        // Often GitLab CI fires on branch creation + push.
        // We trigger explicitly if needed as backup.
        let _ = client
            .trigger_pipeline(project_id, &hypo_branch, vec![])
            .await;
    }

    Ok(AgentTask {
        project_id,
        task_description: task_description.to_string(),
        branch_name: base_branch_name,
        target_branch: attempt_base.to_string(),
        issue_iid: Some(issue.iid),
        bot_user_id: Some(bot.user_id),
        bot_token: Some(bot.token),
    })
}

/// Create a merge request for an agent's work.
pub async fn create_agent_mr(client: &GitlabClient, task: &AgentTask) -> Result<i64> {
    let description = format!(
        "Automated change by jeryu agent.\n\n\
         **Task:** {}\n\n\
         {}",
        task.task_description,
        task.issue_iid
            .map(|iid| format!("Closes #{}", iid))
            .unwrap_or_default(),
    );

    let mr = client
        .create_merge_request(
            task.project_id,
            &task.branch_name,
            &task.target_branch,
            &format!("[Agent] {}", task.task_description),
            &description,
        )
        .await
        .context("creating merge request")?;

    info!(
        project_id = task.project_id,
        mr_iid = mr.iid,
        "agent created merge request"
    );

    Ok(mr.iid)
}

/// Check a pipeline result for an agent's MR and decide next action.
pub async fn check_agent_pipeline(
    client: &GitlabClient,
    task: &AgentTask,
    _mr_iid: i64,
) -> Result<AgentOutcome> {
    // Get the latest jobs for this project to check pipeline status
    let jobs = client
        .list_jobs(task.project_id, &["success", "failed"])
        .await?;

    // Find jobs on our branch or our hypo branches
    let branch_jobs: Vec<_> = jobs
        .iter()
        .filter(|j| {
            j.ref_name
                .as_deref()
                .map(|r| r.starts_with(&task.branch_name))
                .unwrap_or(false)
        })
        .collect();

    if branch_jobs.is_empty() {
        return Ok(AgentOutcome::Pending);
    }

    // Determine unique refs to see if this is a Race
    let mut refs_seen = std::collections::HashSet::new();
    for j in &branch_jobs {
        if let Some(r) = &j.ref_name {
            refs_seen.insert(r.clone());
        }
    }

    let is_race = refs_seen.len() > 1
        || branch_jobs
            .iter()
            .any(|j| j.ref_name.as_deref().unwrap_or("").contains("-hypo-"));

    if is_race {
        info!("Reviewing parallel hypothesis race pipelines...");
        for ref_name in &refs_seen {
            let ref_jobs: Vec<_> = branch_jobs
                .iter()
                .filter(|j| j.ref_name.as_deref() == Some(ref_name))
                .collect();
            let all_success = ref_jobs.iter().all(|j| j.status == "success");

            if all_success {
                info!("🏁 Race winner determined: {}!", ref_name);
                // Purge losers
                for loser_ref in refs_seen.iter().filter(|r| *r != ref_name) {
                    tracing::info!("Purging losing branch: {}", loser_ref);
                    client.delete_branch(task.project_id, loser_ref).await.ok();
                }

                // TODO: Merge the winner back into the base branch automatically
                // using GitLab MR or API. For now, declare success.
                return Ok(AgentOutcome::Success);
            }
        }

        let all_failed = refs_seen.iter().all(|r| {
            let ref_jobs: Vec<_> = branch_jobs
                .iter()
                .filter(|j| j.ref_name.as_deref() == Some(r))
                .collect();
            ref_jobs.iter().any(|j| j.status == "failed")
        });

        if all_failed {
            // All hypotheses failed
            let mut capsules = Vec::new();
            for j in branch_jobs.iter().filter(|j| j.status == "failed") {
                if let Ok(trace) = client
                    .get_job_log_snippet(task.project_id, j.id, 4096)
                    .await
                {
                    capsules.push(crate::capsule::FailureCapsule {
                        job_id: j.id,
                        pipeline_id: None,
                        project_id: task.project_id,
                        stage: j.stage.clone(),
                        exit_code: 1,
                        commit_sha: "unknown".to_string(),
                        ref_name: j.ref_name.clone().unwrap_or_else(|| "unknown".to_string()),
                        working_directory: "/builds/agent".to_string(),
                        log_snippet: trace,
                        repro_script: format!("Failed hypothesis: {:?}", j.ref_name),
                        environment: std::collections::HashMap::new(),
                        failure_kind: "unknown".to_string(),
                        summary: "failed hypothesis race job".to_string(),
                        superseded_by_sha: None,
                        retried_from_job_id: None,
                    });
                }
            }
            return Ok(AgentOutcome::Failed { capsules });
        }
        return Ok(AgentOutcome::Pending);
    }

    // Standard linear agent flow
    let any_failed = branch_jobs.iter().any(|j| j.status == "failed");
    let all_success = branch_jobs.iter().all(|j| j.status == "success");

    if any_failed {
        let db = crate::state::Db::open().await?;
        let mut capsules = Vec::new();

        for j in &branch_jobs {
            if j.status == "failed" {
                // Try to find a failure capsule in the event ledger first
                let capsule = db.latest_evidence_for_job(task.project_id, j.id).await?;

                if let Some(c) = capsule {
                    capsules.push(c);
                } else if let Ok(trace) = client
                    .get_job_log_snippet(task.project_id, j.id, 4096)
                    .await
                {
                    // Fallback to raw trace snippet if no capsule found
                    capsules.push(crate::capsule::FailureCapsule {
                        job_id: j.id,
                        pipeline_id: None,
                        project_id: task.project_id,
                        stage: j.stage.clone(),
                        exit_code: 1,
                        commit_sha: "unknown".to_string(),
                        ref_name: j.ref_name.clone().unwrap_or_else(|| "unknown".to_string()),
                        working_directory: "/builds/agent".to_string(),
                        log_snippet: trace,
                        repro_script: "unknown".to_string(),
                        environment: std::collections::HashMap::new(),
                        failure_kind: "unknown".to_string(),
                        summary: "failed agent job".to_string(),
                        superseded_by_sha: None,
                        retried_from_job_id: None,
                    });
                }
            }
        }

        Ok(AgentOutcome::Failed { capsules })
    } else if all_success {
        Ok(AgentOutcome::Success)
    } else {
        Ok(AgentOutcome::Pending)
    }
}

#[derive(Debug)]
pub enum AgentOutcome {
    Pending,
    Success,
    Failed {
        capsules: Vec<crate::capsule::FailureCapsule>,
    },
}

/// Mark an agent task as completed.
pub async fn complete_agent(client: &GitlabClient, task: &AgentTask, success: bool) -> Result<()> {
    if let Some(issue_iid) = task.issue_iid {
        let label = if success {
            "agent:done"
        } else {
            "agent:failed"
        };
        client
            .update_issue_labels(task.project_id, issue_iid, &[label])
            .await
            .ok();

        let comment = if success {
            "✅ Agent task completed successfully. Pipeline passed."
        } else {
            "❌ Agent task failed. See pipeline logs for details."
        };
        client
            .comment_on_issue(task.project_id, issue_iid, comment)
            .await
            .ok();
    }

    Ok(())
}

/// List active agent issues for a project.
pub async fn list_agents(
    client: &GitlabClient,
    project_id: i64,
) -> Result<Vec<crate::gitlab_client::Issue>> {
    let mut active = client
        .list_issues_by_labels(project_id, &["agent:running"], Some("opened"))
        .await?;
    let mut pending = client
        .list_issues_by_labels(project_id, &["agent:pending"], Some("opened"))
        .await?;
    active.append(&mut pending);
    active.sort_by_key(|issue| issue.iid);
    active.dedup_by_key(|issue| issue.iid);
    Ok(active)
}

pub async fn merge_agent_mr(
    client: &GitlabClient,
    project_id: i64,
    mr_iid: i64,
    trust_tier: TrustTier,
) -> Result<crate::decision::RiskEvaluation> {
    let mr = client.get_merge_request(project_id, mr_iid).await?;
    let jobs = client
        .list_jobs(
            project_id,
            &["success", "failed", "pending", "running", "created"],
        )
        .await?;

    let branch_jobs: Vec<_> = jobs
        .iter()
        .filter(|job| job.ref_name.as_deref() == Some(mr.source_branch.as_str()))
        .collect();

    let successful_jobs = branch_jobs
        .iter()
        .filter(|job| job.status == "success")
        .count();
    let failed_jobs = branch_jobs
        .iter()
        .filter(|job| job.status == "failed")
        .count();
    let pending_jobs = branch_jobs
        .iter()
        .filter(|job| matches!(job.status.as_str(), "pending" | "running" | "created"))
        .count();

    let evaluation = evaluate_risk_gate(
        trust_tier.clone(),
        successful_jobs,
        pending_jobs,
        failed_jobs,
        &RequiredEvidencePolicy::default(),
    );

    let db = crate::state::Db::open().await?;
    db.append_event(
        "risk_gate_decision",
        Some(project_id),
        None,
        "agent",
        &serde_json::json!({
            "mr_iid": mr_iid,
            "source_branch": mr.source_branch,
            "successful_jobs": successful_jobs,
            "pending_jobs": pending_jobs,
            "failed_jobs": failed_jobs,
            "trust_tier": trust_tier,
            "decision": evaluation.decision,
            "reason": evaluation.reason,
        })
        .to_string(),
    )
    .await?;

    if evaluation.decision == RiskGateDecision::Allow {
        client.accept_merge_request(project_id, mr_iid).await?;
    }

    Ok(evaluation)
}
