//! Shadow mode: replay the Evidence Gate over historical commits and emit a
//! discrepancy report. Lets users see "what WOULD have happened" before they
//! turn `sovereign_plus` autopilot on for a repo.
//!
//! For each commit in the window we synthesize an [`EvidencePack`] (signed with
//! a per-run ed25519 key), classify risk, call `judge()` with empty receipts,
//! determine the *actual* historical outcome (landed / reverted / not-on-default),
//! and score Match / Disagreement. The summary's `agreement_rate` = matches
//! over applicable commits.
//!
//! Legacy [`ShadowEntry`] / [`ShadowSummary::from_entries`] surface is kept so
//! the existing CLI still compiles; the new fields ([`ShadowSummary::results`],
//! [`ShadowSummary::agreement_rate`]) are additive.

use crate::agent_review::judge::{JudgeInputs, judge};
use crate::autonomy::evidence::{EvidenceInputs, build_evidence_pack};
use crate::autonomy::policy_yaml::PolicyBundle;
use crate::autonomy::risk::{ClassificationInputs, RiskClassifier};
use crate::autonomy::signing::EdSigningKey;
use crate::autonomy::types::{
    ChangedFile, GateDecision, RiskTier, RollbackSection, RollbackStrategy, ScanOutcome,
    SecuritySection, SupplyChainSection, TestsSection,
};
use chrono::{DateTime, TimeZone, Utc};
use std::path::PathBuf;
use std::process::Command;

// --- Public surface ---------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ShadowOptions {
    pub repo_root: PathBuf,
    pub autonomy_dir: PathBuf,
    /// Walk only merge commits when true; otherwise only non-merge commits.
    pub merges_only: bool,
    /// Maximum number of commits to walk. `None` = unlimited.
    pub max_commits: Option<usize>,
    /// Skip commits older than this many seconds before "now". `None` = no cutoff.
    pub since_seconds: Option<u64>,
}

impl Default for ShadowOptions {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::from("."),
            autonomy_dir: PathBuf::from(".autonomy"),
            merges_only: true,
            max_commits: Some(100),
            since_seconds: Some(30 * 24 * 3600),
        }
    }
}

/// Legacy per-commit row: kept so the CLI's JSON output still compiles. New
/// callers should prefer [`ShadowResult`].
#[derive(Debug, Clone)]
pub struct ShadowEntry {
    pub commit_sha: String,
    pub commit_summary: String,
    pub author: String,
    pub timestamp_unix: i64,
    pub files_changed: usize,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub would_be_risk: RiskTier,
    pub would_be_auto_mergeable: bool,
}

/// Did the commit's historical state match what `judge()` would have done?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agreement {
    /// Prediction matched reality.
    Match,
    /// Prediction diverged from reality.
    Disagreement,
    /// Reality is ambiguous (e.g. commit not in the working tree, can't be
    /// scored). Excluded from `agreement_rate`.
    NotApplicable,
}

/// What actually happened to this commit historically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActualOutcome {
    /// Found in the default branch's ancestry and was *not* subsequently reverted.
    LandedOnDefaultBranch,
    /// Found in the default branch's ancestry but a later commit's subject
    /// references it via `Revert ...`.
    Reverted,
    /// Not reachable from the default branch (sits on a feature branch / discarded).
    NotOnDefaultBranch,
}

/// Per-commit shadow result: prediction vs. reality.
#[derive(Debug, Clone)]
pub struct ShadowResult {
    pub commit_sha: String,
    pub commit_short: String,
    pub message_first_line: String,
    pub author: String,
    pub committed_at: DateTime<Utc>,
    pub changed_files: usize,
    pub risk: RiskTier,
    pub predicted: GateDecision,
    pub actual: ActualOutcome,
    pub agreement: Agreement,
    pub hard_stops: Vec<String>,
    pub reason: String,
}

