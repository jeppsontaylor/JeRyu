use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchVariantResult {
    pub scenario: String,
    pub variant: String,
    pub wall_time_ms: u64,
    #[serde(default)]
    pub peak_rss_kb: Option<u64>,
    #[serde(default)]
    pub thread_count_max: Option<u64>,
    #[serde(default)]
    pub throughput: Option<f64>,
    #[serde(default)]
    pub latency_p50_ms: Option<f64>,
    #[serde(default)]
    pub latency_p95_ms: Option<f64>,
    #[serde(default)]
    pub context_files: Option<usize>,
    #[serde(default)]
    pub context_bytes: Option<u64>,
    #[serde(default)]
    pub selected_tests: Option<usize>,
    #[serde(default)]
    pub selected_arcs: Option<usize>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExceptionCaseResult {
    pub case_id: String,
    pub category: String,
    pub failure_mode: String,
    pub expected_signal: String,
    pub observed_signal: String,
    pub success: bool,
    pub wall_time_ms: u64,
    pub docs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioReport {
    pub scenario: String,
    pub generated_at: String,
    pub results: Vec<BenchVariantResult>,
    #[serde(default)]
    pub cases: Vec<ExceptionCaseResult>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionCaseSpec {
    pub case_id: String,
    pub category: String,
    pub failure_mode: String,
    pub expected_signal: String,
    pub docs: Vec<String>,
    pub fix_pattern: String,
    pub benchmarkable: bool,
    pub mode: String,
    pub manifest_path: String,
    #[serde(default)]
    pub cargo_args: Vec<String>,
}
