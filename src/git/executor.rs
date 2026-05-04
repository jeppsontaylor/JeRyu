//! Owner: Git passthrough execution and event recording
//! Proof: `cargo test -p jeryu -- git_passthrough`
//! Invariants: Each command invokes the real git binary exactly once before any optional mirror step.

use anyhow::Result;
use chrono::Utc;

use crate::git::event::GitCommandEvent;
use crate::git::invocation::GitInvocation;
use crate::git::mirror::{mirror_push_plan, parse_push_mirror_plan};
use crate::git::policy::{mirror_remote, should_mirror, strict_mode_enabled};
use crate::git::snapshot::{capture, snapshot_or_empty};
use crate::git::store::store_git_event;
use crate::git::system::SystemGit;
use crate::state::{Db, GitMirrorJob, GitRefUpdate};

pub async fn execute_git(db: Option<&Db>, argv: &[String]) -> Result<i32> {
    let cwd = std::env::current_dir()?;
    let invocation = GitInvocation::new(&cwd, argv.to_vec());
    let git = SystemGit::resolve()?;
    let before = snapshot_or_empty(&cwd);
    let git_args: Vec<&str> = argv.iter().map(String::as_str).collect();

    let status = git.status(&cwd, &git_args)?;
    let exit_code = status.code().unwrap_or(1);
    let after = capture(&cwd).ok();

    let mut mirror_status = "not_attempted".to_string();
    let mut sidecar_status = "ok".to_string();
    let mut final_exit_code = exit_code;
    if exit_code == 0 && should_mirror(invocation.class, &invocation.argv) {
        let remote = mirror_remote();
        let mirror_plan = parse_push_mirror_plan(argv, &remote);
        let mirrored = mirror_plan
            .as_ref()
            .map(|plan| mirror_push_plan(&cwd, plan).unwrap_or(false))
            .unwrap_or(false);
        mirror_status = if mirrored {
            "mirrored".to_string()
        } else {
            "mirror_failed".to_string()
        };
        if !mirrored && strict_mode_enabled() {
            final_exit_code = 1;
        }

        if let (Some(db), Some(plan)) = (db, mirror_plan.as_ref()) {
            let job = GitMirrorJob {
                id: 0,
                request_id: invocation.request_id.clone(),
                remote_name: plan.remote_name.clone(),
                branch_name: plan.ref_name.clone(),
                status: mirror_status.clone(),
                detail: plan.git_args.join(" "),
                created_at: Utc::now().to_rfc3339(),
            };
            if let Err(err) = db.record_git_mirror_job(&job).await {
                tracing::warn!(error = %err, request_id = %invocation.request_id, "failed to record git mirror job");
                sidecar_status = "db_write_failed".to_string();
            }
        }
    }

    if let Some(db) = db {
        if let Some(update) =
            ref_update_from_snapshots(&invocation, &before, after.as_ref(), &mirror_status)
        {
            if let Err(err) = db.record_git_ref_update(&update).await {
                tracing::warn!(error = %err, request_id = %invocation.request_id, "failed to record git ref update");
                sidecar_status = "db_write_failed".to_string();
            }
        }

        let event = GitCommandEvent::from_invocation(
            &invocation,
            before,
            after,
            final_exit_code,
            sidecar_status,
            mirror_status.clone(),
        );
        if let Err(err) = store_git_event(db, &event).await {
            tracing::warn!(error = %err, request_id = %invocation.request_id, "failed to record git command event");
        }
    }

    Ok(final_exit_code)
}

pub async fn execute_git_once(argv: &[String]) -> Result<i32> {
    execute_git(None, argv).await
}

fn ref_update_from_snapshots(
    invocation: &GitInvocation,
    before: &crate::git::GitSnapshot,
    after: Option<&crate::git::GitSnapshot>,
    mirror_status: &str,
) -> Option<GitRefUpdate> {
    let after = after?;
    let changed = before.head != after.head || before.branch != after.branch;
    if !changed && !invocation.is_push() {
        return None;
    }

    Some(GitRefUpdate {
        id: 0,
        request_id: invocation.request_id.clone(),
        ref_name: after
            .branch
            .clone()
            .or_else(|| before.branch.clone())
            .unwrap_or_else(|| "HEAD".to_string()),
        before_sha: before.head.clone(),
        after_sha: after.head.clone(),
        status: if invocation.is_push() {
            mirror_status.to_string()
        } else {
            "observed".to_string()
        },
        created_at: Utc::now().to_rfc3339(),
    })
}
