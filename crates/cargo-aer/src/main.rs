use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};

use cargo_aer::{init_records, markdown_report, sarif_report, scan_workspace, stale_records};
use cargo_vrc::load_workspace;

#[derive(Parser)]
#[command(name = "cargo-aer")]
#[command(about = "Agent Exception Record auditing for Cargo workspaces")]
struct Cli {
    #[arg(long, global = true)]
    manifest_path: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Scan {
        #[arg(long, default_value = "aer-findings.json")]
        output: PathBuf,
    },
    Init,
    Stale,
    Report {
        #[arg(long, default_value = "aer-findings.json")]
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = ReportFormat::Md)]
        format: ReportFormat,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ReportFormat {
    Json,
    Md,
    Sarif,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cargo-aer error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Scan { output } => {
            let report = scan_workspace(cli.manifest_path.as_deref())?;
            write_json(&output, &report)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report
                .findings
                .iter()
                .any(|finding| finding.severity == "error")
            {
                bail!("scan reported error-level findings");
            }
        }
        Command::Init => {
            let snapshot = load_workspace(cli.manifest_path.as_deref())?;
            let created = init_records(&snapshot.workspace_root)?;
            for path in created {
                println!("{}", path.display());
            }
        }
        Command::Stale => {
            let snapshot = load_workspace(cli.manifest_path.as_deref())?;
            let findings = stale_records(&snapshot.workspace_root)?;
            println!("{}", serde_json::to_string_pretty(&findings)?);
            if !findings.is_empty() {
                bail!("found stale or incomplete AER records");
            }
        }
        Command::Report {
            input,
            format,
            output,
        } => {
            let payload = fs::read_to_string(&input)
                .with_context(|| format!("failed to read {}", input.display()))?;
            let report: cargo_aer::ScanReport = serde_json::from_str(&payload)
                .with_context(|| format!("failed to parse {}", input.display()))?;
            match format {
                ReportFormat::Json => emit_text(output, serde_json::to_string_pretty(&report)?)?,
                ReportFormat::Md => emit_text(output, markdown_report(&report))?,
                ReportFormat::Sarif => emit_text(
                    output,
                    serde_json::to_string_pretty(&sarif_report(&report))?,
                )?,
            }
        }
    }
    Ok(())
}

fn emit_text(path: Option<PathBuf>, payload: String) -> Result<()> {
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&path, payload).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        println!("{payload}");
    }
    Ok(())
}

fn write_json(path: &PathBuf, value: &impl serde::Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
