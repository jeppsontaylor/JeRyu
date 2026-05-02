//! Owner: Cache Gateway subsystem — Cargo registry proxy
//! Proof: `cargo nextest run -p jeryu -- gateway::cargo`
//! Invariants: Cargo registry caching never crosses trust namespaces or serves unverified content as trusted.
use super::singleflight::Singleflight;
use anyhow::Result;
use std::sync::Arc;

use reqwest::Client;

// We will use an Arc-wrapped Singleflight so we can clone it across concurrent actors.
#[derive(Clone)]
pub struct CargoAdapter {
    upstream_url: String,
    http_client: Client,
    // Coalesce fetches of the same crate name + version
    fetch_coalescer: Arc<Singleflight<Result<Vec<u8>, String>>>,
}

impl CargoAdapter {
    pub fn new(upstream_url: &str) -> Self {
        Self {
            upstream_url: upstream_url.to_string(),
            http_client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            fetch_coalescer: Arc::new(Singleflight::new()),
        }
    }

    /// Fetch a crate tarball, utilizing the singleflight mechanism to coalesce concurrent requests
    /// for the same crate and version.
    pub async fn fetch_crate(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        let key = format!("{}:{}", name, version);

        if let Some(mut rx) = self.fetch_coalescer.join_or_start(&key) {
            // Another task is already fetching this crate, await its result.
            tracing::info!("Singleflight: joining active fetch for cargo crate {}", key);
            match rx.recv().await {
                Ok(Ok(bytes)) => return Ok(bytes),
                Ok(Err(e)) => anyhow::bail!("Coalesced fetch failed: {}", e),
                Err(_) => anyhow::bail!("Fetch coalescer sender dropped for {}", key),
            }
        }

        tracing::info!("Singleflight: initiating fetch for cargo crate {}", key);
        // We are the elected fetcher. Handle panics by wrapping in a Drop guard.
        let guard = super::singleflight::SingleflightGuard::new(&self.fetch_coalescer, &key);
        let result = self.do_fetch_crate(name, version).await;

        match &result {
            Ok(bytes) => {
                guard.complete(Ok(bytes.clone()));
            }
            Err(e) => {
                guard.complete(Err(e.to_string()));
            }
        }

        result
    }

    async fn do_fetch_crate(&self, name: &str, version: &str) -> Result<Vec<u8>> {
        let url = format!(
            "{}/api/v1/crates/{}/{}/download",
            self.upstream_url, name, version
        );
        let resp = self.http_client.get(&url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "Failed to fetch cargo crate {} {}: HTTP {}",
                name,
                version,
                resp.status()
            );
        }
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_cargo_singleflight_coalescence_with_real_server() -> Result<()> {
        // Shared counter to verify how many times the upstream API was ACTUALLY hit
        let hit_count = Arc::new(AtomicUsize::new(0));
        let hit_count_clone = hit_count.clone();

        // Spin up a mock axum server on a random port
        let app = Router::new().route(
            "/api/v1/crates/test-crate/1.0.0/download",
            get(move || {
                let count = hit_count_clone.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    // Add an artificial delay to guarantee we have overlapping concurrent requests
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    "mock_tarball_content".to_string()
                }
            }),
        );
        let listener = match TcpListener::bind("127.0.0.1:0").await {
            Ok(listener) => listener,
            Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
                eprintln!("skipping local listener test: {err}");
                return Ok(());
            }
            Err(err) => return Err(err.into()),
        };
        let port = listener.local_addr()?.port();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Give the server a tiny moment to bind
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let upstream_url = format!("http://127.0.0.1:{}", port);
        let adapter = CargoAdapter::new(&upstream_url);

        // Spawn 20 concurrent fetch requests for the exact same crate
        let mut handlers = Vec::new();
        for _ in 0..20 {
            let ad = adapter.clone();
            handlers.push(tokio::spawn(async move {
                ad.fetch_crate("test-crate", "1.0.0").await
            }));
        }

        // Wait for all fetches to finish
        for handle in handlers {
            let result = handle.await??;
            assert_eq!(result, b"mock_tarball_content");
        }

        // If singleflight works correctly, out of 20 concurrent requests, exactly 1 should hit the server.
        let total_hits = hit_count.load(Ordering::SeqCst);
        assert_eq!(
            total_hits, 1,
            "Expected exactly 1 request to hit the server due to singleflight coalescence, but got {}",
            total_hits
        );
        Ok(())
    }
}
