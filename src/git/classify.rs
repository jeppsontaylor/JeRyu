//! Owner: Git command classification
//! Proof: `cargo test -p jeryu -- git_classify`
//! Invariants: Classification is deterministic and errs toward `Unknown` when the intent is unclear.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitCommandClass {
    ReadOnly,
    WorktreeMutation,
    IndexMutation,
    HistoryMutation,
    RefMutation,
    NetworkRead,
    NetworkWrite,
    Destructive,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitRisk {
    Low,
    Medium,
    High,
    Critical,
}

impl GitCommandClass {
    pub fn risk(self) -> GitRisk {
        match self {
            GitCommandClass::ReadOnly | GitCommandClass::NetworkRead => GitRisk::Low,
            GitCommandClass::IndexMutation | GitCommandClass::WorktreeMutation => GitRisk::Medium,
            GitCommandClass::HistoryMutation | GitCommandClass::NetworkWrite => GitRisk::High,
            GitCommandClass::RefMutation
            | GitCommandClass::Destructive
            | GitCommandClass::Unknown => GitRisk::Critical,
        }
    }
}

pub fn classify_argv(argv: &[String]) -> GitCommandClass {
    match argv.first().map(String::as_str) {
        Some("status" | "diff" | "log" | "show" | "rev-parse" | "ls-files" | "tag") => {
            GitCommandClass::ReadOnly
        }
        Some("fetch" | "ls-remote" | "remote") => GitCommandClass::NetworkRead,
        Some("add" | "restore" | "checkout" | "switch" | "stash" | "revert") => {
            GitCommandClass::WorktreeMutation
        }
        Some("commit" | "merge" | "rebase" | "cherry-pick") => GitCommandClass::HistoryMutation,
        Some("push") => GitCommandClass::NetworkWrite,
        Some("branch") => GitCommandClass::RefMutation,
        Some("reset" | "clean") => GitCommandClass::Destructive,
        Some("rm") => GitCommandClass::IndexMutation,
        Some("init" | "clone" | "submodule") => GitCommandClass::Unknown,
        Some(_) => GitCommandClass::Unknown,
        None => GitCommandClass::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_read_only_commands() {
        assert_eq!(classify_argv(&["status".into()]), GitCommandClass::ReadOnly);
        assert_eq!(
            classify_argv(&["rev-parse".into()]),
            GitCommandClass::ReadOnly
        );
    }

    #[test]
    fn classifies_push_and_reset() {
        assert_eq!(
            classify_argv(&["push".into()]),
            GitCommandClass::NetworkWrite
        );
        assert_eq!(
            classify_argv(&["reset".into()]),
            GitCommandClass::Destructive
        );
    }
}
