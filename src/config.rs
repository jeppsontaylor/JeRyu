//! Owner: Configuration & Templates subsystem
//! Proof: `cargo nextest run -p jeryu -- config`
//! Invariants: Defaults and templates remain deterministic, local, and safe for unattended bootstrap.
//! Embedded templates and configuration constants for jeryu.
//!
//! This module owns the docker-compose.yml template, runner config.toml
//! template, and all default values that flow through the system.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLoadMode {
    Permissive,
    FailClosed,
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Root data directory for jeryu state on the host.
///
/// Defaults to `~/.jeryu` so a fresh bootstrap creates the canonical path.
pub fn data_dir() -> PathBuf {
    dirs_home().join(".jeryu")
}

/// Where jeryu.env lives (secrets file).
pub fn env_file() -> PathBuf {
    data_dir().join("jeryu.env")
}

/// Where the SQLite database lives.
pub fn db_path() -> PathBuf {
    data_dir().join("jeryu.db")
}

/// Persistent data root for the jeryu state Postgres service.
pub fn postgres_data_dir() -> PathBuf {
    data_dir().join("postgres")
}

/// Database URL override used for production Postgres or explicit SQLite paths.
pub fn database_url() -> Option<String> {
    std::env::var("JERYU_DATABASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Where runner config directories are created (one per manager).
pub fn runners_dir() -> PathBuf {
    data_dir().join("runners")
}

/// Root directory for runner cache bind mounts.
pub fn cache_root_dir() -> PathBuf {
    data_dir().join("cache")
}

/// Dedicated cache directory for a single runner manager.
pub fn manager_cache_dir(manager_id: &str) -> PathBuf {
    cache_root_dir().join("managers").join(manager_id)
}

/// Root for local agent-owned Cargo caches.
pub fn local_cargo_cache_root() -> PathBuf {
    cache_root_dir().join("local-cargo")
}

/// Local agent Cargo target cache root.
pub fn local_cargo_targets_root() -> PathBuf {
    local_cargo_cache_root().join("targets")
}

/// Local agent Cargo sccache root.
pub fn local_cargo_sccache_dir() -> PathBuf {
    local_cargo_cache_root().join("sccache")
}

/// Root for a pool-scoped runner cache namespace.
pub fn pool_cache_root(pool_name: &str) -> PathBuf {
    cache_root_dir().join("pools").join(pool_name)
}

/// Pool-scoped Cargo target cache root.
pub fn pool_cargo_targets_root(pool_name: &str) -> PathBuf {
    pool_cache_root(pool_name).join("cargo-targets")
}

/// Pool-scoped sccache root.
pub fn pool_cargo_sccache_dir(pool_name: &str) -> PathBuf {
    pool_cache_root(pool_name).join("sccache")
}

/// Inside-container mount path for the shared pool cache.
pub fn pool_cache_mount_path(executor: &str) -> &'static str {
    if executor == "custom" {
        "/pool-cache"
    } else {
        "/cache"
    }
}

/// Timeout, in seconds, used when waiting for runner managers to exit after SIGQUIT.
/// The production default comes from settings, but CI/test runs use a shorter fallback
/// unless an explicit override is provided.
pub fn runner_shutdown_timeout_secs() -> u64 {
    if let Some(value) = std::env::var("JERYU_POOL_SHUTDOWN_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        return value;
    }

    if is_test_or_ci_runtime() {
        return 30;
    }

    crate::settings::get().pool.runner_shutdown_timeout_secs
}

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

fn dirs_home() -> PathBuf {
    dirs::home_dir().expect("cannot determine home directory")
}

fn is_test_or_ci_runtime() -> bool {
    std::env::var_os("CI").is_some()
        || std::env::var_os("GITHUB_ACTIONS").is_some()
        || std::env::var_os("GITLAB_CI").is_some()
        || std::env::var_os("RUST_TEST_THREADS").is_some()
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

fn render_vault_local_config() -> String {
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

fn yaml_block(value: &str, indent: usize) -> String {
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

// ---------------------------------------------------------------------------
// Docker Compose Template
// ---------------------------------------------------------------------------

pub fn render_compose(root_password: &str) -> String {
    let cfg = gitlab_config_dir().display().to_string();
    let logs = gitlab_logs_dir().display().to_string();
    let data = gitlab_data_dir().display().to_string();
    let vault_data = vault_storage_dir().display().to_string();
    let postgres_data = postgres_data_dir().display().to_string();
    let vault_local_config = yaml_block(&render_vault_local_config(), 8);

    format!(
        r#"# docker-compose.yml - Generated by jeryu bootstrap — do not edit manually
services:
  jeryu-postgres:
    image: {postgres_image}
    container_name: jeryu-postgres
    restart: unless-stopped
    environment:
      POSTGRES_DB: "jeryu"
      POSTGRES_USER: "jeryu"
      POSTGRES_PASSWORD: "{root_password}"
    ports:
      - "127.0.0.1:{postgres_port}:5432"
    volumes:
      - "{postgres_data}:/var/lib/postgresql/data"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U jeryu -d jeryu"]
      interval: 5s
      timeout: 5s
      retries: 30

  vault:
    image: {vault_image}
    container_name: {vault_container}
    restart: unless-stopped
    command: server
    cap_add:
      - IPC_LOCK
    environment:
      VAULT_ADDR: "http://127.0.0.1:{vault_port}"
      VAULT_API_ADDR: "http://127.0.0.1:{vault_port}"
      VAULT_LOG_LEVEL: "warn"
      VAULT_LOCAL_CONFIG: |
{vault_local_config}
    ports:
      - "0.0.0.0:{vault_port}:8200"
    volumes:
      - "{vault_data}:/vault/file"
    healthcheck:
      test: ["CMD", "vault", "status"]
      interval: 10s
      timeout: 5s
      retries: 20

  gitlab:
    image: {image}
    container_name: jeryu-gitlab
    hostname: {hostname}
    restart: unless-stopped
    environment:
      GITLAB_ROOT_PASSWORD: "{root_password}"
      GITLAB_OMNIBUS_CONFIG: |
        external_url 'http://{hostname}:{http_port}'
        gitlab_rails['gitlab_shell_ssh_port'] = {ssh_port}
        gitlab_workhorse['api_ci_long_polling_duration'] = "50s"
        
        # --- Balanced local CI tuning ---
        # Run Puma in single mode to avoid cluster overhead on the
        # single-node local control plane while still allowing more
        # request concurrency than the old 1-worker cluster setup.
        puma['worker_processes'] = 0
        puma['max_threads'] = 8
        
        # Keep PostgreSQL lightweight, but not so starved that CI API
        # activity and job scheduling thrash under moderate load.
        postgresql['shared_buffers'] = "256MB"
        postgresql['max_worker_processes'] = 4

        # Sidekiq is the main memory spike during CI trace/artifact bursts.
        # Keep enough workers for responsive pipeline processing without
        # letting background jobs crowd Puma out of the cgroup.
        sidekiq['concurrency'] = 8
        
        # 3. Disable all side-utilities and monitoring
        prometheus_monitoring['enable'] = false
        alertmanager['enable'] = false
        gitlab_exporter['enable'] = false
        node_exporter['enable'] = false
        postgres_exporter['enable'] = false
        redis_exporter['enable'] = false
        
        # 4. Disable heavy enterprise components not needed for local CI
        gitlab_pages['enable'] = false
        mattermost['enable'] = false
        registry['enable'] = false
        
        # 5. Disable Redis persistence to save disk/IO overhead
        redis['save'] = []
        redis['appendonly'] = "no"
    ports:
      - "{http_port}:{http_port}"
      - "{ssh_port}:22"
    volumes:
      - "{cfg}:/etc/gitlab"
      - "{logs}:/var/log/gitlab"
      - "{data}:/var/opt/gitlab"
      - "{jeryu_bin}:/opt/jeryu/jeryu:ro"
    # Hard Docker limit so CI bursts cannot OOM-kill GitLab services
    mem_limit: 8g
    mem_reservation: 4g
    shm_size: "256m"
    logging:
      driver: "json-file"
      options:
        max-size: "50m"
        max-file: "3"
"#,
        image = GITLAB_IMAGE,
        hostname = GITLAB_HOSTNAME,
        root_password = root_password,
        http_port = GITLAB_HTTP_PORT,
        ssh_port = GITLAB_SSH_PORT,
        vault_image = VAULT_IMAGE,
        vault_container = VAULT_CONTAINER_NAME,
        vault_port = VAULT_HTTP_PORT,
        postgres_image = POSTGRES_IMAGE,
        postgres_port = POSTGRES_PORT,
        postgres_data = postgres_data,
        vault_data = vault_data,
        vault_local_config = vault_local_config,
        cfg = cfg,
        logs = logs,
        data = data,
        jeryu_bin = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("jeryu"))
            .display(),
    )
}

// ---------------------------------------------------------------------------
// Runner config.toml Template
// ---------------------------------------------------------------------------

pub fn render_runner_config(
    pool_name: &str,
    gitlab_url: &str,
    token: &str,
    executor: &str,
    cache_dir: &str,
    concurrent: i64,
    request_concurrency: i64,
) -> String {
    let pool_cache_mount = pool_cache_mount_path(executor);
    let cargo_pre_build_script =
        crate::cargo_cache::render_runner_cargo_pre_build_script(pool_cache_mount, executor);
    let executor_block = match executor {
        "custom" => format!(
            r#"  executor = "custom"
  shell = "sh"
  pre_get_sources_script = "export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt GIT_SSL_CAINFO=/etc/ssl/certs/ca-certificates.crt NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt; mkdir -p {pool_cache_mount}"
  [runners.custom_build_dir]
    enabled = true

  [runners.custom]
    # In Phase 2, we use our own Rust binary to handle container lifecycles.
    # The 'jeryu' binary is mounted into the manager container from the host.
    config_exec = "/usr/local/bin/jeryu"
    config_args = ["exec", "config"]

    prepare_exec = "/usr/local/bin/jeryu"
    prepare_args = ["exec", "prepare"]

    run_exec = "/usr/local/bin/jeryu"
    run_args = ["exec", "run"]

    cleanup_exec = "/usr/local/bin/jeryu"
    cleanup_args = ["exec", "cleanup"]"#,
            pool_cache_mount = pool_cache_mount,
        ),
        _ => {
            let ca_cert = "/etc/ssl/certs/ca-certificates.crt";
            let privileged = if pool_name == "build" {
                "true"
            } else {
                "false"
            };
            format!(
                r#"  executor = "docker"
  pre_get_sources_script = "export SSL_CERT_FILE={ca_cert} CARGO_HTTP_CAINFO={ca_cert} GIT_SSL_CAINFO={ca_cert} NODE_EXTRA_CA_CERTS={ca_cert}; mkdir -p /cache; if [ -n \"$CI_PROJECT_DIR\" ] && [ -d \"$CI_PROJECT_DIR\" ]; then if [ -d \"$CI_PROJECT_DIR/.git\" ]; then rm -f \"$CI_PROJECT_DIR/.git/shallow.lock\" \"$CI_PROJECT_DIR/.git/index.lock\"; git -C \"$CI_PROJECT_DIR\" remote get-url origin >/dev/null 2>&1 || rm -rf \"$CI_PROJECT_DIR\"; else rm -rf \"$CI_PROJECT_DIR\"; fi; fi"
  pre_build_script = {cargo_pre_build_script:?}
  [runners.custom_build_dir]
    enabled = true

  [runners.docker]
    image = "alpine:latest"
    pull_policy = "if-not-present"
    allowed_pull_policies = ["always", "if-not-present", "never"]
    privileged = {privileged}
    volumes = ["{ca_cert}:{ca_cert}:ro", "{cache_dir}:/cache:rw"]
    extra_hosts = ["host.docker.internal:host-gateway", "gitlab.local:host-gateway"]"#
            )
        }
    };

    format!(
        r#"# Generated by jeryu — pool: {pool_name}
concurrent = {concurrent}
check_interval = 3
log_format = "json"
listen_address = "0.0.0.0:9252"
shutdown_timeout = 3600

[[runners]]
  name = "jeryu-{pool_name}"
  url = "{gitlab_url}"
  token = "{token}"
  limit = {concurrent}
  request_concurrency = {request_concurrency}
  environment = [
    "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt",
    "CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt",
    "GIT_SSL_CAINFO=/etc/ssl/certs/ca-certificates.crt",
    "NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt",
    "JERYU_SCCACHE_ENABLED={sccache_enabled}",
    "JERYU_SCCACHE_CACHE_SIZE={sccache_cache_size}",
    "JERYU_CARGO_INCREMENTAL=0",
    "JERYU_CARGO_CACHE=1",
    "JERYU_CARGO_CACHE_ROOT={pool_cache_mount}",
  ]
{executor_block}
"#,
        pool_name = pool_name,
        concurrent = concurrent,
        gitlab_url = gitlab_url,
        token = token,
        request_concurrency = request_concurrency,
        executor_block = executor_block,
        sccache_enabled = if crate::settings::get().sccache.enabled {
            "1"
        } else {
            "0"
        },
        sccache_cache_size = crate::settings::get().sccache.cache_size,
        pool_cache_mount = pool_cache_mount,
    )
}

// ---------------------------------------------------------------------------
// .jeryu Workspace Config Parsing
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ShadowConfig {
    #[serde(default)]
    pub auto_sync_on_commit: bool,
    #[serde(default)]
    pub auto_remediate: bool,
    #[serde(default = "default_max_blocking_seconds")]
    pub max_blocking_seconds_on_push: u64,
}

fn default_max_blocking_seconds() -> u64 {
    5
}

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

pub fn load_shadow_config(repo_root: &std::path::Path) -> ShadowConfig {
    load_jeryu_workspace_config(repo_root, "shadow.toml")
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
        let composed = render_compose("supersecret");
        assert!(composed.contains("container_name: jeryu-postgres"));
        assert!(composed.contains("postgres:16-alpine"));
        assert!(composed.contains("POSTGRES_DB: \"jeryu\""));
        assert!(composed.contains("127.0.0.1:15432:5432"));
        assert!(composed.contains("container_name: jeryu-vault"));
        assert!(composed.contains("hashicorp/vault"));
        assert!(composed.contains("GITLAB_ROOT_PASSWORD: \"supersecret\""));
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
            "http://gitlab.local",
            "token123",
            "docker",
            "/tmp/jeryu-cache/default",
            4,
            2,
        );
        assert!(docker_cfg.contains("name = \"jeryu-default\""));
        assert!(docker_cfg.contains("executor = \"docker\""));
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
            "http://gitlab.local",
            "token123",
            "docker",
            "/tmp/jeryu-cache/build",
            4,
            2,
        );
        assert!(build_cfg.contains("executor = \"docker\""));
        assert!(build_cfg.contains("privileged = true"));

        let custom_cfg = render_runner_config(
            "default",
            "http://gitlab.local",
            "token123",
            "custom",
            "/tmp/jeryu-cache/default",
            4,
            2,
        );
        assert!(custom_cfg.contains("executor = \"custom\""));
        assert!(custom_cfg.contains("config_args = [\"exec\", \"config\"]"));
        assert!(custom_cfg.contains("run_args = [\"exec\", \"run\"]"));
        assert!(custom_cfg.contains("JERYU_CARGO_CACHE_ROOT=/pool-cache"));
        assert!(!custom_cfg.contains("pre_build_script ="));
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
