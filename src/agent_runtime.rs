use anyhow::{Context, Result};
use tracing::info;

use super::*;
use crate::decision::{RequiredEvidencePolicy, RiskGateDecision, TrustTier, evaluate_risk_gate};

/// Finalize a linear (non-race) agent task: open the tracking issue with the
/// pending label, create the agent branch (main with secondary attempt on master), promote
/// the issue to running, and assemble the AgentTask record. Centralised so
/// `spawn_agent` is one statement and the issue/branch/task shape exists in
/// exactly one place.
async fn finalize_linear_agent_task(
    client: &GitlabClient,
    project_id: i64,
    task_description: &str,
    branch_name: String,
    bot: ProjectPatResp,
) -> Result<AgentTask> {
    let title = format!("[Agent] {}", task_description);
    let body = format!(
        "Autonomous agent task.\n\n\
         **Task:** {}\n\
         **Branch:** `{}`\n\
         **Identity:** `{}`\n\
         **Status:** Pending\n\n\
         _This issue is managed by jeryu agent._",
        task_description, branch_name, bot.name
    );
    let issue = create_tracking_issue_for_agent(
        client,
        project_id,
        &title,
        &body,
        &["agent:pending"],
        &bot,
    )
    .await?;
    info!(
        project_id,
        issue_iid = issue.iid,
        branch = %branch_name,
        bot_id = bot.user_id,
        "agent spawned"
    );
    let _ = create_agent_branch_with_master_attempt(client, project_id, &branch_name).await?;
    client
        .update_issue_labels(project_id, issue.iid, &["agent:running"])
        .await
        .ok();
    Ok(build_agent_task(
        project_id,
        task_description,
        branch_name,
        "main",
        &issue,
        bot,
    ))
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
    let AgentIdentity { branch_name, bot } =
        provision_agent_identity(client, project_id, task_description).await?;

    finalize_linear_agent_task(client, project_id, task_description, branch_name, bot).await
}

/// Spawns a Parallel Hypothesis Race.
/// Creates multiple branches and commits a different patch hypothesis to each.
pub async fn spawn_race(
    client: &GitlabClient,
    project_id: i64,
    task_description: &str,
    hypotheses: Vec<Vec<crate::capability::FileModification>>,
) -> Result<AgentTask> {
    let AgentIdentity {
        branch_name: base_branch_name,
        bot,
    } = provision_agent_identity(client, project_id, task_description).await?;

    let issue = create_tracking_issue_for_agent(
        client,
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
        &bot,
    )
    .await?;
    info!(
        project_id,
        issue_iid = issue.iid,
        "race spawned for {} hypotheses",
        hypotheses.len()
    );

    let attempt_base = match client
        .create_branch(project_id, &base_branch_name, "main")
        .await
    {
        Ok(()) => "main",
        Err(_) => {
            let _ = client
                .create_branch(project_id, &base_branch_name, "master")
                .await;
            "master"
        }
    };

    for (idx, mods) in hypotheses.iter().enumerate() {
        let hypo_branch = format!("{}-hypo-{}", base_branch_name, idx);

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

        let _ = client
            .trigger_pipeline(project_id, &hypo_branch, vec![])
            .await;
    }

    Ok(build_agent_task(
        project_id,
        task_description,
        base_branch_name,
        attempt_base,
        &issue,
        bot,
    ))
}

/// Create a merge request for an agent's work.
pub async fn create_agent_mr(client: &GitlabClient, task: &AgentTask) -> Result<i64> {
    let description = format!(
        "Automated change by jeryu agent.\n\n\
         **Task:** {}\n\n\
         {}",
        task.task_description,
        match task.issue_iid {
            Some(iid) => format!("Closes #{}", iid),
            None => String::new(),
        },
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

fn unmatched_capsule_from_trace(
    job: &crate::gitlab_client::Job,
    project_id: i64,
    log_snippet: String,
    repro_script: String,
    summary: &str,
) -> crate::capsule::FailureCapsule {
    let ref_name = match job.ref_name.clone() {
        Some(r) => r,
        None => "unknown".to_string(),
    };
    crate::capsule::FailureCapsule {
        job_id: job.id,
        pipeline_id: None,
        project_id,
        stage: job.stage.clone(),
        exit_code: 1,
        commit_sha: "unknown".to_string(),
        ref_name,
        working_directory: "/builds/agent".to_string(),
        log_snippet,
        repro_script,
        environment: std::collections::HashMap::new(),
        failure_kind: "unknown".to_string(),
        summary: summary.to_string(),
        superseded_by_sha: None,
        requeued_from_job_id: None,
    }
}

/// Check a pipeline result for an agent's MR and decide next action.
pub async fn check_agent_pipeline(
    client: &GitlabClient,
    task: &AgentTask,
    _mr_iid: i64,
) -> Result<AgentOutcome> {
    let jobs = client
        .list_jobs(task.project_id, &["success", "failed"])
        .await?;

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
                for loser_ref in refs_seen.iter().filter(|r| *r != ref_name) {
                    tracing::info!("Purging losing branch: {}", loser_ref);
                    client.delete_branch(task.project_id, loser_ref).await.ok();
                }
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
            let mut capsules = Vec::new();
            for j in branch_jobs.iter().filter(|j| j.status == "failed") {
                if let Ok(trace) = client
                    .get_job_log_snippet(task.project_id, j.id, 4096)
                    .await
                {
                    capsules.push(unmatched_capsule_from_trace(
                        j,
                        task.project_id,
                        trace,
                        format!("Failed hypothesis: {:?}", j.ref_name),
                        "failed hypothesis race job",
                    ));
                }
            }
            return Ok(AgentOutcome::Failed { capsules });
        }
        return Ok(AgentOutcome::Pending);
    }

    let any_failed = branch_jobs.iter().any(|j| j.status == "failed");
    let all_success = branch_jobs.iter().all(|j| j.status == "success");

    if any_failed {
        let db = crate::state::Db::open().await?;
        let mut capsules = Vec::new();

        for j in &branch_jobs {
            if j.status == "failed" {
                let capsule = db.latest_evidence_for_job(task.project_id, j.id).await?;

                if let Some(c) = capsule {
                    capsules.push(c);
                } else if let Ok(trace) = client
                    .get_job_log_snippet(task.project_id, j.id, 4096)
                    .await
                {
                    capsules.push(unmatched_capsule_from_trace(
                        j,
                        task.project_id,
                        trace,
                        "unknown".to_string(),
                        "failed agent job",
                    ));
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
