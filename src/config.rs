//! Owner: Configuration & Templates subsystem
//! Proof: `cargo nextest run -p vgit -- config`
//! Invariants: Defaults and templates remain deterministic, local, and safe for unattended bootstrap.
//! Embedded templates and configuration constants for vgit.
//!
//! This module owns the docker-compose.yml template, runner config.toml
//! template, and all default values that flow through the system.

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// Root data directory for vgit state on the host.
pub fn data_dir() -> PathBuf {
    dirs_home().join(".vgit")
}

/// Where vgit.env lives (secrets file).
pub fn env_file() -> PathBuf {
    data_dir().join("vgit.env")
}

/// Where the SQLite database lives.
pub fn db_path() -> PathBuf {
    data_dir().join("vgit.db")
}

/// Persistent data root for the vgit state Postgres service.
pub fn postgres_data_dir() -> PathBuf {
    data_dir().join("postgres")
}

/// Database URL override used for production Postgres or explicit SQLite paths.
pub fn database_url() -> Option<String> {
    std::env::var("VGIT_DATABASE_URL")
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
    if let Some(value) = std::env::var("VGIT_POOL_SHUTDOWN_TIMEOUT_SECS")
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

/// vgit-managed Vault operational environment file.
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
pub const VAULT_CONTAINER_NAME: &str = "vgit-vault";
pub const VAULT_HTTP_PORT: u16 = 18200;
pub const POSTGRES_IMAGE: &str = "postgres:16-alpine";
pub const POSTGRES_PORT: u16 = 15432;
pub const VAULT_DEFAULT_MOUNT: &str = "secret";
pub const VAULT_DEFAULT_PREFIX: &str = "veox";

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
        r#"# docker-compose.yml - Generated by vgit bootstrap — do not edit manually
services:
  vgit-postgres:
    image: {postgres_image}
    container_name: vgit-postgres
    restart: unless-stopped
    environment:
      POSTGRES_DB: "vgit"
      POSTGRES_USER: "vgit"
      POSTGRES_PASSWORD: "{root_password}"
    ports:
      - "127.0.0.1:{postgres_port}:5432"
    volumes:
      - "{postgres_data}:/var/lib/postgresql/data"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U vgit -d vgit"]
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
    container_name: vgit-gitlab
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
      - "{vgit_bin}:/opt/vgit/vgit:ro"
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
        vgit_bin = std::env::current_exe()
            .unwrap_or_else(|_| PathBuf::from("vgit"))
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
    # The 'vgit' binary is mounted into the manager container from the host.
    config_exec = "/usr/local/bin/vgit"
    config_args = ["exec", "config"]

    prepare_exec = "/usr/local/bin/vgit"
    prepare_args = ["exec", "prepare"]

    run_exec = "/usr/local/bin/vgit"
    run_args = ["exec", "run"]

    cleanup_exec = "/usr/local/bin/vgit"
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
        r#"# Generated by vgit — pool: {pool_name}
concurrent = {concurrent}
check_interval = 3
log_format = "json"
listen_address = "0.0.0.0:9252"
shutdown_timeout = 3600

[[runners]]
  name = "vgit-{pool_name}"
  url = "{gitlab_url}"
  token = "{token}"
  limit = {concurrent}
  request_concurrency = {request_concurrency}
  environment = [
    "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt",
    "CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt",
    "GIT_SSL_CAINFO=/etc/ssl/certs/ca-certificates.crt",
    "NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt",
    "VGIT_SCCACHE_ENABLED={sccache_enabled}",
    "VGIT_SCCACHE_CACHE_SIZE={sccache_cache_size}",
    "VGIT_CARGO_INCREMENTAL=0",
    "VGIT_CARGO_CACHE=1",
    "VGIT_CARGO_CACHE_ROOT={pool_cache_mount}",
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
        assert!(composed.contains("container_name: vgit-postgres"));
        assert!(composed.contains("postgres:16-alpine"));
        assert!(composed.contains("POSTGRES_DB: \"vgit\""));
        assert!(composed.contains("127.0.0.1:15432:5432"));
        assert!(composed.contains("container_name: vgit-vault"));
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
            "/tmp/vgit-cache/default",
            4,
            2,
        );
        assert!(docker_cfg.contains("name = \"vgit-default\""));
        assert!(docker_cfg.contains("executor = \"docker\""));
        assert!(docker_cfg.contains("limit = 4"));
        assert!(docker_cfg.contains("privileged = false"));
        assert!(docker_cfg.contains("pull_policy = \"if-not-present\""));
        assert!(docker_cfg.contains("VGIT_CARGO_CACHE=1"));
        assert!(docker_cfg.contains("VGIT_CARGO_CACHE_ROOT=/cache"));
        assert!(docker_cfg.contains("pre_build_script"));
        assert!(docker_cfg.contains("VGIT_SCCACHE_ENABLED=1"));
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
            "/tmp/vgit-cache/build",
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
            "/tmp/vgit-cache/default",
            4,
            2,
        );
        assert!(custom_cfg.contains("executor = \"custom\""));
        assert!(custom_cfg.contains("config_args = [\"exec\", \"config\"]"));
        assert!(custom_cfg.contains("run_args = [\"exec\", \"run\"]"));
        assert!(custom_cfg.contains("VGIT_CARGO_CACHE_ROOT=/pool-cache"));
        assert!(!custom_cfg.contains("pre_build_script ="));
    }

    #[test]
    fn runner_shutdown_timeout_uses_env_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        let original = std::env::var("VGIT_POOL_SHUTDOWN_TIMEOUT_SECS").ok();
        set_env_var("VGIT_POOL_SHUTDOWN_TIMEOUT_SECS", "12");

        assert_eq!(runner_shutdown_timeout_secs(), 12);

        match original {
            Some(value) => set_env_var("VGIT_POOL_SHUTDOWN_TIMEOUT_SECS", value),
            None => remove_env_var("VGIT_POOL_SHUTDOWN_TIMEOUT_SECS"),
        }
    }
}
