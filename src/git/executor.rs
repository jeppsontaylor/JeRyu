//! Owner: Git passthrough execution and event recording
//! Proof: `cargo test -p jeryu -- git_passthrough`
//! Invariants: Each command invokes the real git binary exactly once before any optional mirror step.

use anyhow::Result;

use crate::git::event::GitCommandEvent;
use crate::git::invocation::GitInvocation;
use crate::git::mirror::mirror_push;
use crate::git::policy::{should_mirror, strict_mode_enabled};
use crate::git::snapshot::{capture, snapshot_or_empty};
use crate::git::store::store_git_event;
use crate::git::system::SystemGit;
use crate::state::Db;

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
    let mut final_exit_code = exit_code;
    if exit_code == 0 && should_mirror(invocation.class, &invocation.argv) {
        let remote = "shadow";
        let branch = argv.get(2).map(String::as_str);
        let mirrored = mirror_push(&cwd, remote, branch, false).unwrap_or(false);
        mirror_status = if mirrored {
            "mirrored".to_string()
        } else {
            "mirror_failed".to_string()
        };
        if !mirrored && strict_mode_enabled() {
            final_exit_code = 1;
        }
    }

    if let Some(db) = db {
        let event = GitCommandEvent::from_invocation(
            &invocation,
            before,
            after,
            final_exit_code,
            "ok",
            mirror_status.clone(),
        );
        let _ = store_git_event(db, &event).await?;
    }

    Ok(final_exit_code)
}

pub async fn execute_git_once(argv: &[String]) -> Result<i32> {
    execute_git(None, argv).await
}