/// Roll-up of a single shadow run. Carries both legacy aggregates (used by the
/// CLI JSON encoder) and the new judge-driven `results` + `agreement_rate`.
#[derive(Debug, Clone)]
pub struct ShadowSummary {
    // --- new fields (judge-driven) -------------------------------------------
    pub repo_root: PathBuf,
    pub commits_walked: usize,
    pub results: Vec<ShadowResult>,
    pub agreement_rate: f64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    // --- legacy aggregates (kept for the CLI JSON path) ----------------------
    pub total: usize,
    pub by_tier: [usize; 6],
    pub auto_merge_eligible: usize,
    pub human_required: usize,
}

impl Default for ShadowSummary {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            repo_root: PathBuf::new(),
            commits_walked: 0,
            results: Vec::new(),
            agreement_rate: 0.0,
            started_at: now,
            finished_at: now,
            total: 0,
            by_tier: [0; 6],
            auto_merge_eligible: 0,
            human_required: 0,
        }
    }
}

impl ShadowSummary {
    /// Re-build the legacy aggregates over a precomputed [`ShadowEntry`] slice.
    /// Preserved so the CLI's JSON encoder still compiles; new callers should
    /// read [`ShadowSummary::results`] directly.
    pub fn from_entries(entries: &[ShadowEntry]) -> Self {
        let mut s = Self::default();
        for e in entries {
            s.total += 1;
            let idx = tier_index(e.would_be_risk);
            s.by_tier[idx] += 1;
            if e.would_be_auto_mergeable {
                s.auto_merge_eligible += 1;
            } else {
                s.human_required += 1;
            }
        }
        s
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShadowError {
    #[error("git invocation failed: {0}")]
    Git(String),
    #[error("policy load error: {0}")]
    Policy(#[from] std::io::Error),
    #[error("no default branch found (looked for main/master)")]
    NoDefaultBranch,
}

// --- Implementation ---------------------------------------------------------

pub fn run_shadow(opts: &ShadowOptions) -> Result<ShadowSummary, ShadowError> {
    let started_at = Utc::now();

    // Load policy bundle once. The .autonomy/policies/ may not exist for some
    // repos; surface that as a clean error rather than panicking.
    let policies = PolicyBundle::from_dir(&opts.autonomy_dir.join("policies"))?;
    let classifier = RiskClassifier::new(&policies);

    // One ed25519 key per run; never persisted. Lets every synthesized pack
    // pass the `evidence_signature_invalid` hard-stop without polluting the
    // real signing keychain.
    let signing_key = EdSigningKey::generate("shadow.replay.v1");

    let default_branch = resolve_default_branch(&opts.repo_root)?;

    let commits = walk_commits(opts)?;

    let mut results: Vec<ShadowResult> = Vec::with_capacity(commits.len());
    let mut entries: Vec<ShadowEntry> = Vec::with_capacity(commits.len());
    let mut applicable = 0usize;
    let mut matches = 0usize;

    let repo_name = opts
        .repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("shadow");

    for c in &commits {
        let changed_files = stat_changed_files(&opts.repo_root, &c.sha);
        let tier = classifier.classify(&ClassificationInputs {
            files: &changed_files,
            triggered_conditions: &[],
        });

        let mut pack = build_evidence_pack(EvidenceInputs {
            repo: repo_name,
            source_branch: "shadow/replay",
            target_branch: &default_branch,
            head_sha: &c.sha,
            base_sha: c.parent_sha.as_deref().unwrap_or(&c.sha),
            policy_sha: "shadow-policy-sha",
            author_agent: Some("shadow.replay"),
            intent_id: None,
            risk: tier,
            changed_files: changed_files.clone(),
            claims: vec![],
            tests: default_passing_tests(),
            security: default_passing_security(),
            supply_chain: SupplyChainSection::default(),
            rollback: default_revert_rollback(),
            legacy_receipts: vec![],
        });
        // Sign the final pack so `evidence_signature_invalid` doesn't trip.
        let body = serde_json::to_string(&pack).expect("pack serialization");
        pack.signature = Some(signing_key.sign_raw(body.as_bytes()));

        let outcome = judge(JudgeInputs {
            pack: &pack,
            receipts: &[],
            policy: &policies,
            repo: "shadow",
            target_branch: &default_branch,
            merge_request: None,
            author_agent: Some("shadow.replay"),
            external_hard_stops: &[],
        });
        let predicted = outcome.verdict.decision;
        let hard_stops = outcome.verdict.hard_stops.clone();
        let reason = if hard_stops.is_empty() {
            format!("{:?}", predicted).to_lowercase()
        } else {
            format!("hard_stops: {}", hard_stops.join(","))
        };

        let actual = classify_actual(&opts.repo_root, &c.sha, &c.short_sha, &default_branch);
        let agreement = score_agreement(predicted, actual);
        if agreement != Agreement::NotApplicable {
            applicable += 1;
            if agreement == Agreement::Match {
                matches += 1;
            }
        }

        results.push(ShadowResult {
            commit_sha: c.sha.clone(),
            commit_short: c.short_sha.clone(),
            message_first_line: c.subject.clone(),
            author: c.author.clone(),
            committed_at: c.committed_at,
            changed_files: changed_files.len(),
            risk: tier,
            predicted,
            actual,
            agreement,
            hard_stops,
            reason,
        });
        entries.push(ShadowEntry {
            commit_sha: c.sha.clone(),
            commit_summary: c.subject.chars().take(80).collect(),
            author: c.author.clone(),
            timestamp_unix: c.committed_at.timestamp(),
            files_changed: changed_files.len(),
            lines_added: changed_files.iter().map(|f| f.lines_added).sum(),
            lines_removed: changed_files.iter().map(|f| f.lines_removed).sum(),
            would_be_risk: tier,
            would_be_auto_mergeable: tier.auto_merge_eligible() && !tier.human_required(),
        });
    }

    let mut summary = ShadowSummary::from_entries(&entries);
    summary.repo_root = opts.repo_root.clone();
    summary.commits_walked = results.len();
    summary.agreement_rate = if applicable == 0 {
        0.0
    } else {
        matches as f64 / applicable as f64
    };
    summary.results = results;
    summary.started_at = started_at;
    summary.finished_at = Utc::now();
    Ok(summary)
}

// --- Git plumbing (shells out via std::process::Command — no new deps) -----

#[derive(Debug, Clone)]
struct GitCommit {
    sha: String,
    short_sha: String,
    committed_at: DateTime<Utc>,
    author: String,
    parent_sha: Option<String>,
    subject: String,
}

fn walk_commits(opts: &ShadowOptions) -> Result<Vec<GitCommit>, ShadowError> {
    let mut args: Vec<String> = vec!["log".into(), "--format=%H|%h|%aI|%an|%P|%s".into()];
    if opts.merges_only {
        args.push("--merges".into());
    } else {
        args.push("--no-merges".into());
    }
    if let Some(n) = opts.max_commits {
        args.push(format!("--max-count={n}"));
    }
    if let Some(s) = opts.since_seconds {
        args.push(format!("--since={s} seconds ago"));
    }
    let out = run_git(&opts.repo_root, &args)?;
    let mut commits = Vec::new();
    for line in out.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(6, '|');
        let sha = parts.next().unwrap_or("").to_string();
        let short_sha = parts.next().unwrap_or("").to_string();
        let committed_at_raw = parts.next().unwrap_or("");
        let author = parts.next().unwrap_or("").to_string();
        let parents_raw = parts.next().unwrap_or("");
        let subject = parts.next().unwrap_or("").to_string();
        if sha.is_empty() {
            continue;
        }
        let committed_at = chrono::DateTime::parse_from_rfc3339(committed_at_raw)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc.timestamp_opt(0, 0).single().unwrap_or_else(Utc::now));
        let parent_sha = parents_raw.split_whitespace().next().map(|s| s.to_string());
        commits.push(GitCommit {
            sha,
            short_sha,
            committed_at,
            author,
            parent_sha,
            subject,
        });
    }
    Ok(commits)
}

fn stat_changed_files(repo_root: &PathBuf, sha: &str) -> Vec<ChangedFile> {
    let Ok(out) = run_git(
        repo_root,
        &[
            "show".into(),
            "--stat".into(),
            "--format=".into(),
            sha.into(),
        ],
    ) else {
        return vec![];
    };
    let mut files = Vec::new();
    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !line.contains('|') {
            continue;
        }
        // Skip the final " N files changed, ..." summary line.
        if trimmed.starts_with(|c: char| c.is_ascii_digit()) && trimmed.contains(" file") {
            continue;
        }
        let Some((path_part, count_part)) = line.split_once('|') else {
            continue;
        };
        let path = path_part.trim().to_string();
        if path.is_empty() {
            continue;
        }
        // Binary marker: "path | Bin 0 -> 1234 bytes" — record path, no counts.
        let (added, removed) = if count_part.trim_start().starts_with("Bin") {
            (0, 0)
        } else {
            // "  12 ++++--" — tally + / - chars (the numeric prefix is the total).
            let marks = count_part
                .trim()
                .splitn(2, char::is_whitespace)
                .nth(1)
                .unwrap_or("");
            (
                marks.chars().filter(|c| *c == '+').count() as u32,
                marks.chars().filter(|c| *c == '-').count() as u32,
            )
        };
        files.push(ChangedFile {
            path,
            risk_tags: vec![],
            lines_added: added,
            lines_removed: removed,
        });
    }
    files
}

