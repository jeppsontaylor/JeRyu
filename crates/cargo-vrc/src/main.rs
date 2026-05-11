use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use cargo_vrc::{
    build_agent_map, build_test_map, build_vrc_plan, explain_subject, load_workspace,
    verify_workspace,
};

#[derive(Parser)]
#[command(name = "cargo-vrc")]
#[command(about = "Validation Radius Contract tooling for Cargo workspaces")]
struct Cli {
    #[arg(long, global = true)]
    manifest_path: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Map {
        #[arg(long, default_value = ".")]
        output_dir: PathBuf,
    },
    Plan {
        #[arg(required = true)]
        changed: Vec<PathBuf>,
        #[arg(long, default_value = "vrc-plan.json")]
        output: PathBuf,
    },
    Explain {
        subject: String,
    },
    Verify,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("cargo-vrc error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let snapshot = load_workspace(cli.manifest_path.as_deref())?;

    match cli.command {
        Command::Map { output_dir } => {
            let agent_map = build_agent_map(&snapshot);
            let test_map = build_test_map(&snapshot);
            fs::create_dir_all(&output_dir)
                .with_context(|| format!("failed to create {}", output_dir.display()))?;
            write_json(output_dir.join("agent-map.json"), &agent_map)?;
            write_json(output_dir.join("test-map.json"), &test_map)?;
            println!(
                "wrote {} and {}",
                output_dir.join("agent-map.json").display(),
                output_dir.join("test-map.json").display()
            );
        }
        Command::Plan { changed, output } => {
            let plan = build_vrc_plan(&snapshot, &changed)?;
            write_json(&output, &plan)?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
        Command::Explain { subject } => {
            let explanation = explain_subject(&snapshot, &subject)?;
            println!("{}", serde_json::to_string_pretty(&explanation)?);
        }
        Command::Verify => {
            let report = verify_workspace(&snapshot);
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.errors.is_empty() {
                bail!("workspace verification failed");
            }
        }
    }

    Ok(())
}

fn write_json(path: impl Into<PathBuf>, value: &impl serde::Serialize) -> Result<()> {
    let path = path.into();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let payload = serde_json::to_string_pretty(value)?;
    fs::write(&path, payload).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}
