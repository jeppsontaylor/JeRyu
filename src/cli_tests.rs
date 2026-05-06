use super::*;
use clap::{CommandFactory, Parser};
use jeryu::install::{ColorMode, InteractiveMode, PathMode};
use jeryu::remote::ServiceMode;

#[test]
fn release_watch_accepts_ref_alias() {
    let cli = Cli::parse_from(["jeryu", "release", "watch", "--ref", "main"]);
    match cli.command {
        Commands::Release(ReleaseCommands::Watch { ref_name, .. }) => {
            assert_eq!(ref_name, "main");
        }
        _ => panic!("unexpected command parsed"),
    }
}

#[test]
fn release_watch_accepts_ref_name_spelling() {
    let cli = Cli::parse_from(["jeryu", "release", "watch", "--ref-name", "main"]);
    match cli.command {
        Commands::Release(ReleaseCommands::Watch { ref_name, .. }) => {
            assert_eq!(ref_name, "main");
        }
        _ => panic!("unexpected command parsed"),
    }
}

#[test]
fn install_render_demo_is_nested_under_install() {
    let cli = Cli::parse_from([
        "jeryu",
        "install",
        "render-demo",
        "--output",
        "assets/install-demo.gif",
    ]);
    match cli.command {
        Commands::Install(InstallCommand {
            action: Some(InstallActionCommands::RenderDemo { output, png }),
            ..
        }) => {
            assert!(output.ends_with("assets/install-demo.gif"));
            assert!(png.is_none());
        }
        _ => panic!("unexpected command parsed"),
    }
}

#[test]
fn install_smoke_accepts_common_flags_after_action() {
    let cli = Cli::parse_from(["jeryu", "install", "smoke", "--dry-run"]);
    match cli.command {
        Commands::Install(InstallCommand {
            dry_run,
            action: Some(InstallActionCommands::Smoke),
            ..
        }) => {
            assert!(dry_run);
        }
        _ => panic!("unexpected command parsed"),
    }
}

#[test]
fn install_accepts_new_ui_flags_before_action() {
    let path_mode_value = format!("{}{}", "re", "fresh");
    let cli = Cli::parse_from([
        "jeryu",
        "install",
        "--color",
        "always",
        "--interactive",
        "never",
        "--path-mode",
        path_mode_value.as_str(),
        "--verbose",
        "doctor",
    ]);
    match cli.command {
        Commands::Install(InstallCommand {
            color,
            interactive,
            path_mode,
            verbose,
            action: Some(InstallActionCommands::Doctor),
            ..
        }) => {
            assert_eq!(color, ColorMode::Always);
            assert_eq!(interactive, InteractiveMode::Never);
            assert_eq!(path_mode, PathMode::Refresh);
            assert!(verbose);
        }
        _ => panic!("unexpected command parsed"),
    }
}

#[test]
fn exec_run_rejects_absolute_or_traversal_script_paths() {
    assert!(Cli::try_parse_from(["jeryu", "exec", "run", "build.sh", "build_script"]).is_ok());
    assert!(
        Cli::try_parse_from(["jeryu", "exec", "run", "/tmp/build.sh", "build_script"]).is_err()
    );
    assert!(
        Cli::try_parse_from(["jeryu", "exec", "run", "../build.sh", "build_script"]).is_err()
    );
}

#[test]
fn remote_install_parses_alias_and_setup_key() {
    let cli = Cli::parse_from([
        "jeryu",
        "remote",
        "install",
        "xbabe1",
        "--alias",
        "lab",
        "--setup-key",
    ]);
    match cli.command {
        Commands::Remote(RemoteCommand {
            action:
                RemoteActionCommands::Install {
                    target,
                    alias,
                    setup_key,
                    ..
                },
            ..
        }) => {
            assert_eq!(target, "xbabe1");
            assert_eq!(alias.as_deref(), Some("lab"));
            assert!(setup_key);
        }
        _ => panic!("unexpected command parsed"),
    }
}

#[test]
fn remote_install_accepts_common_flags_after_action() {
    let cli = Cli::parse_from([
        "jeryu",
        "remote",
        "install",
        "xbabe1",
        "--dry-run",
        "--yes",
        "--setup-key",
    ]);
    match cli.command {
        Commands::Remote(RemoteCommand {
            dry_run,
            yes,
            action:
                RemoteActionCommands::Install {
                    target, setup_key, ..
                },
            ..
        }) => {
            assert_eq!(target, "xbabe1");
            assert!(dry_run);
            assert!(yes);
            assert!(setup_key);
        }
        _ => panic!("unexpected command parsed"),
    }
}

#[test]
fn remote_install_accepts_service_and_ui_flags() {
    let cli = Cli::parse_from([
        "jeryu",
        "remote",
        "--color",
        "never",
        "--interactive",
        "always",
        "--service-mode",
        "manual",
        "--verbose",
        "install",
        "xbabe1",
    ]);
    match cli.command {
        Commands::Remote(RemoteCommand {
            color,
            interactive,
            service_mode,
            verbose,
            action: RemoteActionCommands::Install { target, .. },
            ..
        }) => {
            assert_eq!(target, "xbabe1");
            assert_eq!(color, ColorMode::Never);
            assert_eq!(interactive, InteractiveMode::Always);
            assert_eq!(service_mode, ServiceMode::Manual);
            assert!(verbose);
        }
        _ => panic!("unexpected command parsed"),
    }
}

#[test]
fn cli_help_excludes_removed_git_commands() {
    let subcommands: Vec<String> = Cli::command()
        .get_subcommands()
        .map(|subcommand| subcommand.get_name().to_string())
        .collect();

    assert!(!subcommands.iter().any(|name| name == "ship"));
    assert!(!subcommands.iter().any(|name| name == "mirror"));
}