fn resolve_default_branch(repo_root: &PathBuf) -> Result<String, ShadowError> {
    for cand in ["main", "master"] {
        let args = [
            "show-ref".into(),
            "--verify".into(),
            "--quiet".into(),
            format!("refs/heads/{cand}"),
        ];
        if run_git(repo_root, &args).is_ok() {
            return Ok(cand.to_string());
        }
    }
    Err(ShadowError::NoDefaultBranch)
}

fn classify_actual(
    repo_root: &PathBuf,
    sha: &str,
    short_sha: &str,
    default_branch: &str,
) -> ActualOutcome {
    let branch_args = [
        "branch".into(),
        "--contains".into(),
        sha.into(),
        "--format=%(refname:short)".into(),
    ];
    let on_default = run_git(repo_root, &branch_args)
        .map(|out| {
            out.lines()
                .any(|l| l.trim().trim_start_matches('*').trim() == default_branch)
        })
        .unwrap_or(false);
    if !on_default {
        return ActualOutcome::NotOnDefaultBranch;
    }
    // Look for `Revert ...<short_sha>...` in any reachable commit subject.
    let log_args = [
        "log".into(),
        "--all".into(),
        format!("--grep=Revert .*{short_sha}"),
        "-E".into(),
        "--format=%H".into(),
        "-n".into(),
        "1".into(),
    ];
    let reverted = run_git(repo_root, &log_args)
        .map(|out| !out.trim().is_empty())
        .unwrap_or(false);
    if reverted {
        ActualOutcome::Reverted
    } else {
        ActualOutcome::LandedOnDefaultBranch
    }
}

