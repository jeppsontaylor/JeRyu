use super::model::{
    JankuraiDimension, JankuraiEntry, JankuraiEntryKind, JankuraiHistoryPoint, JankuraiScan,
};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;

const DEFAULT_FINDING_LABEL: &str = "UnlabeledFinding";
const DEFAULT_DECISION: &str = "MissingDecision";
const DEFAULT_SCORE_STATUS: &str = "NotReported";

#[derive(Debug, Clone)]
pub(crate) struct ParsedReport {
    pub(crate) scan: JankuraiScan,
    pub(crate) dimensions: Vec<JankuraiDimension>,
    pub(crate) entries: Vec<JankuraiEntry>,
}

pub(crate) fn parse_report(raw: &str) -> Result<ParsedReport, String> {
    let RawRepoScore {
        generated_at,
        score,
        raw_score,
        finding_count,
        hard_findings,
        soft_findings,
        decision,
        conformance_decision,
        dimensions,
        caps_applied,
        findings,
    } = serde_json::from_str(raw).map_err(|err| err.to_string())?;

    let dimensions = dimensions
        .into_iter()
        .map(|dim| JankuraiDimension {
            name: dim.name,
            weight: dim.weight,
            score: dim.score,
            weighted_points: dim.weighted_points,
            evidence: dim.evidence,
            notes: dim.notes,
        })
        .collect::<Vec<_>>();

    let mut entries = Vec::with_capacity(caps_applied.len() + findings.len());
    for cap in &caps_applied {
        entries.push(JankuraiEntry {
            kind: JankuraiEntryKind::Cap,
            label: cap.clone(),
            severity: Some("cap".into()),
            hardness: Some("n/a".into()),
            path: Some("agent/repo-score.json".into()),
            rule: Some(cap.clone()),
            lane: Some("audit".into()),
            owner: Some("agent".into()),
            problem: Some(format!("cap applied: {cap}")),
            evidence: vec!["applied cap recorded in repo score".into()],
            suggested_fix: Some("review the blocking audit rule and rerun the score lane".into()),
        });
    }

    let derived_hard = findings
        .iter()
        .filter(|finding| finding.hardness.as_deref() == Some("hard"))
        .count();
    for finding in findings {
        let rule = match (finding.rule_id.clone(), finding.check_id.clone()) {
            (Some(rule_id), _) => rule_id,
            (None, Some(check_id)) => check_id,
            (None, None) => DEFAULT_FINDING_LABEL.into(),
        };
        entries.push(JankuraiEntry {
            kind: JankuraiEntryKind::Finding,
            label: rule.clone(),
            severity: finding.severity.clone(),
            hardness: finding.hardness.clone(),
            path: Some(finding.path.clone()),
            rule: Some(rule),
            lane: finding.lane.clone(),
            owner: finding.owner.clone(),
            problem: Some(finding.problem.clone()),
            evidence: finding.evidence,
            suggested_fix: finding.agent_fix.clone(),
        });
    }

    let finding_count = match finding_count {
        Some(count) => count,
        None => entries.len().saturating_sub(caps_applied.len()),
    };
    let decision_hard_findings = match &decision {
        Some(decision) => decision.hard_findings,
        None => None,
    };
    let hard_findings = match (hard_findings, decision_hard_findings) {
        (Some(count), _) => count,
        (None, Some(count)) => count,
        (None, None) => derived_hard,
    };
    let decision_soft_findings = match &decision {
        Some(decision) => decision.soft_findings,
        None => None,
    };
    let soft_findings = match (soft_findings, decision_soft_findings) {
        (Some(count), _) => count,
        (None, Some(count)) => count,
        (None, None) => finding_count.saturating_sub(derived_hard),
    };
    let raw_score = match (raw_score, score) {
        (Some(raw_score), _) => raw_score,
        (None, Some(score)) => score,
        (None, None) => 0,
    };
    let score = match score {
        Some(score) => score,
        None => raw_score,
    };
    let minimum_score = match &decision {
        Some(decision) => decision.minimum_score,
        None => 0,
    };
    let decision_label = match (conformance_decision.clone(), &decision) {
        (Some(conformance_decision), _) => conformance_decision,
        (None, Some(decision)) => decision.status.clone(),
        (None, None) => DEFAULT_DECISION.into(),
    };
    let score_status = match &decision {
        Some(decision) => decision.status.clone(),
        None => DEFAULT_SCORE_STATUS.into(),
    };

    Ok(ParsedReport {
        scan: JankuraiScan {
            generated_at: parse_timestamp_value(&generated_at),
            score,
            raw_score,
            minimum_score,
            decision: decision_label,
            score_status,
            finding_count,
            hard_findings,
            soft_findings,
            caps_applied,
        },
        dimensions,
        entries,
    })
}

pub(crate) fn parse_history(raw: &str) -> (Vec<JankuraiHistoryPoint>, Vec<String>) {
    let mut points = Vec::new();
    let mut errors = Vec::new();

    for (line_index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<RawHistoryEntry>(line) {
            Ok(entry) => match parse_timestamp_value(&entry.generated_at) {
                Some(generated_at) => points.push(JankuraiHistoryPoint {
                    generated_at,
                    score: entry.score,
                    raw_score: entry.raw_score,
                    decision: entry.decision,
                }),
                None => errors.push(format!(
                    "history line {} missing a parseable generated_at value",
                    line_index + 1
                )),
            },
            Err(err) => errors.push(format!("history line {}: {}", line_index + 1, err)),
        }
    }

    points.sort_by_key(|point| point.generated_at);
    (points, errors)
}

