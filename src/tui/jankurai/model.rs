use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Default)]
pub struct JankuraiSnapshot {
    pub installed: bool,
    pub history: Vec<JankuraiHistoryPoint>,
    pub dimensions: Vec<JankuraiDimension>,
    pub entries: Vec<JankuraiEntry>,
    pub last_scan: Option<JankuraiScan>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JankuraiHistoryPoint {
    pub generated_at: DateTime<Utc>,
    pub score: i64,
    pub raw_score: Option<i64>,
    pub decision: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JankuraiScan {
    pub generated_at: Option<DateTime<Utc>>,
    pub score: i64,
    pub raw_score: i64,
    pub minimum_score: i64,
    pub decision: String,
    pub score_status: String,
    pub finding_count: usize,
    pub hard_findings: usize,
    pub soft_findings: usize,
    pub caps_applied: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct JankuraiDimension {
    pub name: String,
    pub weight: u64,
    pub score: u64,
    pub weighted_points: f64,
    pub evidence: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JankuraiEntryKind {
    Cap,
    Finding,
}

#[derive(Debug, Clone)]
pub struct JankuraiEntry {
    pub kind: JankuraiEntryKind,
    pub label: String,
    pub severity: Option<String>,
    pub hardness: Option<String>,
    pub path: Option<String>,
    pub rule: Option<String>,
    pub lane: Option<String>,
    pub owner: Option<String>,
    pub problem: Option<String>,
    pub evidence: Vec<String>,
    pub suggested_fix: Option<String>,
}
