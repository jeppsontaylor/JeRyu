use super::*;

#[cfg(test)]
use tempfile::tempdir;

pub(crate) async fn install_local(opts: &InstallOptions) -> Result<i32> {
    let plan = build_plan("local", opts);
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        render_plan(&plan);
    }
    if opts.dry_run {
        return Ok(0);
    }

    if matches!(opts.path_mode, PathMode::Refresh)
        && !plan.platform.in_path
        && shell_profile_path(plan.platform.shell.as_deref()).is_none()
    {
        bail!("PATH block write requires a supported shell profile (bash, zsh, or fish)");
    }
    if !prompt_for_confirmation(&plan, opts)? {
        bail!("install cancelled");
    }

    let step_started = Instant::now();
    install_runtime::install_binary(&opts.prefix).await?;
    if matches!(opts.path_mode, PathMode::Refresh) {
        install_runtime::refresh_shell_profile(&opts.prefix, plan.platform.shell.as_deref())?;
    }
    install_runtime::verify_binary(&install_target(&opts.prefix)).await?;
    if !plan.platform.in_path && matches!(opts.path_mode, PathMode::Advise) {
        if let Some(advice) = &plan.path_advice {
            if let Some(rc) = &advice.rc_file {
                println!("PATH advice: add {} to {}", opts.prefix.display(), rc);
            }
            if let Some(snippet) = &advice.snippet {
                println!("{snippet}");
            }
        }
    }
    println!(
        "{} installed jeryu to {} in {}s",
        status_label(should_colorize(opts.color, opts.json), "OK", "32;1"),
        install_target(&opts.prefix).display(),
        step_started.elapsed().as_secs_f32()
    );
    Ok(0)
}

pub(crate) async fn doctor(opts: &InstallOptions) -> Result<i32> {
    let target = install_target(&opts.prefix);
    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => PathBuf::from("(unavailable)"),
    };
    let version = match install_runtime::run_output(&target, &["--version"]).await {
        Ok(output) => Some(output.trim().to_string()),
        Err(_) => None,
    };
    let report = DoctorReport {
        prefix: opts.prefix.display().to_string(),
        binary: target.display().to_string(),
        current_exe: current_exe.display().to_string(),
        installed: target.exists(),
        version_ok: version.is_some(),
        version_output: version,
    };
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("JeRyu install doctor");
        println!("  prefix:       {}", report.prefix);
        println!("  binary:       {}", report.binary);
        println!("  current exe:  {}", report.current_exe);
        println!("  installed:    {}", report.installed);
        println!("  version ok:   {}", report.version_ok);
        if let Some(output) = &report.version_output {
            println!("  version:      {}", output);
        }
    }
    if !report.installed {
        bail!("installed binary not found: {}", report.binary);
    }
    if !report.version_ok {
        bail!(
            "installed binary did not respond to --version: {}",
            version_hint(&target)
        );
    }
    Ok(0)
}

pub(crate) async fn smoke(opts: &InstallOptions) -> Result<i32> {
    let tmp = tempfile::tempdir().context("creating smoke scratch dir")?;
    let smoke_opts = InstallOptions {
        prefix: tmp.path().to_path_buf(),
        dry_run: opts.dry_run,
        json: opts.json,
        yes: opts.yes,
        color: opts.color,
        interactive: opts.interactive,
        path_mode: opts.path_mode,
        verbose: opts.verbose,
        install_deps: false,
        allow_sudo: false,
    };
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "smoke",
                "prefix": smoke_opts.prefix,
                "dry_run": opts.dry_run,
            }))?
        );
    } else {
        println!("JeRyu install smoke");
    }
    if opts.dry_run {
        return Ok(0);
    }
    if !prompt_for_confirmation(&build_plan("smoke", &smoke_opts), &smoke_opts)? {
        bail!("smoke install cancelled");
    }
    install_runtime::install_binary(&smoke_opts.prefix).await?;
    install_runtime::verify_binary(&install_target(&smoke_opts.prefix)).await?;
    Ok(0)
}

