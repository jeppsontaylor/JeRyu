use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CiProfile {
    pub name: String,
    #[serde(default)]
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceAgentMetadata {
    #[serde(default)]
    pub validation_order: Vec<String>,
    #[serde(default)]
    pub slow_members: Vec<String>,
    #[serde(default)]
    pub shared_contracts: Vec<String>,
    #[serde(default)]
    pub ci_profiles: Vec<CiProfile>,
    #[serde(default)]
    pub instruction_roots: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackageAgentMetadata {
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub owned_paths: Vec<String>,
    #[serde(default)]
    pub entrypoints: Vec<String>,
    #[serde(default)]
    pub invariants: Vec<String>,
    #[serde(default)]
    pub local_validate: Vec<String>,
    #[serde(default)]
    pub boundary_validate: Vec<String>,
    #[serde(default)]
    pub public_api: bool,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub consumers: Vec<String>,
    #[serde(default)]
    pub exceptions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepairHint {
    pub purpose: String,
    pub reason: String,
    #[serde(default)]
    pub common_fixes: Vec<String>,
    pub docs_url: String,
    pub repair_hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCommands {
    pub local: Vec<String>,
    pub boundary: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMember {
    pub name: String,
    pub manifest_path: String,
    pub package_root: String,
    pub direct_dependencies: Vec<String>,
    pub reverse_dependencies: Vec<String>,
    pub public_surfaces: Vec<String>,
    pub risk_tags: Vec<String>,
    pub instruction_locations: Vec<String>,
    pub validation_commands: ValidationCommands,
    pub api_surface_hash: String,
    pub proof_density: f64,
    pub context_roots: Vec<String>,
    pub exception_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMap {
    pub generated_at: String,
    pub workspace_root: String,
    pub validation_order: Vec<String>,
    pub shared_contracts: Vec<String>,
    pub ci_profiles: Vec<CiProfile>,
    pub instruction_roots: Vec<String>,
    pub members: Vec<AgentMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEntry {
    pub arc: String,
    pub source_roots: Vec<String>,
    pub unit_tests: Vec<String>,
    pub doctests: Vec<String>,
    pub integration_harnesses: Vec<String>,
    pub reverse_dependency_tests: Vec<String>,
    pub smoke_tests: Vec<String>,
    pub e2e_gates: Vec<String>,
    pub selection_reason: String,
    pub estimated_cost: String,
    pub required_for_change_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestMap {
    pub generated_at: String,
    pub workspace_root: String,
    pub entries: Vec<TestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedArc {
    pub name: String,
    pub reason: String,
    pub local_validate: Vec<String>,
    pub boundary_validate: Vec<String>,
    pub public_api: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedTest {
    pub arc: String,
    pub command: String,
    pub ring: String,
    pub selection_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VrcPlan {
    pub generated_at: String,
    pub changed_paths: Vec<String>,
    pub selected_arcs: Vec<SelectedArc>,
    pub selected_tests: Vec<SelectedTest>,
    pub stop_condition: String,
    pub skipped_rings: Vec<String>,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationReport {
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportRepairHint {
    pub purpose: String,
    pub reason: String,
    #[serde(default)]
    pub common_fixes: Vec<String>,
    pub docs_url: String,
    pub repair_hint: String,
}
