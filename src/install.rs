//! Owner: Local installer and guided bootstrap UX
//! Proof: `cargo test -p jeryu -- install`
//! Invariants: Local installs remain user-space by default, avoid shell mutations unless requested, and never require sudo for the default path.

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::ValueEnum;
use serde::Serialize;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Instant;
use tempfile::Builder;
use tokio::process::Command;

#[path = "install_runtime.rs"]
mod install_runtime;

const JERYU_PATH_START: &str = "# >>> jeryu path >>>";
const JERYU_PATH_END: &str = "# <<< jeryu path <<<";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum InteractiveMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ValueEnum)]
pub enum PathMode {
    Advise,
    Refresh,
    Skip,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlatformProbe {
    pub os: String,
    pub arch: String,
    pub shell: Option<String>,
    pub tty: bool,
    pub in_path: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathAdvice {
    pub shell: Option<String>,
    pub rc_file: Option<String>,
    pub snippet: Option<String>,
    pub refresh_performed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub action: String,
    pub mode: String,
    pub prefix: String,
    pub target_binary: String,
    pub source_binary: String,
    pub platform: PlatformProbe,
    pub path_advice: Option<PathAdvice>,
    pub dry_run: bool,
    pub json: bool,
    pub color: ColorMode,
    pub interactive: InteractiveMode,
    pub path_mode: PathMode,
    pub verbose: bool,
    pub install_deps: bool,
    pub allow_sudo: bool,
    pub steps: Vec<PlanStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlanStep {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub command: Option<String>,
    pub requires_sudo: bool,
    pub estimated_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub prefix: String,
    pub binary: String,
    pub current_exe: String,
    pub installed: bool,
    pub version_ok: bool,
    pub version_output: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UninstallReport {
    pub action: String,
    pub prefix: String,
    pub binary: String,
    pub backup_dir: String,
    pub dry_run: bool,
    pub path_mode: PathMode,
    pub path_rc_file: Option<String>,
    pub binary_present_before: bool,
    pub backups_present_before: bool,
    pub path_block_found: bool,
    pub binary_removed: bool,
    pub backups_removed: bool,
    pub path_block_removed: bool,
}

/// Resolved install runtime options.
///
/// Field order here is grouped by concern (target, safety gates, output mode,
/// UX) and intentionally diverges from the flat clap layout in
/// `crate::cli::InstallCommand`. Initialise by name; clap is the only
/// canonical source for default values and CLI ergonomics.
#[derive(Debug, Clone)]
pub struct InstallOptions {
    // --- target ---
    /// Install prefix; expands `~` against the current user.
    pub prefix: PathBuf,
    /// Strategy for managing the user's PATH (advise, refresh, skip).
    pub path_mode: PathMode,
    // --- safety gates ---
    /// Plan only; do not mutate the host filesystem.
    pub dry_run: bool,
    /// Skip interactive confirmation (`--yes`).
    pub yes: bool,
    /// Allow installing system-level dependencies.
    pub install_deps: bool,
    /// Permit invoking `sudo` for privileged steps.
    pub allow_sudo: bool,
    // --- output / UX ---
    /// Emit machine-readable JSON instead of human prose.
    pub json: bool,
    /// Verbose progress logging.
    pub verbose: bool,
    /// Color rendering policy.
    pub color: ColorMode,
    /// Interactive prompt policy.
    pub interactive: InteractiveMode,
}

pub async fn run_local(opts: &InstallOptions) -> Result<i32> {
    install_local(opts).await
}

pub async fn run_doctor(opts: &InstallOptions) -> Result<i32> {
    doctor(opts).await
}

pub async fn run_smoke(opts: &InstallOptions) -> Result<i32> {
    smoke(opts).await
}

pub async fn run_server(opts: &InstallOptions) -> Result<i32> {
    server(opts).await
}

pub async fn run_uninstall(opts: &InstallOptions) -> Result<i32> {
    uninstall(opts).await
}

fn current_exe_string() -> String {
    match env::current_exe() {
        Ok(path) => path.display().to_string(),
        Err(_) => "(unavailable)".into(),
    }
}

pub fn expand_tilde(input: impl AsRef<str>) -> PathBuf {
    let input = input.as_ref();
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(input)
}

fn install_target(prefix: &Path) -> PathBuf {
    prefix.join("jeryu")
}

fn detect_platform(prefix: &Path) -> PlatformProbe {
    let shell = env::var("SHELL").ok();
    PlatformProbe {
        os: env::consts::OS.to_string(),
        arch: env::consts::ARCH.to_string(),
        shell,
        tty: io::stdout().is_terminal(),
        in_path: path_contains_dir(prefix),
    }
}

fn path_contains_dir(dir: &Path) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|entry| entry == dir)
}

fn shell_profile_path(shell: Option<&str>) -> Option<PathBuf> {
    let shell = shell?;
    let name = Path::new(shell)
        .file_name()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let home = dirs::home_dir()?;
    match name.as_str() {
        "bash" => Some(home.join(".bashrc")),
        "zsh" => Some(home.join(".zshrc")),
        "fish" => Some(home.join(".config/fish/config.fish")),
        _ => None,
    }
}

fn path_snippet(prefix: &Path, shell: Option<&str>) -> String {
    let path = prefix.display();
    let shell_name = match shell {
        Some(value) => match Path::new(value).file_name() {
            Some(name) => name.to_string_lossy().to_ascii_lowercase(),
            None => String::new(),
        },
        None => String::new(),
    };
    match shell_name.as_str() {
        "fish" => format!(
            "{JERYU_PATH_START}\nset -gx PATH \"{}\" $PATH\n{JERYU_PATH_END}",
            path
        ),
        _ => format!(
            "{JERYU_PATH_START}\nexport PATH=\"{}:$PATH\"\n{JERYU_PATH_END}",
            path
        ),
    }
}

fn build_plan(mode: &str, opts: &InstallOptions) -> InstallPlan {
    let prefix = opts.prefix.display().to_string();
    let target = install_target(&opts.prefix);
    let source = current_exe_string();
    let platform = detect_platform(&opts.prefix);
    let path_advice = if platform.in_path {
        None
    } else {
        let rc_file = shell_profile_path(platform.shell.as_deref());
        Some(PathAdvice {
            shell: platform.shell.clone(),
            rc_file: rc_file.as_ref().map(|path| path.display().to_string()),
            snippet: rc_file
                .as_ref()
                .map(|_| path_snippet(&opts.prefix, platform.shell.as_deref())),
            refresh_performed: matches!(opts.path_mode, PathMode::Refresh),
        })
    };
    let mut steps = vec![
        PlanStep {
            id: "ensure-prefix".into(),
            label: "ensure install prefix exists".into(),
            detail: format!("create {}", opts.prefix.display()),
            command: Some(format!("mkdir -p {}", opts.prefix.display())),
            requires_sudo: false,
            estimated_seconds: Some(1),
        },
        PlanStep {
            id: "install-binary".into(),
            label: "replace the binary atomically".into(),
            detail: format!("copy {} -> {}", source, target.display()),
            command: Some(format!(
                "install -m 0755 <current-exe> {}",
                target.display()
            )),
            requires_sudo: false,
            estimated_seconds: Some(2),
        },
    ];
    if !platform.in_path {
        let detail = match opts.path_mode {
            PathMode::Advise => "print shell-specific PATH advice".to_string(),
            PathMode::Refresh => "write the shell profile with a guarded PATH block".to_string(),
            PathMode::Skip => "skip PATH advice and leave shell profiles untouched".to_string(),
        };
        steps.push(PlanStep {
            id: "path".into(),
            label: "handle PATH visibility".into(),
            detail,
            command: Some(match opts.path_mode {
                PathMode::Advise => format!(
                    "echo {}",
                    path_snippet(&opts.prefix, platform.shell.as_deref())
                ),
                PathMode::Refresh => {
                    if let Some(rc) = shell_profile_path(platform.shell.as_deref()) {
                        format!("append {} to {}", opts.prefix.display(), rc.display())
                    } else {
                        "no supported shell profile found".into()
                    }
                }
                PathMode::Skip => "no PATH mutation".into(),
            }),
            requires_sudo: false,
            estimated_seconds: Some(1),
        });
    }
    steps.push(PlanStep {
        id: "verify".into(),
        label: "verify the installed binary".into(),
        detail: "run jeryu --version from the target binary".into(),
        command: Some(format!("{} --version", target.display())),
        requires_sudo: false,
        estimated_seconds: Some(1),
    });
    InstallPlan {
        action: "install".into(),
        mode: mode.into(),
        prefix,
        target_binary: target.display().to_string(),
        source_binary: source,
        platform,
        path_advice,
        dry_run: opts.dry_run,
        json: opts.json,
        color: opts.color,
        interactive: opts.interactive,
        path_mode: opts.path_mode,
        verbose: opts.verbose,
        install_deps: opts.install_deps,
        allow_sudo: opts.allow_sudo,
        steps,
    }
}

pub(crate) fn should_colorize(mode: ColorMode, json: bool) -> bool {
    if json {
        return false;
    }
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none(),
    }
}

pub(crate) fn should_interactive(mode: InteractiveMode) -> bool {
    match mode {
        InteractiveMode::Always => true,
        InteractiveMode::Never => false,
        InteractiveMode::Auto => io::stdin().is_terminal(),
    }
}

pub(crate) fn color_text(enabled: bool, code: &str, text: &str) -> String {
    if enabled {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub(crate) fn status_label(enabled: bool, label: &str, code: &str) -> String {
    format!("[{}]", color_text(enabled, code, label))
}

pub(crate) fn render_plan_steps<T, FReq, FLabel, FDetail, FCommand>(
    steps: &[T],
    verbose: bool,
    mut requires_highlight: FReq,
    mut label_of: FLabel,
    mut detail_of: FDetail,
    mut command_of: FCommand,
    enabled: bool,
    label_when_true: &str,
    label_when_false: &str,
    true_code: &str,
    false_code: &str,
) where
    FReq: FnMut(&T) -> bool,
    FLabel: FnMut(&T) -> &str,
    FDetail: FnMut(&T) -> &str,
    FCommand: FnMut(&T) -> Option<&str>,
{
    for step in steps {
        let label = if requires_highlight(step) {
            status_label(enabled, label_when_true, true_code)
        } else {
            status_label(enabled, label_when_false, false_code)
        };
        println!("  {} {} - {}", label, label_of(step), detail_of(step));
        if verbose && let Some(command) = command_of(step) {
            println!("      {}", command);
        }
    }
}

pub(crate) fn prompt_for_confirmation_with_message(
    prompt: &str,
    refusal_message: &str,
    interactive: InteractiveMode,
    yes: bool,
) -> Result<bool> {
    if yes {
        return Ok(true);
    }
    if !should_interactive(interactive) {
        bail!("{}", refusal_message);
    }
    print!("{}", prompt);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("reading confirmation")?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn render_plan(plan: &InstallPlan) {
    let color = should_colorize(plan.color, plan.json);
    println!(
        "{} {}",
        status_label(color, "PLAN", "36;1"),
        color_text(color, "1", &format!("JeRyu {} plan", plan.mode))
    );
    println!("  prefix: {}", plan.prefix);
    println!("  target: {}", plan.target_binary);
    println!("  source: {}", plan.source_binary);
    println!(
        "  platform: {} / {}{}",
        plan.platform.os,
        plan.platform.arch,
        if plan.platform.tty { " / tty" } else { "" }
    );
    println!(
        "  PATH: {}",
        if plan.platform.in_path {
            "already on PATH"
        } else {
            "not on PATH"
        }
    );
    render_plan_steps(
        &plan.steps,
        plan.verbose,
        |step| step.requires_sudo,
        |step| step.label.as_str(),
        |step| step.detail.as_str(),
        |step| step.command.as_deref(),
        color,
        "WARN",
        "RUN",
        "33;1",
        "36;1",
    );
    if let Some(advice) = &plan.path_advice {
        match plan.path_mode {
            PathMode::Skip => {
                println!("  PATH: skipped by request");
            }
            PathMode::Advise | PathMode::Refresh => {
                if let Some(snippet) = &advice.snippet {
                    println!("  PATH snippet:");
                    for line in snippet.lines() {
                        println!("      {}", line);
                    }
                }
            }
        }
    }
}

fn prompt_for_confirmation(_plan: &InstallPlan, opts: &InstallOptions) -> Result<bool> {
    prompt_for_confirmation_with_message(
        "Proceed with this install? [y/N] ",
        "refusing to mutate the machine without --yes in non-interactive mode; rerun with --yes or --dry-run",
        opts.interactive,
        opts.yes,
    )
}

fn version_hint(binary: &Path) -> String {
    format!("Try: {} --version", binary.display())
}

#[path = "install_commands.rs"]
mod install_commands;

pub(crate) use install_commands::{doctor, install_local, server, smoke, uninstall};