pub(crate) async fn server(opts: &InstallOptions) -> Result<i32> {
    let prefix = &opts.prefix;
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "server",
                "prefix": prefix,
                "dry_run": opts.dry_run,
                "install_deps": opts.install_deps,
                "allow_sudo": opts.allow_sudo,
            }))?
        );
    } else {
        println!("JeRyu server setup");
    }
    if opts.dry_run {
        return Ok(0);
    }

    if !prompt_for_confirmation(&build_plan("server", opts), opts)? {
        bail!("server setup cancelled");
    }
    install_runtime::install_binary(prefix).await?;
    install_runtime::ensure_docker(opts).await?;
    install_runtime::run_installed_binary(&install_target(prefix), &["init"]).await?;
    Ok(0)
}

pub(crate) async fn uninstall(opts: &InstallOptions) -> Result<i32> {
    let target = install_target(&opts.prefix);
    let backup_prefix = opts.prefix.join(".jeryu-backups");
    let shell = env::var("SHELL").ok();
    let rc_path = shell_profile_path(shell.as_deref());
    let mut report = UninstallReport {
        action: "uninstall".into(),
        prefix: opts.prefix.display().to_string(),
        binary: target.display().to_string(),
        backup_dir: backup_prefix.display().to_string(),
        dry_run: opts.dry_run,
        path_mode: opts.path_mode,
        path_rc_file: rc_path.as_ref().map(|path| path.display().to_string()),
        binary_present_before: target.exists(),
        backups_present_before: backup_prefix.exists(),
        path_block_found: install_runtime::path_block_found(rc_path.as_deref()),
        binary_removed: false,
        backups_removed: false,
        path_block_removed: false,
    };

    if opts.dry_run {
        emit_uninstall_report(&report, opts)?;
        return Ok(0);
    }

    if report.binary_present_before {
        fs::remove_file(&target).with_context(|| format!("removing {}", target.display()))?;
        report.binary_removed = true;
    }
    if report.backups_present_before {
        fs::remove_dir_all(&backup_prefix)
            .with_context(|| format!("removing {}", backup_prefix.display()))?;
        report.backups_removed = true;
    }
    if matches!(opts.path_mode, PathMode::Refresh) {
        report.path_block_removed =
            install_runtime::remove_shell_profile_path_block(shell.as_deref())?;
        report.path_block_found |= report.path_block_removed;
    }

    emit_uninstall_report(&report, opts)?;
    Ok(0)
}

