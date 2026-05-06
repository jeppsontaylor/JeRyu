use super::*;

/// Where docker-compose.yml is written.
pub fn compose_file() -> PathBuf {
    data_dir().join("docker-compose.yml")
}

/// Vault persistent data root.
pub fn vault_dir() -> PathBuf {
    data_dir().join("vault")
}

/// Vault runtime configuration directory.
pub fn vault_config_dir() -> PathBuf {
    vault_dir().join("config")
}

/// Vault persistent storage directory.
pub fn vault_storage_dir() -> PathBuf {
    vault_dir().join("data")
}

/// jeryu-managed Vault operational environment file.
pub fn vault_env_file() -> PathBuf {
    vault_dir().join("vault.env")
}

/// Break-glass bootstrap material for Vault.
pub fn vault_bootstrap_file() -> PathBuf {
    vault_dir().join("bootstrap.json")
}

/// Vault server configuration file.
pub fn vault_config_file() -> PathBuf {
    vault_config_dir().join("vault.hcl")
}

/// GitLab persistent volume paths on the host.
pub fn gitlab_config_dir() -> PathBuf {
    data_dir().join("gitlab").join("config")
}
pub fn gitlab_logs_dir() -> PathBuf {
    data_dir().join("gitlab").join("logs")
}
pub fn gitlab_data_dir() -> PathBuf {
    data_dir().join("gitlab").join("data")
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

pub const GITLAB_IMAGE: &str = "gitlab/gitlab-ce:17.9.2-ce.0";
pub const GITLAB_RUNNER_IMAGE: &str = "gitlab/gitlab-runner:v17.9.2";
pub const GITLAB_HOSTNAME: &str = "gitlab.local";
pub const GITLAB_HTTP_PORT: u16 = 8929;
pub const GITLAB_SSH_PORT: u16 = 2224;
pub const WEBHOOK_LISTEN_PORT: u16 = 9777;
pub const VAULT_IMAGE: &str = "hashicorp/vault:1.17.5";
pub const VAULT_CONTAINER_NAME: &str = "jeryu-vault";
pub const VAULT_HTTP_PORT: u16 = 18200;
pub const POSTGRES_IMAGE: &str = "postgres:16-alpine";
pub const POSTGRES_PORT: u16 = 15432;
pub const VAULT_DEFAULT_MOUNT: &str = "secret";
pub const VAULT_DEFAULT_PREFIX: &str = "jeryu";

pub const CACHE_PROXY_PORT: u16 = 19800;
pub const CACHE_REGISTRY_PORT: u16 = 19801;

pub(crate) fn render_vault_local_config() -> String {
    format!(
        r#"ui = true
disable_mlock = true
api_addr = "http://127.0.0.1:{port}"

listener "tcp" {{
  address     = "0.0.0.0:8200"
  tls_disable = 1
}}

storage "file" {{
  path = "/vault/file"
}}
"#,
        port = VAULT_HTTP_PORT
    )
}

pub(crate) fn yaml_block(value: &str, indent: usize) -> String {
    let padding = " ".repeat(indent);
    value
        .lines()
        .map(|line| format!("{padding}{line}\n"))
        .collect::<String>()
}

/// Default pool definitions created during bootstrap.
pub struct PoolDef {
    pub name: &'static str,
    pub tags: &'static str,
    pub executor: &'static str,
    pub min_warm: i64,
    pub max_managers: i64,
    pub concurrent: i64,
    pub request_concurrency: i64,
    pub trust_tier: &'static str,
}

pub const DEFAULT_POOLS: &[PoolDef] = &[
    PoolDef {
        name: "default",
        tags: "default,rust,test",
        executor: "docker",
        min_warm: 2,
        max_managers: 4,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "trusted",
    },
    PoolDef {
        name: "build",
        tags: "build,docker-build,x86-64,docker,dind",
        executor: "docker",
        min_warm: 2,
        max_managers: 4,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "privileged",
    },
    PoolDef {
        name: "untrusted",
        tags: "untrusted,sandbox,mr",
        executor: "custom",
        min_warm: 1,
        max_managers: 2,
        concurrent: 1,
        request_concurrency: 1,
        trust_tier: "untrusted",
    },
];

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProofConfig {
    #[serde(default)]
    pub lanes: std::collections::HashMap<String, Vec<String>>,
    #[serde(default)]
    pub vti: VtiConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct VtiConfig {
    #[serde(default)]
    pub ast_aware_skipping: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AgentConfig {
    #[serde(default)]
    pub autonomy: AutonomyConfig,
    #[serde(default)]
    pub context: ContextConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AutonomyConfig {
    #[serde(default)]
    pub auto_merge_remediations: bool,
    #[serde(default)]
    pub budget_limit_usd: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ContextConfig {
    #[serde(default)]
    pub mandatory_context: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SandboxConfig {
    #[serde(default)]
    pub isolation: IsolationConfig,
    #[serde(default)]
    pub exceptions: ExceptionsConfig,
    #[serde(default)]
    pub detonation: DetonationConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct IsolationConfig {
    #[serde(default = "default_egress")]
    pub default_network_egress: String,
}

fn default_egress() -> String {
    "block".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ExceptionsConfig {
    #[serde(default)]
    pub allow_egress: Vec<EgressException>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct EgressException {
    #[serde(default)]
    pub lane: String,
    #[serde(default)]
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DetonationConfig {
    #[serde(default)]
    pub tripwires: Vec<String>,
}

/// Helper to load a TOML configuration file if it exists, otherwise returning the Default.
pub fn load_jeryu_workspace_config<T: serde::de::DeserializeOwned + Default>(
    repo_root: &std::path::Path,
    filename: &str,
) -> T {
    load_jeryu_workspace_config_with_mode(repo_root, filename, ConfigLoadMode::Permissive)
}

pub fn load_jeryu_workspace_config_with_mode<T: serde::de::DeserializeOwned + Default>(
    repo_root: &std::path::Path,
    filename: &str,
    mode: ConfigLoadMode,
) -> T {
    let path = repo_root.join(".jeryu").join(filename);
    if let Ok(contents) = std::fs::read_to_string(&path) {
        match toml::from_str(&contents) {
            Ok(value) => value,
            Err(err) => match mode {
                ConfigLoadMode::Permissive => {
                    eprintln!(
                        "Warning: Failed to parse {}, using defaults: {}",
                        path.display(),
                        err
                    );
                    T::default()
                }
                ConfigLoadMode::FailClosed => {
                    panic!("Failed to parse {}: {}", path.display(), err)
                }
            },
        }
    } else {
        T::default()
    }
}

pub fn load_proof_config(repo_root: &std::path::Path) -> ProofConfig {
    load_jeryu_workspace_config(repo_root, "proof.toml")
}

pub fn load_agent_config(repo_root: &std::path::Path) -> AgentConfig {
    load_jeryu_workspace_config(repo_root, "agent.toml")
}

pub fn load_sandbox_config(repo_root: &std::path::Path) -> SandboxConfig {
    load_jeryu_workspace_config(repo_root, "sandbox.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
        // SAFETY: this test module serializes environment mutation with ENV_LOCK
        // and restores prior values before releasing the lock.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
        // SAFETY: this test module serializes environment mutation with ENV_LOCK
        // and restores prior values before releasing the lock.
        unsafe {
            std::env::remove_var(key);
        }
    }

    #[test]
    fn test_render_compose() {
        let composed = render_compose("example-root-password");
        assert!(composed.contains("container_name: jeryu-postgres"));
        assert!(composed.contains("postgres:16-alpine"));
        assert!(composed.contains("POSTGRES_DB: \"jeryu\""));
        assert!(composed.contains("127.0.0.1:15432:5432"));
        assert!(composed.contains("container_name: jeryu-vault"));
        assert!(composed.contains("hashicorp/vault"));
        assert!(composed.contains("GITLAB_ROOT_PASSWORD: \"example-root-password\""));
        assert!(composed.contains("gitlab_workhorse['api_ci_long_polling_duration']"));
        assert!(composed.contains("docker-compose.yml")); // Should have some identifying comment
        assert!(composed.contains("puma['worker_processes'] = 0"));
        assert!(composed.contains("puma['max_threads'] = 8"));
        assert!(composed.contains("postgresql['shared_buffers'] = \"256MB\""));
        assert!(composed.contains("sidekiq['concurrency'] = 8"));
        assert!(composed.contains("mem_limit: 8g"));
        assert!(composed.contains("mem_reservation: 4g"));
        assert!(composed.contains("redis['save'] = []"));
        assert!(composed.contains("max-size: \"50m\""));
    }

    #[test]
    fn test_render_runner_config() {
        let docker_cfg = render_runner_config(
            "default",
            "manager-1",
            "http://gitlab.local",
            "example-runner-token",
            "docker",
            "/tmp/jeryu-cache/default",
            4,
            2,
        );
        assert!(docker_cfg.contains("name = \"jeryu-default\""));
        assert!(docker_cfg.contains("executor = \"docker\""));
        assert!(docker_cfg.contains("builds_dir = \"/builds/default-manager-1\""));
        assert!(docker_cfg.contains("limit = 4"));
        assert!(docker_cfg.contains("privileged = false"));
        assert!(docker_cfg.contains("pull_policy = \"if-not-present\""));
        assert!(docker_cfg.contains("JERYU_CARGO_CACHE=1"));
        assert!(docker_cfg.contains("JERYU_CARGO_CACHE_ROOT=/cache"));
        assert!(docker_cfg.contains("pre_build_script"));
        assert!(docker_cfg.contains("JERYU_SCCACHE_ENABLED=1"));
        assert!(!docker_cfg.contains("/usr/local/bin/sccache:/usr/local/bin/sccache:ro"));
        assert!(!docker_cfg.contains("find /cache -mindepth 1 -maxdepth 1 -exec rm -rf"));
        assert!(!docker_cfg.contains("executor = \"custom\""));
        let parsed = docker_cfg.parse::<toml::Value>().unwrap();
        let runners = parsed
            .get("runners")
            .and_then(|value| value.as_array())
            .unwrap();
        let docker_runner = &runners[0];
        assert!(docker_runner.get("pre_build_script").is_some());
        let pre_build_script = docker_runner
            .get("pre_build_script")
            .and_then(toml::Value::as_str)
            .unwrap();
        assert!(!pre_build_script.contains("exit 0"));
        assert!(
            docker_runner
                .get("docker")
                .and_then(|value| value.get("pre_build_script"))
                .is_none()
        );

        let build_cfg = render_runner_config(
            "build",
            "manager-2",
            "http://gitlab.local",
            "example-runner-token",
            "docker",
            "/tmp/jeryu-cache/build",
            4,
            2,
        );
        assert!(build_cfg.contains("executor = \"docker\""));
        assert!(build_cfg.contains("privileged = true"));

        let custom_cfg = render_runner_config(
            "default",
            "manager/with spaces",
            "http://gitlab.local",
            "example-runner-token",
            "custom",
            "/tmp/jeryu-cache/default",
            4,
            2,
        );
        assert!(custom_cfg.contains("executor = \"custom\""));
        assert!(custom_cfg.contains("builds_dir = \"/builds/default-manager-with-spaces\""));
        assert!(custom_cfg.contains("config_args = [\"exec\", \"config\"]"));
        assert!(custom_cfg.contains("run_args = [\"exec\", \"run\"]"));
        assert!(custom_cfg.contains("JERYU_CARGO_CACHE_ROOT=/pool-cache"));
        assert!(!custom_cfg.contains("pre_build_script ="));
    }

    #[test]
    fn manager_builds_dir_is_pool_and_manager_scoped() {
        assert_eq!(
            manager_builds_dir("build/pool", "manager 123"),
            "/builds/build-pool-manager-123"
        );
        assert_ne!(
            manager_builds_dir("build", "manager-a"),
            manager_builds_dir("build", "manager-b")
        );
    }

    #[test]
    fn runner_shutdown_timeout_uses_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS").ok();
        set_env_var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS", "12");

        assert_eq!(runner_shutdown_timeout_secs(), 12);

        match original {
            Some(value) => set_env_var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS", value),
            None => remove_env_var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS"),
        }
    }
}