fn parse_timestamp_value(value: &serde_json::Value) -> Option<DateTime<Utc>> {
    match value {
        serde_json::Value::Number(number) => number.as_i64().and_then(timestamp_from_epoch_secs),
        serde_json::Value::String(raw) => parse_timestamp_text(raw),
        _ => None,
    }
}

fn parse_timestamp_text(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(secs) = raw.parse::<i64>() {
        return timestamp_from_epoch_secs(secs);
    }
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn timestamp_from_epoch_secs(secs: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(secs, 0).single()
}

#[derive(Debug, Deserialize)]
struct RawRepoScore {
    #[serde(default)]
    generated_at: serde_json::Value,
    #[serde(default)]
    score: Option<i64>,
    #[serde(default)]
    raw_score: Option<i64>,
    #[serde(default)]
    finding_count: Option<usize>,
    #[serde(default)]
    hard_findings: Option<usize>,
    #[serde(default)]
    soft_findings: Option<usize>,
    #[serde(default)]
    decision: Option<RawDecision>,
    #[serde(default)]
    conformance_decision: Option<String>,
    #[serde(default)]
    dimensions: Vec<RawDimension>,
    #[serde(default)]
    caps_applied: Vec<String>,
    #[serde(default)]
    findings: Vec<RawFinding>,
}

#[derive(Debug, Deserialize)]
struct RawDecision {
    status: String,
    minimum_score: i64,
    #[serde(default)]
    hard_findings: Option<usize>,
    #[serde(default)]
    soft_findings: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct RawDimension {
    name: String,
    weight: u64,
    score: u64,
    weighted_points: f64,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    notes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawFinding {
    severity: Option<String>,
    hardness: Option<String>,
    path: String,
    problem: String,
    #[serde(default)]
    agent_fix: Option<String>,
    #[serde(default)]
    evidence: Vec<String>,
    #[serde(default)]
    rule_id: Option<String>,
    #[serde(default)]
    check_id: Option<String>,
    #[serde(default)]
    lane: Option<String>,
    #[serde(default)]
    owner: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawHistoryEntry {
    generated_at: serde_json::Value,
    score: i64,
    #[serde(default)]
    raw_score: Option<i64>,
    #[serde(default)]
    decision: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_history_and_sorts_by_generated_time() {
        let raw = r#"{"generated_at":"1778038030","score":88,"raw_score":89}
{"generated_at":"1778038020","score":80,"raw_score":81}
{"generated_at":"1778038040","score":92,"raw_score":92}"#;

        let (history, errors) = parse_history(raw);

        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            history.iter().map(|point| point.score).collect::<Vec<_>>(),
            vec![80, 88, 92]
        );
    }

    #[test]
    fn derives_finding_counts_from_findings_when_counts_are_missing() {
        let raw = r#"{
            "generated_at":"1778038040",
            "score":92,
            "raw_score":92,
            "decision":{"status":"advisory","minimum_score":85,"passed":true},
            "conformance_decision":"block",
            "caps_applied":["cap-a"],
            "dimensions":[],
            "findings":[
                {"severity":"high","hardness":"hard","path":"src/lib.rs","problem":"one","agent_fix":"fix one","evidence":["a"],"rule_id":"rule-a","lane":"fast","owner":"tools"},
                {"severity":"medium","hardness":"soft","path":"src/main.rs","problem":"two","agent_fix":"fix two","evidence":["b"],"rule_id":"rule-b","lane":"contract","owner":"agent"}
            ]
        }"#;

        let parsed = parse_report(raw).expect("report should parse");
        assert_eq!(parsed.scan.finding_count, 2);
        assert_eq!(parsed.scan.hard_findings, 1);
        assert_eq!(parsed.scan.soft_findings, 1);
        assert_eq!(parsed.entries.len(), 3);
    }

    #[test]
    fn tolerates_malformed_jsonl_lines_and_reports_a_non_fatal_error() {
        let raw = r#"{"generated_at":"1778038030","score":88,"raw_score":89}
not-json
{"generated_at":"1778038040","score":92,"raw_score":92}"#;

        let (history, errors) = parse_history(raw);

        assert_eq!(history.len(), 2);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("history line 2"));
    }

    #[test]
    fn report_uses_explicit_default_labels_when_audit_fields_are_absent() {
        let raw = r#"{
            "generated_at":"1778038040",
            "score":72,
            "raw_score":82,
            "dimensions":[],
            "findings":[
                {"severity":"medium","hardness":"soft","path":"src/lib.rs","problem":"missing labels","evidence":[]}
            ]
        }"#;

        let parsed = parse_report(raw).expect("report should parse");
        let finding = parsed
            .entries
            .iter()
            .find(|entry| entry.kind == JankuraiEntryKind::Finding)
            .expect("finding entry");
        assert_eq!(finding.label, DEFAULT_FINDING_LABEL);
        assert_eq!(parsed.scan.decision, DEFAULT_DECISION);
        assert_eq!(parsed.scan.score_status, DEFAULT_SCORE_STATUS);
    }
}
