//! Owner: Git execution policy
//! Proof: `cargo test -p jeryu -- git_policy`
//! Invariants: Policy defaults are fail-closed for destructive or unknown commands.

use crate::git::classify::GitCommandClass;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitMode {
    Observe,
    AfterSuccess,
    Parallel,
    Strict,
}

impl GitMode {
    pub fn current() -> Self {
        match std::env::var("JERYU_GIT_MODE")
            .unwrap_or_else(|_| "after_success".into())
            .to_ascii_lowercase()
            .as_str()
        {
            "observe" => GitMode::Observe,
            "parallel" => GitMode::Parallel,
            "strict" => GitMode::Strict,
            _ => GitMode::AfterSuccess,
        }
    }
}

pub fn should_mirror(class: GitCommandClass, argv: &[String]) -> bool {
    matches!(
        class,
        GitCommandClass::NetworkWrite | GitCommandClass::RefMutation
    ) && matches!(argv.first().map(String::as_str), Some("push"))
}

pub fn strict_mode_enabled() -> bool {
    matches!(GitMode::current(), GitMode::Strict)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_is_mirrored() {
        assert!(should_mirror(
            GitCommandClass::NetworkWrite,
            &["push".into(), "origin".into()]
        ));
    }
}
