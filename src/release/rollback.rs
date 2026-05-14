//! Owner: Release Pipeline (rollback)
//! Proof: `cargo test -p jeryu -- release::rollback`
//! Invariants: Never re-tags; always writes rollback.json evidence.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackStep {
    pub n: u8,
    pub kind: String,
    pub description: String,
    pub applied: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackReport {
    pub version: String,
    pub reason: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub steps: Vec<RollbackStep>,
    pub final_status: String,
}

/// Default rollback ladder. Mirrors `release.policy.toml [[rollback_step]]`.
pub fn default_ladder() -> Vec<RollbackStep> {
    vec![
        RollbackStep {
            n: 1,
            kind: "feature-flag".into(),
            description: "Disable feature/capability flag. Keep deployed binary.".into(),
            applied: false,
            detail: None,
        },
        RollbackStep {
            n: 2,
            kind: "channel".into(),
            description: "Move stable channel pointer back to previous known-good artifact.".into(),
            applied: false,
            detail: None,
        },
        RollbackStep {
            n: 3,
            kind: "revert-pr".into(),
            description: "Open revert PR through normal merge queue. Publish patch release.".into(),
            applied: false,
            detail: None,
        },
        RollbackStep {
            n: 4,
            kind: "incident".into(),
            description: "Open incident issue, follow runbook in docs/release-policy.md.".into(),
            applied: false,
            detail: None,
        },
    ]
}

/// Build a rollback report. In dry-run mode no filesystem changes occur; the
/// caller still writes the evidence record. In real mode this is where step
/// 1..3 would be applied (currently best-effort scaffolding — channel pointer
/// moves and feature-flag toggles need additional infra to implement safely).
pub fn build_report(version: &str, reason: &str, dry_run: bool) -> RollbackReport {
    let started_at = Utc::now().to_rfc3339();
    let mut steps = default_ladder();
    if dry_run {
        for s in steps.iter_mut() {
            s.detail = Some("dry-run: not applied".into());
        }
    } else {
        // Future: actually toggle the feature flag and move the channel pointer.
        // For now we record the rollback request and mark steps as "scheduled"
        // so the operator can complete them manually with full audit.
        for s in steps.iter_mut() {
            s.detail = Some("scheduled — apply via manual operator step".into());
        }
    }

    RollbackReport {
        version: version.to_string(),
        reason: reason.to_string(),
        started_at: started_at.clone(),
        completed_at: if dry_run { Some(started_at) } else { None },
        steps,
        final_status: if dry_run {
            "dry-run".into()
        } else {
            "scheduled".into()
        },
    }
}

/// Write `rollback.json` into the version's evidence directory. Returns the
/// path that was written.
pub fn write_evidence(report: &RollbackReport, evidence_dir: PathBuf) -> Result<PathBuf> {
    fs::create_dir_all(&evidence_dir)
        .with_context(|| format!("create evidence dir {}", evidence_dir.display()))?;
    let path = evidence_dir.join("rollback.json");
    let body = serde_json::to_string_pretty(report)?;
    fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ladder_has_four_steps_in_order() {
        let ladder = default_ladder();
        assert_eq!(ladder.len(), 4);
        for (i, step) in ladder.iter().enumerate() {
            assert_eq!(step.n as usize, i + 1, "step {} out of order", i);
        }
    }

    #[test]
    fn dry_run_report_has_no_real_apply() {
        let r = build_report("3.0.1-rc.1", "test rollback", true);
        assert_eq!(r.final_status, "dry-run");
        assert!(r.completed_at.is_some());
        for step in &r.steps {
            assert!(!step.applied);
            assert!(step.detail.as_ref().unwrap().contains("dry-run"));
        }
    }

    #[test]
    fn real_report_is_scheduled_not_completed() {
        let r = build_report("3.0.1-rc.1", "test rollback", false);
        assert_eq!(r.final_status, "scheduled");
        assert!(r.completed_at.is_none());
    }

    #[test]
    fn write_evidence_creates_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let report = build_report("3.0.1-rc.1", "reason", true);
        let path = write_evidence(&report, tmp.path().to_path_buf()).expect("write");
        assert!(path.exists());
        let body = std::fs::read_to_string(&path).expect("read");
        assert!(body.contains("3.0.1-rc.1"));
    }
}
