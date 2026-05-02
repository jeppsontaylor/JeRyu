//! Owner: Runner Telemetry subsystem
//! Proof: `cargo nextest run -p jeryu -- telemetry`
//! Invariants: Telemetry is append-friendly, non-secret, and stable enough for agent reasoning.
//! Runner telemetry and observability.
//!
//! Scrapes the local Prometheus metrics exposed by gitlab-runner.

use anyhow::{Context, Result};
use std::collections::HashMap;

/// Fetches metrics from a runner manager container's prometheus endpoint.
/// Under `docker` execution, if the manager is configured to expose metrics
/// on port 9252, this will retrieve them.
pub async fn scrape_manager_metrics(
    manager_ip: &str,
    port: u16,
) -> Result<HashMap<String, String>> {
    let url = format!("http://{}:{}/metrics", manager_ip, port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?;

    let text = client
        .get(&url)
        .send()
        .await
        .context("failed to metrics endpoint")?
        .error_for_status()?
        .text()
        .await?;

    let mut metrics = HashMap::new();

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(' ') {
            metrics.insert(key.to_string(), value.to_string());
        }
    }

    Ok(metrics)
}
