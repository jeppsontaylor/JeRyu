//! Owner: Interactive TUI subsystem — flow ETA estimation
//! Proof: `cargo nextest run -p jeryu -- tui::flow`
//! Invariants: ETA estimates remain advisory and never affect scheduling or release decisions.

use super::model::{EtaConfidence, EtaEstimate};

pub fn estimate_job_eta(
    job_name: &str,
    lane: super::model::LaneKind,
    elapsed_secs: i64,
) -> EtaEstimate {
    // A real implementation would query `Db::get_test_bottlenecks` or similar.
    // We will simulate estimation via rough fallbacks.
    let historical_duration = match lane {
        super::model::LaneKind::Unit => 60,
        super::model::LaneKind::Integration => 300,
        super::model::LaneKind::Security => 120,
        super::model::LaneKind::Build => 180,
        super::model::LaneKind::ReleaseExecution => 120,
        _ => 90,
    };

    let remaining = i64::max(0, historical_duration - elapsed_secs);

    // Slight tweak for known jobs that take a while
    let remaining = if job_name.contains("cargo build") {
        i64::max(0, 360 - elapsed_secs)
    } else {
        remaining
    };

    EtaEstimate {
        remaining_secs: remaining,
        confidence: EtaConfidence::Medium,
        reason: format!("lane avg {}s", historical_duration),
    }
}
