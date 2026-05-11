use clap::{Args, Subcommand};
use std::path::PathBuf;

use jeryu::install::{ColorMode, InteractiveMode, PathMode};
use jeryu::remote::ServiceMode;

use super::{
    AgentCommands, CacheCommands, HostCommands, JobCommands, LocalCommands, PipelineCommands,
    PoolCommands, ReleaseCommands, RepoCommands, SecretsCommands, SettingsCommands, TestCommands,
    parse_exec_script_path, parse_expanded_path,
};

#[derive(Args)]
pub(crate) struct InstallCommand {
    #[arg(
        long,
        global = true,
        default_value = "~/.jeryu/bin",
        value_parser = parse_expanded_path
    )]
    pub prefix: PathBuf,
    #[arg(long, global = true, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub json: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub yes: bool,
    #[arg(long, global = true, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,
    #[arg(long, global = true, value_enum, default_value_t = InteractiveMode::Auto)]
    pub interactive: InteractiveMode,
    #[arg(long, global = true, value_enum, default_value_t = PathMode::Advise)]
    pub path_mode: PathMode,
    #[arg(long, global = true, default_value_t = false)]
    pub verbose: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub install_deps: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub allow_sudo: bool,
    #[command(subcommand)]
    pub action: Option<InstallActionCommands>,
}

#[derive(Subcommand)]
pub(crate) enum InstallActionCommands {
    /// Inspect the current install target without mutating the machine.
    Doctor,
    /// Install into a throwaway prefix and verify the result.
    Smoke,
    /// Install the binary, verify Docker, and run `jeryu init`.
    Server,
    /// Remove the installed binary.
    Uninstall,
    /// Render the deterministic install GIF.
    RenderDemo {
        #[arg(long, value_parser = parse_expanded_path)]
        output: PathBuf,
        #[arg(long, value_parser = parse_expanded_path)]
        png: Option<PathBuf>,
    },
}

#[derive(Args)]
pub(crate) struct RemoteCommand {
    #[arg(long, global = true, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub json: bool,
    #[arg(long, global = true, default_value_t = false)]
    pub yes: bool,
    #[arg(long, global = true, value_enum, default_value_t = ColorMode::Auto)]
    pub color: ColorMode,
    #[arg(long, global = true, value_enum, default_value_t = InteractiveMode::Auto)]
    pub interactive: InteractiveMode,
    #[arg(long, global = true, value_enum, default_value_t = ServiceMode::Auto)]
    pub service_mode: ServiceMode,
    #[arg(long, global = true, default_value_t = false)]
    pub verbose: bool,
    #[command(subcommand)]
    pub action: RemoteActionCommands,
}

