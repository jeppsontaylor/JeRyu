//! Owner: Cache Gateway subsystem — OCI image proxy
//! Proof: `cargo nextest run -p jeryu -- gateway::oci`
//! Invariants: OCI cache decisions keep digest identity and trust namespace separation intact.
use super::singleflight::Singleflight;
use anyhow::Result;
use reqwest::Client;
use std::{future::Future, sync::Arc};

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
    let resp_result = request().await;

    let result = match resp_result {
        Ok(resp) if resp.status().is_success() => {
            resp.bytes().await.map(|b| b.to_vec()).map_err(|e| e.into())
        }
        Ok(resp) => Err(anyhow::anyhow!("HTTP error: {}", resp.status())),
        Err(e) => Err(e.into()),
    };

    match &result {
        Ok(bytes) => fetch_coalescer.complete(key, Ok(bytes.clone())),
        Err(e) => fetch_coalescer.complete(key, Err(e.to_string())),
    }

    result
}

#[derive(Clone)]
pub struct OciAdapter {
    upstream_url: String,
    http_client: Client,
    fetch_coalescer: Arc<Singleflight<Result<Vec<u8>, String>>>,
}

impl OciAdapter {
    pub fn new(upstream_url: &str) -> Self {
        Self {
            upstream_url: upstream_url.to_string(),
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            fetch_coalescer: Arc::new(Singleflight::new()),
        }
    }

    pub async fn fetch_blob(&self, repo: &str, digest: &str) -> Result<Vec<u8>> {
        let key = format!("{}:{}", repo, digest);
        let url = format!("{}/v2/{}/blobs/{}", self.upstream_url, repo, digest);
        fetch_bytes_with_singleflight(&self.fetch_coalescer, &key, "OCI blob", "OCI blob", || {
            self.http_client.get(&url).send()
        })
        .await
    }
}
