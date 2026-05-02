use serde::{Deserialize, Serialize};

// ── Witness Graph ──────────────────────────────────────────────────────

/// A complete witness graph for a Cargo workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessGraph {
    pub generated_at: String,
    pub workspace_root: String,
    pub crates: Vec<CrateWitness>,
}

/// Per-crate witness data including dual hashes and pub-item inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateWitness {
    /// Crate name (from `Cargo.toml`).
    pub name: String,

    /// SHA-256 of all `pub` item signatures, sorted deterministically.
    pub interface_hash: String,

    /// SHA-256 of all non-pub source content.
    pub implementation_hash: String,

    /// Inventory of public items with their signatures.
    #[serde(default)]
    pub pub_items: Vec<PubItem>,

    /// Direct workspace dependencies.
    #[serde(default)]
    pub direct_deps: Vec<String>,

    /// Reverse workspace dependencies (who depends on this crate).
    #[serde(default)]
    pub reverse_deps: Vec<String>,

    /// Number of source files in `src/`.
    pub file_count: usize,

    /// Total lines of Rust source.
    pub total_lines: usize,
}

/// A single public item extracted from source via `syn`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubItem {
    /// Kind of item: `"fn"`, `"struct"`, `"enum"`, `"trait"`, `"type"`, `"const"`, `"static"`.
    pub kind: String,

    /// Item name.
    pub name: String,

    /// Full signature as a string (name + generics + args + return type).
    pub signature: String,
}

// ── Diff / Impact Plan ─────────────────────────────────────────────────

/// Result of diffing two witness graphs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WitnessDiff {
    pub generated_at: String,
    pub changes: Vec<CrateChange>,
    pub total_crates_changed: usize,
    pub escalation_required: bool,
    pub estimated_test_commands: usize,
}

/// Classification of a change to a single crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateChange {
    pub name: String,
    pub classification: ChangeClassification,
    pub interface_changed: bool,
    pub implementation_changed: bool,
    pub local_commands: Vec<String>,
    pub escalation_commands: Vec<String>,
    pub reason: String,
}

/// Change type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeClassification {
    /// Public interface changed — must escalate to reverse deps.
    InterfaceChanged,
    /// Only internal implementation changed — stay local.
    ImplementationOnly,
    /// No change detected.
    Unchanged,
    /// New crate added.
    Added,
    /// Crate removed.
    Removed,
}

impl std::fmt::Display for ChangeClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InterfaceChanged => write!(f, "interface-changed"),
            Self::ImplementationOnly => write!(f, "implementation-only"),
            Self::Unchanged => write!(f, "unchanged"),
            Self::Added => write!(f, "added"),
            Self::Removed => write!(f, "removed"),
        }
    }
}

// ── Compile Diagnostic Packets ─────────────────────────────────────────

/// Collection of compile diagnostic packets routed to their owning ARCs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilePackets {
    pub generated_at: String,
    pub packets: Vec<CompilePacket>,
    pub summary: CompileSummary,
}

/// A single compile diagnostic routed to its owning ARC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilePacket {
    pub level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub owning_arc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_purpose: Option<String>,
    #[serde(default)]
    pub invariants: Vec<String>,
    #[serde(default)]
    pub local_commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiler_suggestion: Option<String>,
}

/// Summary of compile diagnostic results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileSummary {
    pub total_errors: usize,
    pub total_warnings: usize,
    pub arcs_affected: usize,
}

// ── Repair Bundle ──────────────────────────────────────────────────────

/// A minimal repair bundle — the smallest context an agent needs to fix an issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairBundle {
    pub status: String,
    pub failure_type: String,
    pub primary_arc: String,
    pub primary_file: String,
    pub primary_line: u32,
    pub error_summary: String,
    pub repair_context: RepairContext,
    pub validate_after_fix: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalate_if: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// Context embedded in a repair bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepairContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_purpose: Option<String>,
    #[serde(default)]
    pub invariants: Vec<String>,
    #[serde(default)]
    pub pub_items_in_scope: Vec<String>,
    #[serde(default)]
    pub likely_causes: Vec<String>,
    #[serde(default)]
    pub hints: Vec<String>,
    #[serde(default)]
    pub files_to_read: Vec<String>,
    pub context_bytes: u64,
}
