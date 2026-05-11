use super::*;

mod explain;
mod promotion;
mod doctor;

pub use explain::{build_pipeline_explain_report, render_pipeline_explain_text};
pub use doctor::{build_pipeline_doctor_report, render_pipeline_doctor_text};
pub use promotion::{maybe_trigger_production_promotion, trigger_production_promotion};
pub(crate) use promotion::{pipeline_matches_release_sha, production_promotion_pipeline_id};

#[cfg(test)]
pub(crate) fn should_trigger_production_promotion_with_gate(
    view: &ReleaseAttemptView,
    prod_gate_exists: bool,
) -> bool {
    view.canary_state == "e2e-passed"
        && view.release_identity_ok
        && view.has_remote_gate
        && view.has_telemetry_gate
        && view.has_e2e_gate
        && !prod_gate_exists
}
