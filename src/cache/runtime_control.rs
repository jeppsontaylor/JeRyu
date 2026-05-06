use anyhow::{Context, Result};
use tracing::info;

use super::*;

impl SmartCache {
    pub fn new(db: crate::state::Db) -> Self {
        Self {
            db,
            proxy_port: crate::config::CACHE_PROXY_PORT,
            registry_port: crate::config::CACHE_REGISTRY_PORT,
        }
    }

    pub async fn start(self) -> Result<()> {
        info!("Starting SmartCache supervisor...");
        self.start_warp_registry().await?;

        let proxy = std::sync::Arc::new(crate::cache_proxy::CacheProxy::new(
            self.proxy_port,
            self.db.clone(),
        ));
        tokio::spawn(async move {
            if let Err(e) = proxy.start().await {
                tracing::error!("warp-proxy failed: {:?}", e);
            }
        });

        Ok(())
    }

    async fn start_warp_registry(&self) -> Result<()> {
        info!(
            "Ensuring warp-registry container is running on 127.0.0.1:{}",
            self.registry_port
        );

        let output = tokio::process::Command::new("docker")
            .args(["ps", "-q", "-f", "name=warp-registry"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.trim().is_empty() {
            info!("warp-registry is already running");
            return Ok(());
        }

        let output = tokio::process::Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                "warp-registry",
                &format!("-p=0.0.0.0:{}:5000", self.registry_port),
                "--restart",
                "always",
                "-e",
                "REGISTRY_PROXY_REMOTEURL=https://registry-1.docker.io",
                "registry:2",
            ])
            .output()
            .await
            .context("Failed to start warp-registry")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(CacheError::RegistryFailed(err.into_owned()).into());
        }

        info!("Started warp-registry container");
        Ok(())
    }

    pub async fn enable(&self) -> Result<()> {
        println!("🔧 Enabling SmartCache Docker mirror...");
        let daemon_json = std::path::Path::new("/etc/docker/daemon.json");
        let mut config = if daemon_json.exists() {
            let content = std::fs::read_to_string(daemon_json)?;
            std::fs::write("/etc/docker/daemon.json.bak", &content)?;
            serde_json::from_str::<serde_json::Value>(&content).unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if let Some(obj) = config.as_object_mut() {
            let mirror = serde_json::json!([format!("http://127.0.0.1:{}", self.registry_port)]);
            obj.insert("registry-mirrors".to_string(), mirror);
        }

        std::fs::write(daemon_json, serde_json::to_string_pretty(&config)?)?;

        let valid = tokio::process::Command::new("sudo")
            .args([
                "dockerd",
                "--validate",
                "--config-file",
                daemon_json.to_str().unwrap(),
            ])
            .status()
            .await?;

        if !valid.success() {
            println!("Docker config validation failed, rolling back...");
            if std::path::Path::new("/etc/docker/daemon.json.bak").exists() {
                std::fs::copy("/etc/docker/daemon.json.bak", "/etc/docker/daemon.json")?;
            }
            return Err(CacheError::DockerConfigInvalid.into());
        }

        println!("Restarting Docker daemon...");
        let status = tokio::process::Command::new("sudo")
            .args(["systemctl", "restart", "docker"])
            .status()
            .await?;

        if !status.success() {
            println!("Docker failed to start, rolling back...");
            if std::path::Path::new("/etc/docker/daemon.json.bak").exists() {
                std::fs::copy("/etc/docker/daemon.json.bak", "/etc/docker/daemon.json")?;
                let _ = tokio::process::Command::new("sudo")
                    .args(["systemctl", "restart", "docker"])
                    .status()
                    .await;
            }
            return Err(CacheError::DockerRestartFailed.into());
        }

        println!("✅ SmartCache Docker mirror enabled");
        Ok(())
    }

    pub async fn doctor(&self) -> Result<()> {
        println!("🩺 Running SmartCache doctor...");
        println!("Checking proxy reachability ({})...", self.proxy_port);
        let proxy_up = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", self.proxy_port))
            .await
            .is_ok();
        println!("Checking registry mirror ({})...", self.registry_port);
        let reg_up = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", self.registry_port))
            .await
            .is_ok();

        println!("Checking local cache directory writeability...");
        let cache_dir = crate::config::data_dir().join("cache");
        std::fs::create_dir_all(&cache_dir)?;
        let test_file = cache_dir.join(".doctor_test");
        let disk_ok = std::fs::write(&test_file, b"ok").is_ok();
        let _ = std::fs::remove_file(test_file);

        if proxy_up && reg_up && disk_ok {
            println!("✅ SmartCache is healthy");
        } else {
            return Err(CacheError::HealthCheckFailed(proxy_up, reg_up, disk_ok).into());
        }
        Ok(())
    }
}