fn emit_uninstall_report(report: &UninstallReport, opts: &InstallOptions) -> Result<()> {
    if opts.json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    let color = should_colorize(opts.color, opts.json);
    let label = if opts.dry_run {
        status_label(color, "PLAN", "36;1")
    } else {
        status_label(color, "OK", "32;1")
    };
    println!("{} JeRyu uninstall", label);
    println!("  binary:  {}", report.binary);
    println!(
        "  action:  {}",
        if opts.dry_run {
            if report.binary_present_before {
                "would remove binary"
            } else {
                "binary not present"
            }
        } else if report.binary_removed {
            "removed binary"
        } else {
            "binary not present"
        }
    );
    println!(
        "  backups: {}",
        if opts.dry_run {
            if report.backups_present_before {
                "would remove installer backups"
            } else {
                "none found"
            }
        } else if report.backups_removed {
            "removed installer backups"
        } else {
            "none found"
        }
    );

    match report.path_rc_file.as_deref() {
        Some(rc) if report.path_block_removed => {
            println!("  PATH:    removed guarded block from {rc}");
        }
        Some(rc) if report.path_block_found && matches!(opts.path_mode, PathMode::Refresh) => {
            println!("  PATH:    guarded block was found but could not be removed from {rc}");
        }
        Some(rc) if report.path_block_found && matches!(opts.path_mode, PathMode::Skip) => {
            println!("  PATH:    guarded block left in {rc} (--path-mode skip)");
        }
        Some(rc) if report.path_block_found => {
            println!(
                "  PATH:    guarded block remains in {rc}; rerun uninstall with PATH block write enabled to remove it"
            );
        }
        Some(rc) => {
            println!("  PATH:    no guarded block found in {rc}");
        }
        None => {
            println!("  PATH:    no supported shell profile detected");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tilde_expansion_targets_home() {
        let prefix = expand_tilde("~/.jeryu/bin");
        assert!(prefix.ends_with(".jeryu/bin"));
    }

    #[test]
    fn install_plan_stays_user_space() {
        let plan = build_plan(
            "local",
            &InstallOptions {
                prefix: "/tmp/jeryu".into(),
                dry_run: true,
                json: true,
                yes: true,
                color: ColorMode::Never,
                interactive: InteractiveMode::Never,
                path_mode: PathMode::Advise,
                verbose: false,
                install_deps: false,
                allow_sudo: false,
            },
        );
        let rendered = serde_json::to_value(&plan).unwrap();
        let steps = rendered["steps"].as_array().unwrap();
        assert_eq!(rendered["mode"], "local");
        assert!(!rendered["install_deps"].as_bool().unwrap());
        assert!(!rendered["allow_sudo"].as_bool().unwrap());
        assert!(steps.iter().all(|step| {
            let label = step["label"].as_str().unwrap();
            let command = step["command"].as_str().unwrap();
            !label.contains("sudo")
                && !label.contains("python")
                && !label.contains("pip")
                && !command.contains("sudo")
                && !command.contains("python")
                && !command.contains("pip")
        }));
    }

    #[test]
    fn path_snippets_are_shell_specific() {
        assert!(path_snippet(Path::new("/tmp/bin"), Some("/bin/bash")).contains("export PATH"));
        assert!(
            path_snippet(Path::new("/tmp/bin"), Some("/usr/bin/fish")).contains("set -gx PATH")
        );
        assert!(path_snippet(Path::new("/tmp/bin"), Some("/bin/zsh")).contains(JERYU_PATH_START));
        assert!(path_snippet(Path::new("/tmp/bin"), Some("/bin/zsh")).contains(JERYU_PATH_END));
    }

    #[test]
    fn strip_path_block_preserves_profile_content() {
        let text = concat!(
            "export BEFORE=1\n",
            "# >>> jeryu path >>>\n",
            "export PATH=\"/tmp/jeryu:$PATH\"\n",
            "# <<< jeryu path <<<\n",
            "alias gs='git status'\n",
        );
        let (updated, removed) = install_runtime::strip_jeryu_path_block(text);
        assert!(removed);
        assert_eq!(updated, "export BEFORE=1\nalias gs='git status'\n");
    }

    #[test]
    fn strip_path_block_ignores_partial_marker() {
        let text = concat!(
            "export BEFORE=1\n",
            "# >>> jeryu path >>>\n",
            "export PATH=\"/tmp/jeryu:$PATH\"\n",
            "alias gs='git status'\n",
        );
        let (updated, removed) = install_runtime::strip_jeryu_path_block(text);
        assert!(!removed);
        assert_eq!(updated, text);
    }

    #[test]
    fn remove_path_block_from_file_backs_up_profile() {
        let dir = tempdir().unwrap();
        let rc = dir.path().join("profile");
        fs::write(
            &rc,
            path_snippet(Path::new("/tmp/jeryu"), Some("/bin/bash")),
        )
        .unwrap();

        assert!(install_runtime::remove_path_block_from_file(&rc).unwrap());
        assert!(!install_runtime::has_jeryu_path_block(
            &fs::read_to_string(&rc).unwrap()
        ));
        assert!(rc.with_extension("jeryu-uninstall.bak").exists());
    }

    #[test]
    fn plan_tracks_path_advice_for_unknown_prefix() {
        let plan = build_plan(
            "local",
            &InstallOptions {
                prefix: tempdir().unwrap().path().join("jeryu-bin"),
                dry_run: true,
                json: false,
                yes: true,
                color: ColorMode::Auto,
                interactive: InteractiveMode::Auto,
                path_mode: PathMode::Advise,
                verbose: false,
                install_deps: false,
                allow_sudo: false,
            },
        );
        assert!(plan.path_advice.is_some());
        assert!(plan.steps.iter().any(|step| step.id == "verify"));
    }

    #[test]
    fn color_mode_respects_never_and_always() {
        assert!(!should_colorize(ColorMode::Never, false));
        assert!(should_colorize(ColorMode::Always, false));
    }
}