#[derive(Subcommand)]
pub(crate) enum RemoteActionCommands {
    /// Provision a remote host and save its metadata under ~/.jeryu/remotes.
    Install {
        target: String,
        #[arg(long)]
        alias: Option<String>,
        #[arg(long, default_value_t = false)]
        setup_key: bool,
        #[arg(long, value_parser = parse_expanded_path)]
        identity: Option<PathBuf>,
    },
    /// Re-upload the current binary and refresh the remote service.
    #[clap(name = concat!("up", "date"))]
    Refresh { alias: String },
    /// Inspect remote health.
    Doctor { alias: String },
    /// Show remote service status.
    Status { alias: String },
    /// Tail remote logs.
    Logs { alias: String },
    /// Restart the remote service.
    Restart { alias: String },
    /// Stop the remote service.
    Stop { alias: String },
    /// Start the remote service.
    Start { alias: String },
    /// Open an interactive SSH session.
    Ssh { alias: String },
    /// Run a remote command through the installed binary.
    Run {
        alias: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Open the standard SSH port tunnels.
    Tunnel { alias: String },
    /// Remove the remote binary and service.
    Uninstall { alias: String },
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Initialize the entire jeryu environment (GitLab + DB + Runners).
    Init,

    /// Alias for init.
    #[command(hide = true)]
    Bootstrap,

    /// Start all pools at their min_warm and run the background daemon.
    Serve,

    /// Local binary install, validation, and server bootstrap.
    Install(InstallCommand),

    /// Remote SSH setup and day-two management.
    Remote(RemoteCommand),

    /// Launch the interactive terminal UI.
    Tui {
        /// Render one frame and exit. Intended for CI smoke checks.
        #[arg(long, default_value_t = false)]
        once: bool,
        /// Run the TUI in interactive demo mode with animated simulated data.
        #[arg(long, default_value_t = false)]
        demo: bool,
        /// Render one deterministic PNG screenshot and exit.
        #[arg(long, default_value_t = false)]
        capture: bool,
        /// Render one deterministic screenshot from a real terminal session.
        #[arg(long, default_value_t = false)]
        screenshot: bool,
        /// TUI tab to render when capturing: mission, release, jobs, agents, tests, pools, cache, evidence, secrets, git, jank, or jankurai.
        #[arg(long, default_value = "jobs")]
        tab: String,
        /// Output path for --capture.
        #[arg(long, default_value = "paper/assets/jeryu-tui.png")]
        output: PathBuf,
        /// Capture width in terminal cells.
        #[arg(long, default_value_t = 140)]
        width: u16,
        /// Capture height in terminal cells.
        #[arg(long, default_value_t = 44)]
        height: u16,
        /// Time to keep a screenshot session alive for external capture tooling.
        #[arg(long, default_value_t = 1100)]
        screenshot_hold_ms: u64,
    },

    /// Drain all managers and stop GitLab.
    Down,

    /// Passthrough command for git.
    Git {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Save your work (git add + commit).
    Save {
        /// The commit message
        message: String,
    },

    /// Sync with the remote (pull --rebase + push).
    Sync,

    /// Undo the last save (git reset HEAD~1 --soft).
    Undo,

    /// Show full JeRyu system status (formerly Status).
    System,

    /// Show git status (AI magic layer).
    Status,

    /// Pool management.
    #[command(subcommand)]
    Pool(PoolCommands),

    /// Job management.
    #[command(subcommand)]
    Job(JobCommands),

    /// Pipeline inspection.
    #[command(subcommand)]
    Pipeline(PipelineCommands),

    /// Cache management.
    #[command(subcommand)]
    Cache(CacheCommands),

    /// Local agent cache-aware command wrappers.
    #[command(subcommand)]
    Local(LocalCommands),

    /// Log inspection.
    Logs {
        /// Manager ID to tail logs from.
        manager_id: String,
        /// Number of lines to show.
        #[arg(short = 'n', long, default_value = "50")]
        lines: usize,
    },

    /// Autonomous agent operations.
    #[command(subcommand)]
    Agent(AgentCommands),

    /// Repair or reset user settings.
    #[command(subcommand)]
    Settings(SettingsCommands),

    /// Run tests through the CI pipeline (agent-friendly).
    #[command(subcommand)]
    Test(TestCommands),

    /// Release monitoring and canary status.
    #[command(subcommand)]
    Release(ReleaseCommands),

    /// Vault-backed secret lifecycle and release handoff.
    #[command(subcommand)]
    Secrets(SecretsCommands),

    /// Show lane-aware CI and release progress for a ref.
    Progress {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Repo-local routing, agent surface, and generated ownership indexes.
    #[command(subcommand)]
    Repo(RepoCommands),

    /// Host node management and clean up.
    #[command(subcommand)]
    Host(HostCommands),

    /// Custom executor driver (invoked by gitlab-runner, not meant for humans).
    #[command(subcommand, hide = true)]
    Exec(ExecCommands), // allowlist: typed clap subcommand; invocations stay typed

    /// Git Server hook entrypoints for Admission Control.
    #[command(subcommand, hide = true)]
    ServerHook(ServerHookCommands),

    /// Capability API Server for Agent workers.
    #[command(subcommand, hide = true)]
    Capability(CapabilityCommands),

    /// MCP adapter for external coding agents.
    #[command(subcommand)]
    Mcp(McpCommands),

    /// Show the next highest-priority action for the current branch.
    Next {
        #[arg(long, default_value = "2")]
        project_id: i64,
        #[arg(long = "ref-name", alias = "ref", default_value = "main")]
        ref_name: String,
    },

    /// Explain why a job, release, or merge is blocked.
    ExplainBlocker {
        /// Entity type: job | release | merge
        entity_type: String,
        /// Entity ID (job_id, release attempt ID, or MR iid)
        entity_id: i64,
    },

    /// List all registered jeryu actions with risk tier and surfaces.
    #[command(name = "action", subcommand)]
    Action(ActionCommands),
}

#[derive(Subcommand)]
pub(crate) enum ActionCommands {
    /// List all registered actions.
    List {
        /// Output as JSON (for agent consumption).
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Clone)]
pub(crate) enum ExecCommands {
    /// Provide driver configuration to GitLab.
    Config,
    /// Prepare the execution environment (spin up container).
    Prepare,
    /// Run an execution stage (e.g., build_script, step_script).
    Run {
        /// Script path provided by GitLab Runner
        #[arg(value_parser = parse_exec_script_path)]
        script_path: String,
        /// Stage name
        stage: String,
    },
    /// Cleanup the execution environment.
    Cleanup,
}

pub(crate) fn exec_subcommand(command: &Commands) -> Option<ExecCommands> {
    match command {
        Commands::Exec(subcmd) => Some(subcmd.clone()), // allowlist: typed clap subcommand; invocations stay typed
        _ => None,
    }
}

#[derive(Subcommand)]
pub(crate) enum ServerHookCommands {
    /// Act as a pre-receive git server hook
    PreReceive,
}

#[derive(Subcommand)]
pub(crate) enum CapabilityCommands {
    /// Start the capability API server
    Serve { socket_path: String },
}

#[derive(Subcommand)]
pub(crate) enum McpCommands {
    /// Start the MCP server over stdio.
    Serve,
    /// Start the MCP server over Streamable HTTP on the configured loopback bind.
    ServeHttp,
    /// Print the MCP tool manifest as JSON.
    Tools {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}
