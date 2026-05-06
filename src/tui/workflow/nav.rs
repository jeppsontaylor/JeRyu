//! Owner: Interactive TUI subsystem — workflow DAG spatial navigation
//! Proof: `cargo nextest run -p jeryu -- tui::workflow::nav`
//! Invariants: Navigation is pure state computation; never mutates workflow data.

use super::model::WorkflowSnapshot;

/// Navigation state for the workflow tab.
#[derive(Debug, Clone, Default)]
pub struct WorkflowNav {
    /// Currently selected phase index.
    pub phase_idx: usize,
    /// Currently selected node index within the phase.
    pub node_idx: usize,
}

impl WorkflowNav {
    /// Move to the next phase (down).
    pub fn down(&mut self, snap: &WorkflowSnapshot) {
        if self.phase_idx + 1 < snap.phases.len() {
            self.phase_idx += 1;
            self.clamp_node(snap);
        }
    }

    /// Move to the previous phase (up).
    pub fn up(&mut self, snap: &WorkflowSnapshot) {
        if self.phase_idx > 0 {
            self.phase_idx -= 1;
            self.clamp_node(snap);
        }
    }

    /// Move to the next sibling node (right).
    pub fn right(&mut self, snap: &WorkflowSnapshot) {
        if let Some(phase) = snap.phases.get(self.phase_idx) {
            if self.node_idx + 1 < phase.node_ids.len() {
                self.node_idx += 1;
            }
        }
    }

    /// Move to the previous sibling node (left).
    pub fn left(&mut self, _snap: &WorkflowSnapshot) {
        if self.node_idx > 0 {
            self.node_idx -= 1;
        }
    }

    /// Get the currently selected node ID.
    pub fn selected_node_id<'a>(&self, snap: &'a WorkflowSnapshot) -> Option<&'a str> {
        snap.phases
            .get(self.phase_idx)
            .and_then(|p| p.node_ids.get(self.node_idx))
            .map(|s| s.as_str())
    }

    /// Ensure node_idx is within bounds for the current phase.
    fn clamp_node(&mut self, snap: &WorkflowSnapshot) {
        if let Some(phase) = snap.phases.get(self.phase_idx) {
            if self.node_idx >= phase.node_ids.len() {
                self.node_idx = phase.node_ids.len().saturating_sub(1);
            }
        } else {
            self.node_idx = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::workflow::builder::build_demo_snapshot;

    #[test]
    fn navigation_basics() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();

        // Start at phase 0, node 0.
        assert_eq!(nav.phase_idx, 0);
        assert_eq!(nav.node_idx, 0);

        // Move right within phase 0 (has 3 nodes: check, fmt, clippy).
        nav.right(&snap);
        assert_eq!(nav.node_idx, 1);
        nav.right(&snap);
        assert_eq!(nav.node_idx, 2);

        // Can't go past last node.
        nav.right(&snap);
        assert_eq!(nav.node_idx, 2);

        // Move down to phase 1.
        nav.down(&snap);
        assert_eq!(nav.phase_idx, 1);
        // node_idx should clamp if phase 1 has fewer nodes.
        assert!(nav.node_idx <= snap.phases[1].node_ids.len());
    }

    #[test]
    fn up_at_top_stays() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        nav.up(&snap);
        assert_eq!(nav.phase_idx, 0);
    }

    #[test]
    fn left_at_zero_stays() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        nav.left(&snap);
        assert_eq!(nav.node_idx, 0);
    }

    #[test]
    fn selected_node_id() {
        let snap = build_demo_snapshot();
        let nav = WorkflowNav::default();
        let id = nav.selected_node_id(&snap);
        assert!(id.is_some());
        // The first node in phase 0 should be one of check/fmt/clippy.
        let id = id.unwrap();
        assert!(
            id == "check" || id == "fmt" || id == "clippy",
            "unexpected first node: {}",
            id
        );
    }

    #[test]
    fn down_to_last_phase() {
        let snap = build_demo_snapshot();
        let mut nav = WorkflowNav::default();
        for _ in 0..20 {
            nav.down(&snap);
        }
        assert_eq!(nav.phase_idx, snap.phases.len() - 1);
    }
}
