//! Owner: Cache Gateway subsystem — OCI image proxy
//! Proof: `cargo nextest run -p vgit -- gateway::oci`
//! Invariants: OCI cache decisions keep digest identity and trust namespace separation intact.
use super::singleflight::Singleflight;
use anyhow::Result;
use reqwest::Client;
use std::sync::Arc;

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

        if let Some(mut rx) = self.fetch_coalescer.join_or_start(&key) {
            tracing::info!("Singleflight: joining active fetch for OCI blob {}", key);
            match rx.recv().await {
                Ok(Ok(bytes)) => return Ok(bytes),
                Ok(Err(e)) => anyhow::bail!("Coalesced OCI fetch failed: {}", e),
                Err(_) => anyhow::bail!("Fetch coalescer sender dropped for {}", key),
            }
        }

        tracing::info!("Singleflight: initiating fetch for OCI blob {}", key);
        let url = format!("{}/v2/{}/blobs/{}", self.upstream_url, repo, digest);
        let resp_result = self.http_client.get(&url).send().await;

        let result = match resp_result {
            Ok(resp) if resp.status().is_success() => {
                resp.bytes().await.map(|b| b.to_vec()).map_err(|e| e.into())
            }
            Ok(resp) => Err(anyhow::anyhow!("HTTP error: {}", resp.status())),
            Err(e) => Err(e.into()),
        };

        match &result {
            Ok(bytes) => self.fetch_coalescer.complete(&key, Ok(bytes.clone())),
            Err(e) => self.fetch_coalescer.complete(&key, Err(e.to_string())),
        }

        result
    }
}
