//! Owner: Interactive TUI subsystem — workflow snapshot collector
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::collector`
//! Invariants: Collector assembles WorkflowSnapshot from available sources; never mutates them.

use super::builder;
use super::model::*;

/// Assemble the best available WorkflowSnapshot from current state.
///
/// Priority order:
/// 1. If there is a live pipeline with VTI overlay → LivePipeline
/// 2. If there is a persisted VTI plan → LatestDbPlan
/// 3. If there is a current diff → CurrentDiff
/// 4. Otherwise → Demo
///
/// This function is intentionally synchronous and pure —
/// it takes pre-fetched data and builds the snapshot.
pub fn collect_snapshot(
    vti_plan: Option<&VtiPlanData>,
    _live_jobs: Option<&[LiveJobData]>,
) -> WorkflowSnapshot {
    match vti_plan {
        Some(plan) => build_from_vti_plan(plan),
        None => builder::build_demo_snapshot(),
    }
}

/// Pre-fetched VTI plan data for the collector.
#[derive(Debug, Clone)]
pub struct VtiPlanData {
    pub mode: String,
    pub confidence: f64,
    pub ref_name: String,
    pub selected_tests: Vec<VtiTestItem>,
    pub skipped_tests: Vec<VtiTestItem>,
    pub sentinel_tests: Vec<String>,
}

/// A single test from the VTI plan.
#[derive(Debug, Clone)]
pub struct VtiTestItem {
    pub id: String,
    pub label: String,
    pub command: String,
    pub subsystem: String,
    pub deps: Vec<String>,
}

/// Pre-fetched live GitLab job data for status overlay.
#[derive(Debug, Clone)]
pub struct LiveJobData {
    pub job_id: i64,
    pub name: String,
    pub status: String,
    pub stage: String,
    pub duration: Option<f64>,
    pub started_at: Option<String>,
}

fn build_from_vti_plan(plan: &VtiPlanData) -> WorkflowSnapshot {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Phase 0: mandatory pre-flight checks.
    let check_node = WorkflowNode {
        id: "cargo-check".into(),
        label: "cargo check".into(),
        command: Some("cargo check -p jeryu".into()),
        kind: WorkflowNodeKind::Check,
        status: WorkflowStatus::Waiting,
        required: true,
        ..Default::default()
    };
    nodes.push(check_node);

    let vti_node = WorkflowNode {
        id: "vti-plan".into(),
        label: "VTI plan".into(),
        command: Some("jeryu test select".into()),
        kind: WorkflowNodeKind::VtiPlan,
        status: WorkflowStatus::Ran,
        required: true,
        deps: vec!["cargo-check".into()],
        ..Default::default()
    };
    edges.push(WorkflowEdge {
        from: "cargo-check".into(),
        to: "vti-plan".into(),
        kind: WorkflowEdgeKind::Dependency,
    });
    nodes.push(vti_node);

    // Phase 1+: selected tests depend on check + vti-plan.
    for test in &plan.selected_tests {
        let node = WorkflowNode {
            id: test.id.clone(),
            label: test.label.clone(),
            command: Some(test.command.clone()),
            kind: WorkflowNodeKind::UnitTest,
            status: WorkflowStatus::Waiting,
            required: true,
            deps: {
                let mut d = vec!["cargo-check".into(), "vti-plan".into()];
                d.extend(test.deps.iter().cloned());
                d
            },
            ..Default::default()
        };
        edges.push(WorkflowEdge {
            from: "cargo-check".into(),
            to: test.id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
        edges.push(WorkflowEdge {
            from: "vti-plan".into(),
            to: test.id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
        nodes.push(node);
    }

    // Sentinel tests (always-run safety net).
    for sentinel_id in &plan.sentinel_tests {
        let node = WorkflowNode {
            id: sentinel_id.clone(),
            label: format!("sentinel: {}", sentinel_id),
            kind: WorkflowNodeKind::Sentinel,
            status: WorkflowStatus::Waiting,
            required: true,
            deps: vec!["cargo-check".into()],
            ..Default::default()
        };
        edges.push(WorkflowEdge {
            from: "cargo-check".into(),
            to: sentinel_id.clone(),
            kind: WorkflowEdgeKind::Dependency,
        });
        nodes.push(node);
    }

    let title = format!("VTI plan: {} ({} selected)", plan.ref_name, plan.selected_tests.len());
    builder::build_snapshot(
        nodes,
        edges,
        &title,
        &plan.mode,
        plan.confidence,
        WorkflowSource::LatestDbPlan,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_plan_returns_demo() {
        let snap = collect_snapshot(None, None);
        assert_eq!(snap.source, WorkflowSource::Demo);
    }

    #[test]
    fn vti_plan_produces_correct_phases() {
        let plan = VtiPlanData {
            mode: "selected".into(),
            confidence: 0.91,
            ref_name: "feature/tui-workflow".into(),
            selected_tests: vec![
                VtiTestItem {
                    id: "unit-tui".into(),
                    label: "unit: tui".into(),
                    command: "cargo nextest run -- tui".into(),
                    subsystem: "tui".into(),
                    deps: vec![],
                },
                VtiTestItem {
                    id: "unit-api".into(),
                    label: "unit: api".into(),
                    command: "cargo nextest run -- api".into(),
                    subsystem: "api".into(),
                    deps: vec![],
                },
            ],
            skipped_tests: vec![],
            sentinel_tests: vec!["smoke".into()],
        };

        let snap = collect_snapshot(Some(&plan), None);
        assert_eq!(snap.source, WorkflowSource::LatestDbPlan);
        assert_eq!(snap.mode, "selected");
        assert!((snap.confidence - 0.91).abs() < 0.01);

        // Should have: cargo-check, vti-plan, unit-tui, unit-api, smoke = 5 nodes
        assert_eq!(snap.nodes.len(), 5);

        // Phase 0 should be cargo-check (only node with no deps)
        assert!(snap.phases[0].node_ids.contains(&"cargo-check".to_string()));
    }
}
