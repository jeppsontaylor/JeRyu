//! Owner: CLI Runtime Commands
//! Proof: `cargo check -p jeryu`
//! Invariants: All types are pub(crate); main.rs is the only consumer
//!
//! Pure data: clap enum definitions for the non-test `jeryu` CLI commands.

use clap::Subcommand;
use std::path::PathBuf;

use super::{infer_repo_name, parse_expanded_path};

#[derive(Subcommand)]
pub(crate) enum PoolCommands {
    /// List all pools and their managers.
    List,
    /// Scale a pool to N managers.
    Scale { name: String, count: usize },
    /// Pause a pool (stop accepting new jobs).
    Pause { name: String },
    /// Resume a paused pool.
    Resume { name: String },
    /// Drain a pool: pause, wait for jobs to finish, stop managers.
    Drain { name: String },
    /// Drain and remove a pool plus its GitLab runner registration.
    #[clap(name = "delete")]
    Remove { name: String },
    /// Rotate the auth token for a pool.
    RotateToken { name: String },
}

#[derive(Subcommand)]
pub(crate) enum JobCommands {
    /// List jobs for a project.
    List {
        project_id: i64,
        #[arg(long, default_value = "running,pending")]
        status: String,
    },
    /// Show job trace (log output).
    Trace { project_id: i64, job_id: i64 },
    /// Trigger a manual job.
    Play { project_id: i64, job_id: i64 },
    /// Cancel a running job.
    Cancel { project_id: i64, job_id: i64 },
    /// Retry a failed job.
    Retry { project_id: i64, job_id: i64 },
    /// Explain the latest structured failure evidence for a job.
    Explain { project_id: i64, job_id: i64 },
    /// Clear all job and pipeline histories from the database.
    Clear,
}

#[derive(Subcommand)]
pub(crate) enum PipelineCommands {
    /// Explain blocking vs non-blocking state for a specific pipeline.
    Explain {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Diagnose active jobs, runner assignment, and outdated trace symptoms.
    Doctor {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List all jobs with start/end/runtime fields and optionally ingest them.
    Jobs {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
        #[arg(long, default_value_t = false)]
        ingest: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Persist all current GitLab job timings for a pipeline.
    Ingest {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
    },
    /// Cancel a superseded or unwanted pipeline.
    Cancel {
        #[arg(long, default_value = "2")]
        project_id: i64,
        pipeline_id: i64,
    },
    /// Show historical slow CI jobs from the local jeryu timing ledger.
    Bottlenecks {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref")]
        ref_name: Option<String>,
        #[arg(long, default_value = "25")]
        limit: i64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum CacheCommands {
    /// Enable and configure Docker daemon for SmartCache registry mirror.
    Enable,
    /// Health-check proxy and registry reachability.
    Doctor,
    /// Show live SmartCache state and metrics.
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run garbage collection on the cache store.
    Gc {
        /// Preview actions without deleting anything.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Emit machine-readable JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Preserve cache directories for running runner managers.
        /// Pass --keep-active-managers=false to evict active caches at emergency disk pressure.
        #[arg(long, action = clap::ArgAction::Set, default_value_t = true, default_missing_value = "true", num_args = 0..=1)]
        keep_active_managers: bool,
        /// Only remove orphan manager caches older than this age, e.g. 12h or 2d.
        #[arg(long)]
        older_than: Option<String>,
        /// If total manager cache exceeds this budget, include all orphan caches as candidates.
        #[arg(long)]
        max_cache_gb: Option<f64>,
    },
}

#[derive(Subcommand)]
pub(crate) enum LocalCommands {
    /// Run cargo with jeryu-managed cache roots for a repository checkout.
    Cargo {
        /// Repository root to run cargo in.
        #[arg(long)]
        repo: PathBuf,
        /// Cargo arguments to forward after `--`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, required = true)]
        cargo_args: Vec<String>,
    },
    /// Print the cache-aware Cargo environment for a repository checkout.
    CargoEnv {
        /// Repository root to inspect.
        #[arg(long)]
        repo: PathBuf,
        /// Emit machine-readable JSON.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum AgentCommands {
    /// Spawn an autonomous agent on a project.
    Spawn {
        project_id: i64,
        /// Description of the task for the agent.
        #[arg(short, long)]
        task: String,
    },
    /// List active agents.
    List { project_id: i64 },
    /// Merge an MR only if the risk gate allows it.
    Merge {
        project_id: i64,
        mr_iid: i64,
        #[arg(long, default_value = "trusted")]
        trust_tier: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum SettingsCommands {
    /// Validate settings and repair a corrupt file backup if present.
    Repair,
    /// Reset `~/.jeryu/settings.json` to defaults.
    Reset {
        #[arg(long, default_value_t = false)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ReleaseCommands {
    /// Show the latest release attempts and canary state.
    Status {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long, default_value = "5")]
        limit: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Continuously refresh the latest release status.
    Watch {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        sha: Option<String>,
        #[arg(long, default_value = "5")]
        limit: usize,
        #[arg(long, default_value = "5")]
        interval_secs: u64,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Reconcile release attempts against the latest successful upstream pipeline.
    Reconcile {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Trigger approved A/B production promotion for a passed canary.
    PromoteProd {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Check SSH, Vault, registry, and disk before launching canary.
    Preflight {
        #[arg(long)]
        ssh_host: Option<String>,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Diagnose what is blocking canary or production for a release version.
    Doctor {
        #[arg(long)]
        version: Option<String>,
        /// Also run live preflight checks (SSH/Vault/registry/disk).
        #[arg(long, default_value_t = true)]
        preflight: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum SecretsCommands {
    /// Bootstrap and initialize the jeryu-managed Vault.
    Init,
    /// Show Vault health and the latest tracked secret rotation state.
    Status {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Rotate release-scoped secrets and render release envs.
    Rotate {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        target: String,
    },
    /// Finalize a previously rotated secret set after promotion succeeds.
    Finalize {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        target: String,
    },
    /// Regenerate the release handoff report from current artifacts.
    Report {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
    },
    /// Print recovery instructions for a release bundle.
    Recover {
        #[arg(long, default_value_t = infer_repo_name())]
        repo: String,
        #[arg(long)]
        version: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum HostCommands {
    /// Perform a storage audit on the host.
    StorageAudit,
    /// Check host, GitLab, Docker, and runner-cache health.
    Doctor {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run an aggressive reclaim operation.
    Reclaim {
        #[arg(long)]
        mode: String,
        #[arg(long, default_value_t = false)]
        plan: bool,
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
    /// Install the jeryu-gc systemd timer from ops/ci.
    InstallGcTimer {
        #[arg(long, default_value_t = false)]
        allow_sudo: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum RepoCommands {
    /// Generate the machine-readable agent routing index for jeryu.
    RenderAgentIndex {
        #[arg(long, default_value_t = false)]
        check: bool,
    },
    /// Audit agent-facing routing, docs, and generated index freshness.
    AuditAgentSurface {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Run the Postgres-backed state proof in a disposable container.
    PostgresStateProof,
    /// Capture the canonical TUI screenshots used in docs.
    CaptureTuiScreenshots {
        #[arg(long, value_parser = parse_expanded_path)]
        output_dir: Option<PathBuf>,
    },
}
