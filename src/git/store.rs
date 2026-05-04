//! Owner: Git event persistence
//! Proof: `cargo test -p jeryu -- git_store`
//! Invariants: All SQL remains centralized in `state::Db` methods.

use anyhow::Result;

use crate::git::event::GitCommandEvent;
use crate::state::Db;

pub async fn store_git_event(db: &Db, event: &GitCommandEvent) -> Result<i64> {
    db.record_git_command_event(event).await
}
