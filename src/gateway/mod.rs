//! Owner: Cache Gateway subsystem (module root)
//! Proof: `cargo nextest run -p jeryu -- gateway`
//! Invariants: Gateway modules preserve namespace isolation, singleflight behavior, and upstream recovery semantics.
pub mod cargo;
pub mod git;
pub mod npm;
pub mod oci;
pub mod singleflight;

use anyhow::Result;
use singleflight::{Singleflight, SingleflightGuard};
use std::{future::Future, sync::Arc};

/// Coalesce concurrent byte-fetch requests for `key` through `fetch_coalescer`.
///
/// Joiners wait on the broadcast channel; the elected fetcher invokes `request`
/// and broadcasts the resulting bytes (or error string) to waiters. A
/// `SingleflightGuard` ensures the singleflight slot is released even if the
/// fetcher panics.
pub(crate) async fn fetch_bytes_with_singleflight<F, Fut>(
    fetch_coalescer: &Arc<Singleflight<Result<Vec<u8>, String>>>,
    key: &str,
    join_label: &'static str,
    start_label: &'static str,
    request: F,
) -> Result<Vec<u8>>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<reqwest::Response, reqwest::Error>>,
{
    if let Some(mut rx) = fetch_coalescer.join_or_start(key) {
        tracing::info!(
            "Singleflight: joining active fetch for {} {}",
            join_label,
            key
        );
        match rx.recv().await {
            Ok(Ok(bytes)) => return Ok(bytes),
            Ok(Err(e)) => anyhow::bail!("Coalesced {} fetch failed: {}", join_label, e),
            Err(_) => anyhow::bail!("Fetch coalescer sender dropped for {}", key),
        }
    }

    tracing::info!("Singleflight: initiating fetch for {} {}", start_label, key);
    let guard = SingleflightGuard::new(fetch_coalescer, key);
    let resp_result = request().await;

    let result = match resp_result {
        Ok(resp) if resp.status().is_success() => {
            resp.bytes().await.map(|b| b.to_vec()).map_err(|e| e.into())
        }
        Ok(resp) => Err(anyhow::anyhow!("HTTP error: {}", resp.status())),
        Err(e) => Err(e.into()),
    };

    match &result {
        Ok(bytes) => guard.complete(Ok(bytes.clone())),
        Err(e) => guard.complete(Err(e.to_string())),
    }

    result
}
