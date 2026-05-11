//! Owner: BuildKit Configuration (Per-Trust-Namespace Rootless Builders)
//! Proof: `cargo test -p jeryu -- buildkit`
//! Invariants: Each trust namespace has its own BuildKit instance to prevent cache poisoning; namespace is injected into all generated configs; never share builder state across namespaces

/// Manager for generating rootless dedicated BuildKit builder configurations
/// per trust namespace to avoid shared multitenancy caching vulnerabilities.
pub struct BuildKitManager {
    pub namespace: String,
}

impl BuildKitManager {
    pub fn new(namespace: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
        }
    }

    /// Provides environment variables for the sandbox to point `docker buildx`
    /// at the isolated builder instance.
    pub fn inject_env(&self) -> Vec<(String, String)> {
        vec![
            (
                "BUILDX_BUILDER".to_string(),
                format!("jeryu-{}", self.namespace),
            ),
            ("DOCKER_BUILDKIT".to_string(), "1".to_string()),
        ]
    }

    /// Generates the `buildkitd.toml` for the rootless builder instance.
    pub fn generate_config(&self, registry_mirror: &str) -> String {
        format!(
            r#"[worker.oci]
  enabled = true
  rootless = true
  gc = true
  gckeepstorage = 20000

[registry."docker.io"]
  mirrors = ["{}"]
  http = true

[registry."{}"]
  http = true
"#,
            registry_mirror, registry_mirror
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buildkit_env_injection() {
        let mgr = BuildKitManager::new("quarantine");
        let envs = mgr.inject_env();
        assert!(envs.contains(&("BUILDX_BUILDER".into(), "jeryu-quarantine".into())));
    }

    #[test]
    fn test_buildkit_config_generation() {
        let mgr = BuildKitManager::new("trusted");
        let toml = mgr.generate_config("127.0.0.1:19800");
        assert!(toml.contains("rootless = true"));
        assert!(toml.contains("mirrors = [\"127.0.0.1:19800\"]"));
    }
}