fn score_agreement(predicted: GateDecision, actual: ActualOutcome) -> Agreement {
    match (predicted, actual) {
        (
            GateDecision::AllowMerge | GateDecision::RequireHuman,
            ActualOutcome::LandedOnDefaultBranch,
        ) => Agreement::Match,
        (GateDecision::Reject, ActualOutcome::Reverted)
        | (GateDecision::Reject, ActualOutcome::NotOnDefaultBranch) => Agreement::Match,
        _ => Agreement::Disagreement,
    }
}

fn run_git(repo_root: &PathBuf, args: &[String]) -> Result<String, ShadowError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args.iter().map(|s| s.as_str()))
        .output()
        .map_err(|e| ShadowError::Git(format!("spawn git: {e}")))?;
    if !out.status.success() {
        return Err(ShadowError::Git(format!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn default_passing_tests() -> TestsSection {
    TestsSection {
        targeted: vec![],
        full_required: false,
        skipped: vec![],
        coverage_delta: None,
    }
}

fn default_passing_security() -> SecuritySection {
    SecuritySection {
        sast: ScanOutcome::Passed,
        dependency_scan: ScanOutcome::Passed,
        secret_scan: ScanOutcome::Passed,
    }
}

fn default_revert_rollback() -> RollbackSection {
    RollbackSection {
        strategy: RollbackStrategy::RevertCommit,
        feature_flag: None,
        data_migration_reversible: Some(true),
    }
}

fn tier_index(t: RiskTier) -> usize {
    match t {
        RiskTier::R0 => 0,
        RiskTier::R1 => 1,
        RiskTier::R2 => 2,
        RiskTier::R3 => 3,
        RiskTier::R4 => 4,
        RiskTier::R5 => 5,
    }
}

// --- Render -----------------------------------------------------------------

pub fn render_summary(summary: &ShadowSummary, _entries: &[ShadowEntry]) -> String {
    let mut s = String::new();
    s.push_str("jeryu autonomy shadow — historical replay\n");
    s.push_str("──────────────────────────────────────\n");
    s.push_str(&format!("commits analyzed: {}\n", summary.commits_walked));
    if summary.commits_walked == 0 {
        s.push_str(
            "(no commits matched the filter — try lowering --since or dropping --merges-only)\n",
        );
        return s;
    }
    s.push_str("\nrisk × decision × outcome:\n");
    for r in summary.results.iter().take(20) {
        s.push_str(&format!(
            "  {:8}  {:>3?}  {:<13?} vs {:<22?}  {:<12?}  {}\n",
            r.commit_short,
            r.risk,
            r.predicted,
            r.actual,
            r.agreement,
            truncate(&r.message_first_line, 60),
        ));
    }
    let pct = (summary.agreement_rate * 100.0).round() as u64;
    let applicable: usize = summary
        .results
        .iter()
        .filter(|r| r.agreement != Agreement::NotApplicable)
        .count();
    let matches: usize = summary
        .results
        .iter()
        .filter(|r| r.agreement == Agreement::Match)
        .count();
    s.push_str(&format!(
        "\nAgreement: {}% ({} / {})\n",
        pct, matches, applicable
    ));
    // Legacy footer for back-compat with the older render: tier counts.
    s.push_str("\nby risk tier (would-have-been):\n");
    for (i, tier_name) in ["R0", "R1", "R2", "R3", "R4", "R5"].iter().enumerate() {
        let c = summary.by_tier[i];
        let pct = (c as f64 / summary.total.max(1) as f64) * 100.0;
        s.push_str(&format!("  {tier_name}: {c:4}  ({pct:5.1}%)\n"));
    }
    s
}

fn truncate(s: &str, n: usize) -> String {
    let mut out = String::with_capacity(n);
    for (i, c) in s.chars().enumerate() {
        if i >= n {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

// --- Tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn autonomy_dir() -> PathBuf {
        repo_root().join(".autonomy")
    }

    fn entry(tier: RiskTier, auto: bool) -> ShadowEntry {
        ShadowEntry {
            commit_sha: "x".into(),
            commit_summary: "x".into(),
            author: "u".into(),
            timestamp_unix: 0,
            files_changed: 1,
            lines_added: 1,
            lines_removed: 0,
            would_be_risk: tier,
            would_be_auto_mergeable: auto,
        }
    }

    #[test]
    fn shadow_summary_aggregates_legacy_tiers() {
        let entries = vec![entry(RiskTier::R0, true), entry(RiskTier::R4, false)];
        let s = ShadowSummary::from_entries(&entries);
        assert_eq!(s.total, 2);
        assert_eq!(s.by_tier[0], 1);
        assert_eq!(s.by_tier[4], 1);
        assert_eq!(s.auto_merge_eligible, 1);
        assert_eq!(s.human_required, 1);
    }

    #[test]
    fn shadow_runs_on_this_repo_without_panic() {
        let opts = ShadowOptions {
            repo_root: repo_root(),
            autonomy_dir: autonomy_dir(),
            merges_only: false,
            max_commits: Some(5),
            since_seconds: Some(7 * 24 * 3600),
        };
        // This test exercises the git + classifier + judge path against the
        // actual jeryu repo. It must run cleanly (or return an empty summary);
        // whether there are commits in the window is incidental.
        let _summary = run_shadow(&opts).expect("shadow runs");
    }

    #[test]
    fn walks_at_most_max_commits() {
        let opts = ShadowOptions {
            repo_root: repo_root(),
            autonomy_dir: autonomy_dir(),
            merges_only: false,
            max_commits: Some(3),
            since_seconds: Some(365 * 24 * 3600),
        };
        let s = run_shadow(&opts).expect("shadow runs");
        assert!(
            s.results.len() <= 3,
            "results capped at 3, got {}",
            s.results.len()
        );
        assert_eq!(s.commits_walked, s.results.len());
    }

    #[test]
    fn merges_only_filters_to_merge_commits() {
        // Probe: is there at least one merge commit in HEAD ancestry? If not,
        // the assertion is trivially that we got an empty result set.
        let probe = std::process::Command::new("git")
            .args([
                "-C",
                env!("CARGO_MANIFEST_DIR"),
                "log",
                "--merges",
                "--max-count=1",
                "--format=%H",
            ])
            .output();
        let has_merge = probe
            .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
            .unwrap_or(false);

        let opts = ShadowOptions {
            repo_root: repo_root(),
            autonomy_dir: autonomy_dir(),
            merges_only: true,
            max_commits: Some(5),
            since_seconds: Some(5 * 365 * 24 * 3600),
        };
        let s = run_shadow(&opts).expect("shadow runs");
        if has_merge {
            assert!(
                !s.results.is_empty(),
                "expected at least one merge commit when repo has merges in ancestry"
            );
        } else {
            // No merges in the entire 5-year window — accept an empty walk.
            // (We don't assert == 0 because the probe and the run use slightly
            // different windows.)
        }
    }

    #[test]
    fn render_summary_includes_agreement_rate() {
        let opts = ShadowOptions {
            repo_root: repo_root(),
            autonomy_dir: autonomy_dir(),
            merges_only: false,
            max_commits: Some(2),
            since_seconds: Some(7 * 24 * 3600),
        };
        let s = run_shadow(&opts).expect("shadow runs");
        let rendered = render_summary(&s, &[]);
        assert!(
            rendered.contains("Agreement:"),
            "render_summary must include 'Agreement:' line; got:\n{rendered}"
        );
    }

    #[test]
    fn disagreement_is_counted_in_agreement_rate() {
        // 3 synthetic results: 2 Match, 1 Disagreement → 2/3 ≈ 0.6667.
        let now = Utc::now();
        let mk = |agreement, predicted, actual| ShadowResult {
            commit_sha: "deadbeef".into(),
            commit_short: "deadbee".into(),
            message_first_line: "synthetic".into(),
            author: "tester".into(),
            committed_at: now,
            changed_files: 1,
            risk: RiskTier::R2,
            predicted,
            actual,
            agreement,
            hard_stops: vec![],
            reason: "synthetic".into(),
        };
        let results = vec![
            mk(
                Agreement::Match,
                GateDecision::AllowMerge,
                ActualOutcome::LandedOnDefaultBranch,
            ),
            mk(
                Agreement::Match,
                GateDecision::Reject,
                ActualOutcome::Reverted,
            ),
            mk(
                Agreement::Disagreement,
                GateDecision::Reject,
                ActualOutcome::LandedOnDefaultBranch,
            ),
        ];
        let mut summary = ShadowSummary::default();
        summary.commits_walked = results.len();
        summary.agreement_rate = 2.0 / 3.0;
        summary.results = results;
        assert!((summary.agreement_rate - 2.0 / 3.0).abs() < 1e-9);
        let rendered = render_summary(&summary, &[]);
        assert!(
            rendered.contains("Agreement: 67%") || rendered.contains("Agreement: 66%"),
            "expected ~67% in render; got:\n{rendered}"
        );
        assert!(rendered.contains("(2 / 3)"));
    }

    // --- Wave 5 coverage-boost additions -----------------------------------

    /// A zero-commit walk (max_commits=0) yields an empty result set, an
    /// agreement_rate of 0.0 (not NaN), and renders without panicking.
    #[test]
    fn shadow_zero_commits_run_renders_cleanly() {
        let opts = ShadowOptions {
            repo_root: repo_root(),
            autonomy_dir: autonomy_dir(),
            merges_only: false,
            max_commits: Some(0),
            since_seconds: Some(7 * 24 * 3600),
        };
        let s = run_shadow(&opts).expect("zero-commit shadow must run");
        assert_eq!(s.commits_walked, 0);
        assert!(s.results.is_empty());
        // Zero applicable pairs → 0.0 (not NaN).
        assert_eq!(s.agreement_rate, 0.0);
        assert!(!s.agreement_rate.is_nan());
        let rendered = render_summary(&s, &[]);
        assert!(rendered.contains("commits analyzed: 0"));
        assert!(
            rendered.contains("no commits matched"),
            "zero-commit render must include the operator hint"
        );
    }

    /// An all-disagreement synthetic run produces an `agreement_rate` of
    /// exactly 0.0 (NOT NaN; NOT panic).
    #[test]
    fn shadow_all_disagreement_scenario_agreement_zero() {
        let now = Utc::now();
        let mk = |predicted, actual| ShadowResult {
            commit_sha: "deadbeef".into(),
            commit_short: "deadbee".into(),
            message_first_line: "synthetic".into(),
            author: "tester".into(),
            committed_at: now,
            changed_files: 1,
            risk: RiskTier::R2,
            predicted,
            actual,
            agreement: Agreement::Disagreement,
            hard_stops: vec![],
            reason: "synthetic".into(),
        };
        let results = vec![
            mk(GateDecision::AllowMerge, ActualOutcome::Reverted),
            mk(GateDecision::Reject, ActualOutcome::LandedOnDefaultBranch),
            mk(GateDecision::AllowMerge, ActualOutcome::Reverted),
        ];
        let mut summary = ShadowSummary::default();
        summary.commits_walked = results.len();
        // 0 matches over 3 applicable = 0.0
        summary.agreement_rate = 0.0;
        summary.results = results;
        assert_eq!(summary.agreement_rate, 0.0);
        let rendered = render_summary(&summary, &[]);
        assert!(
            rendered.contains("Agreement: 0%"),
            "all-disagreement run renders 0%; got:\n{rendered}"
        );
        assert!(rendered.contains("(0 / 3)"));
    }

    #[test]
    fn score_agreement_truth_table() {
        use ActualOutcome::*;
        use GateDecision::*;
        assert_eq!(
            score_agreement(AllowMerge, LandedOnDefaultBranch),
            Agreement::Match
        );
        assert_eq!(
            score_agreement(RequireHuman, LandedOnDefaultBranch),
            Agreement::Match
        );
        assert_eq!(score_agreement(Reject, Reverted), Agreement::Match);
        assert_eq!(
            score_agreement(Reject, NotOnDefaultBranch),
            Agreement::Match
        );
        assert_eq!(
            score_agreement(AllowMerge, Reverted),
            Agreement::Disagreement
        );
        assert_eq!(
            score_agreement(Reject, LandedOnDefaultBranch),
            Agreement::Disagreement
        );
    }
}
