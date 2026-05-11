//! Owner: Git event record model
//! Proof: `cargo test -p jeryu -- git_event`
//! Invariants: Events are append-only and contain only redacted command material.

use crate::git::{classify::GitRisk, invocation::GitInvocation, snapshot::GitSnapshot};
use crate::redact::{hash_argv, redact_args};
use chrono::Utc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommandEvent {
    pub request_id: String,
    pub actor: String,
    pub cwd: String,
    pub repo_root: Option<String>,
    pub argv_redacted: Vec<String>,
    pub argv_hash: String,
    pub class: String,
    pub risk: GitRisk,
    pub mode: String,
    pub before: GitSnapshot,
    pub after: Option<GitSnapshot>,
    pub exit_code: i32,
    pub sidecar_status: String,
    pub mirror_status: String,
    pub created_at: String,
}

impl GitCommandEvent {
    pub fn from_invocation(
        invocation: &GitInvocation,
        before: GitSnapshot,
        after: Option<GitSnapshot>,
        exit_code: i32,
        sidecar_status: impl Into<String>,
        mirror_status: impl Into<String>,
    ) -> Self {
        Self {
            request_id: invocation.request_id.clone(),
            actor: invocation.actor.clone(),
            cwd: invocation.cwd.display().to_string(),
            repo_root: before.repo_root.clone(),
            argv_redacted: redact_args(&invocation.argv),
            argv_hash: hash_argv(&invocation.argv),
            class: format!("{:?}", invocation.class),
            risk: invocation.risk,
            mode: format!("{:?}", invocation.mode),
            before,
            after,
            exit_code,
            sidecar_status: sidecar_status.into(),
            mirror_status: mirror_status.into(),
            created_at: Utc::now().to_rfc3339(),
        }
    }
}
