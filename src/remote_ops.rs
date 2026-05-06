use super::*;

pub(crate) async fn remote_install(
    cfg: RemoteConfig,
    setup_key: bool,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    let plan = build_remote_plan(&cfg, setup_key, opts);
    if plan.options.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        render_remote_plan(&plan);
    }
    if plan.options.dry_run {
        return Ok(0);
    }

    if !remote_confirmation(&plan, opts)? {
        bail!("remote install cancelled");
    }

    if !plan.preflight.local_ssh || !plan.preflight.local_ssh_keygen {
        bail!("ssh and ssh-keygen are required for remote install");
    }
    let preflight = probe_remote(&cfg).await?;
    if opts.verbose {
        println!("remote probe: {:?}", preflight);
    }
    ensure_remote_key(&cfg, setup_key).await?;
    upload_current_binary(&cfg).await?;
    run_remote_binary(&cfg, &["--version"], false).await?;
    remote_bootstrap(&cfg).await?;
    let mut cfg = cfg;
    cfg.service_mode = effective_service_mode(opts.service_mode, preflight.remote_systemd_user);
    match cfg.service_mode {
        ServiceMode::User => {
            if !preflight.remote_systemd_user.unwrap_or(false) {
                bail!("remote host does not expose systemd --user");
            }
            ensure_remote_service(&cfg).await?;
        }
        ServiceMode::Manual => {
            print_manual_service_guidance(&cfg);
        }
        ServiceMode::Auto => panic!("effective service mode should not remain Auto"),
    }
    save_remote_config(&cfg)?;
    println!("remote host ready: {} ({})", cfg.alias, cfg.target);
    Ok(0)
}

pub(crate) async fn remote_refresh(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    if opts.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "action": "remote-update",
                "config": cfg,
                "dry_run": opts.dry_run,
            }))?
        );
    } else {
        println!("Remote refresh for {}", cfg.alias);
    }
    if opts.dry_run {
        return Ok(0);
    }
    upload_current_binary(cfg).await?;
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            ensure_remote_service(cfg).await?;
            remote_service(cfg, "restart", opts).await
        }
        ServiceMode::Manual => {
            print_manual_service_guidance(cfg);
            Ok(0)
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
}

pub(crate) fn print_action_envelope(
    opts: &RemoteCommonOptions,
    payload: serde_json::Value,
) -> Result<()> {
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    Ok(())
}

pub(crate) fn print_remote_report(
    label: &str,
    report: &RemoteReport,
    opts: &RemoteCommonOptions,
) -> Result<()> {
    if opts.json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        println!("Remote {}: {}", label, report.alias);
        println!("  target:         {}", report.target);
        println!("  binary:         {}", report.remote_bin);
        println!("  installed:      {}", report.installed);
        println!("  service active: {}", report.service_active);
        println!("  docker ready:   {}", report.docker_ready);
        if label == "doctor" {
            if let Some(version) = &report.version_output {
                println!("  version:        {}", version.trim());
            }
        }
    }
    Ok(())
}

pub(crate) async fn remote_doctor(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let report = collect_report(cfg).await?;
    print_remote_report("doctor", &report, opts)?;
    if !report.installed {
        bail!("remote binary not installed");
    }
    if !report.docker_ready {
        bail!("remote docker is not ready");
    }
    Ok(0)
}

pub(crate) async fn remote_status(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let report = collect_report(cfg).await?;
    print_remote_report("status", &report, opts)?;
    Ok(0)
}

pub(crate) async fn remote_logs(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    print_action_envelope(
        opts,
        serde_json::json!({
            "alias": cfg.alias,
            "target": cfg.target,
            "action": "logs",
        }),
    )?;
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            let cmd = "journalctl --user -u jeryu -n 100 --no-pager";
            run_remote_shell(cfg, cmd, opts.dry_run).await?;
        }
        ServiceMode::Manual => {
            bail!("remote host uses manual service mode; there is no systemd journal to tail");
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
    Ok(0)
}

pub(crate) async fn remote_service(
    cfg: &RemoteConfig,
    action: &str,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            let cmd = format!("systemctl --user {action} jeryu.service");
            run_remote_shell(cfg, &cmd, opts.dry_run).await?;
        }
        ServiceMode::Manual => {
            bail!(
                "remote host uses manual service mode; use '{}' serve over ssh instead",
                cfg.remote_bin
            );
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
    Ok(0)
}

pub(crate) async fn remote_ssh(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    let mut command = Command::new("ssh");
    command.args(ssh_args(cfg));
    command.arg(&cfg.target);
    if opts.dry_run {
        println!("dry-run: ssh {}", cfg.target);
        return Ok(0);
    }
    run_interactive_ssh(command, "ssh", "opening ssh session").await
}

pub(crate) async fn remote_run(
    cfg: &RemoteConfig,
    command: Vec<String>,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    if command.is_empty() {
        bail!("remote run requires a command after --");
    }
    if opts.dry_run {
        println!("dry-run: {} {}", cfg.remote_bin, command.join(" "));
        return Ok(0);
    }
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg(&cfg.remote_bin);
    cmd.args(&command);
    run_interactive_ssh(cmd, "remote command", "running remote command").await
}

pub(crate) async fn remote_tunnel(cfg: &RemoteConfig, opts: &RemoteCommonOptions) -> Result<i32> {
    if opts.dry_run {
        println!(
            "dry-run: ssh -N -L 127.0.0.1:{}:127.0.0.1:{} -L 127.0.0.1:{}:127.0.0.1:{} -L 127.0.0.1:{}:127.0.0.1:{} -L 127.0.0.1:{}:127.0.0.1:{} {}",
            cfg.local_http_port,
            DEFAULT_HTTP_PORT,
            cfg.local_ssh_port,
            DEFAULT_SSH_PORT,
            cfg.local_vault_port,
            DEFAULT_VAULT_PORT,
            cfg.local_webhook_port,
            DEFAULT_WEBHOOK_PORT,
            cfg.target
        );
        return Ok(0);
    }
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg("-N");
    push_local_forward(&mut cmd, cfg.local_http_port, DEFAULT_HTTP_PORT);
    push_local_forward(&mut cmd, cfg.local_ssh_port, DEFAULT_SSH_PORT);
    push_local_forward(&mut cmd, cfg.local_vault_port, DEFAULT_VAULT_PORT);
    push_local_forward(&mut cmd, cfg.local_webhook_port, DEFAULT_WEBHOOK_PORT);
    cmd.arg(&cfg.target);
    run_interactive_ssh(cmd, "ssh tunnel", "opening ssh tunnel").await
}

pub(crate) fn push_local_forward(cmd: &mut Command, local_port: u16, remote_port: u16) {
    cmd.arg("-L");
    cmd.arg(format!(
        "127.0.0.1:{}:127.0.0.1:{}",
        local_port, remote_port
    ));
}

pub(crate) async fn remote_uninstall(
    cfg: &RemoteConfig,
    opts: &RemoteCommonOptions,
) -> Result<i32> {
    print_action_envelope(
        opts,
        serde_json::json!({
            "action": "remote-uninstall",
            "alias": cfg.alias,
            "target": cfg.target,
            "dry_run": opts.dry_run,
        }),
    )?;
    if opts.dry_run {
        return Ok(0);
    }
    match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            let cmd = "systemctl --user disable --now jeryu.service >/dev/null 2>&1 || true; rm -f \"$HOME/.jeryu/bin/jeryu\" \"$HOME/.config/systemd/user/jeryu.service\"; systemctl --user daemon-reload";
            run_remote_shell(cfg, &cmd, false).await?;
        }
        ServiceMode::Manual => {
            let cmd = "rm -f \"$HOME/.jeryu/bin/jeryu\"";
            run_remote_shell(cfg, &cmd, false).await?;
        }
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    }
    let _ = fs::remove_file(config_path(&cfg.alias));
    Ok(0)
}

pub(crate) async fn probe_remote(cfg: &RemoteConfig) -> Result<RemotePreflight> {
    let remote_os = run_remote_shell_capture(cfg, "uname -s").await?;
    let remote_arch = run_remote_shell_capture(cfg, "uname -m").await?;
    let docker_ready = run_remote_shell_status(cfg, "docker info >/dev/null 2>&1").await?;
    let systemd_user =
        run_remote_shell_status(cfg, "systemctl --user is-system-running >/dev/null 2>&1")
            .await
            .ok();
    let disk_free_gb = run_remote_shell_capture(
        cfg,
        "df -Pk \"$HOME\" | awk 'NR==2 { printf \"%.2f\", $4 / 1024 / 1024 }'",
    )
    .await?
    .and_then(|text| text.trim().parse::<f64>().ok());
    Ok(RemotePreflight {
        local_ssh: command_exists("ssh"),
        local_ssh_keygen: command_exists("ssh-keygen"),
        remote_os,
        remote_arch,
        remote_docker_ready: Some(docker_ready),
        remote_systemd_user: systemd_user,
        remote_disk_free_gb: disk_free_gb,
    })
}

pub(crate) async fn remote_bootstrap(cfg: &RemoteConfig) -> Result<()> {
    let _ = run_remote_binary(cfg, &["init"], false).await?;
    Ok(())
}

pub(crate) async fn manual_service_active(cfg: &RemoteConfig) -> Result<bool> {
    run_remote_shell_status(cfg, "pgrep -f 'jeryu serve' >/dev/null 2>&1").await
}

pub(crate) async fn ensure_remote_service(cfg: &RemoteConfig) -> Result<()> {
    let unit = format!(
        r#"[Unit]
Description=JeRyu remote control plane
After=network-online.target

[Service]
Type=simple
ExecStart=%h/.jeryu/bin/jeryu serve
WorkingDirectory=%h/.jeryu
Restart=always
RestartSec=5

[Install]
WantedBy=default.target
"#
    );
    let script = format!(
        "mkdir -p \"$HOME/.config/systemd/user\" \"$HOME/.jeryu/bin\" \"$HOME/.jeryu\" && cat > \"$HOME/.config/systemd/user/jeryu.service\" <<'EOF'\n{}\nEOF\nsystemctl --user daemon-reload\nsystemctl --user enable --now jeryu.service",
        unit
    );
    run_remote_shell(cfg, &script, false).await
}

pub(crate) async fn collect_report(cfg: &RemoteConfig) -> Result<RemoteReport> {
    let binary_output = run_remote_binary(cfg, &["--version"], true).await?;
    let docker_ready = run_remote_shell_status(cfg, "docker info >/dev/null 2>&1").await?;
    let service_active = match resolve_service_mode(cfg).await? {
        ServiceMode::User => {
            run_remote_shell_status(cfg, "systemctl --user is-active jeryu.service").await?
        }
        ServiceMode::Manual => manual_service_active(cfg).await?,
        ServiceMode::Auto => panic!("resolved service mode should never be Auto"),
    };
    Ok(RemoteReport {
        alias: cfg.alias.clone(),
        target: cfg.target.clone(),
        config_path: config_path(&cfg.alias).display().to_string(),
        remote_prefix: cfg.remote_prefix.clone(),
        remote_bin: cfg.remote_bin.clone(),
        installed: binary_output.is_some(),
        service_active,
        docker_ready,
        version_output: binary_output,
    })
}

pub(crate) fn print_manual_service_guidance(cfg: &RemoteConfig) {
    println!("manual service guidance for {}:", cfg.alias);
    println!("  - keep {} available on the remote host", cfg.remote_bin);
    println!("  - run: {} serve", cfg.remote_bin);
    println!("  - if you want a user unit later, create ~/.config/systemd/user/jeryu.service");
}

pub(crate) async fn ensure_remote_key(cfg: &RemoteConfig, setup_key: bool) -> Result<()> {
    if !setup_key {
        return Ok(());
    }
    let identity = match cfg.identity.as_deref() {
        Some(identity) => PathBuf::from(identity),
        None => expand_tilde(format!("~/.ssh/jeryu_{}_ed25519", cfg.alias)),
    };
    if !identity.exists() {
        if let Some(parent) = identity.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let mut keygen = Command::new("ssh-keygen");
        keygen.args(["-t", "ed25519", "-f"]);
        keygen.arg(&identity);
        keygen.args(["-N", "", "-C", &format!("jeryu-{}", cfg.alias)]);
        crate::exec::run_status_check(&mut keygen, "ssh-keygen failed").await?;
    }
    let pubkey = fs::read_to_string(identity.with_extension("pub"))
        .with_context(|| format!("reading {}", identity.with_extension("pub").display()))?;
    let script = format!(
        "mkdir -p ~/.ssh && chmod 700 ~/.ssh && touch ~/.ssh/authorized_keys && grep -qxF -- {} ~/.ssh/authorized_keys || printf '%s\\n' {} >> ~/.ssh/authorized_keys",
        remote_shell::shell_single_quote(&pubkey.trim()),
        remote_shell::shell_single_quote(&pubkey.trim())
    );
    run_remote_shell(cfg, &script, false).await
}

pub(crate) async fn upload_current_binary(cfg: &RemoteConfig) -> Result<()> {
    let local = std::env::current_exe().context("locating current executable")?;
    let script = r#"mkdir -p "$HOME/.jeryu/bin" && cat > "$HOME/.jeryu/bin/jeryu.tmp" && install -m 0755 "$HOME/.jeryu/bin/jeryu.tmp" "$HOME/.jeryu/bin/jeryu" && rm -f "$HOME/.jeryu/bin/jeryu.tmp""#;
    let started = Instant::now();
    println!("uploading {} to {}...", local.display(), cfg.target);
    let bytes = fs::read(&local).with_context(|| format!("reading {}", local.display()))?;
    let mut cmd = remote_shell::ssh_bash_command(cfg, script);
    crate::exec::run_with_stdin(&mut cmd, &bytes, "ssh upload failed").await?;
    println!(
        "uploaded remote binary in {}s",
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

pub(crate) async fn run_remote_binary(
    cfg: &RemoteConfig,
    args: &[&str],
    allow_fail: bool,
) -> Result<Option<String>> {
    let mut cmd = Command::new("ssh");
    cmd.args(ssh_args(cfg));
    cmd.arg(&cfg.target);
    cmd.arg(&cfg.remote_bin);
    cmd.args(args);
    let output = cmd.output().await.context("running remote binary")?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    } else if allow_fail {
        Ok(None)
    } else {
        bail!(
            "remote binary exited with {}",
            output.status.code().unwrap_or(-1)
        );
    }
}

async fn run_interactive_ssh(
    mut cmd: Command,
    _label: &'static str,
    context_msg: &'static str,
) -> Result<i32> {
    crate::exec::run_status_check(&mut cmd, context_msg).await?;
    Ok(0)
}

pub(crate) async fn run_remote_shell(
    cfg: &RemoteConfig,
    script: &str,
    allow_fail: bool,
) -> Result<()> {
    let output = remote_shell::capture_ssh_bash_output(cfg, script, "running remote shell").await?;
    if output.status.success() || allow_fail {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", stderr.trim());
    }
}

pub(crate) async fn run_remote_shell_status(cfg: &RemoteConfig, script: &str) -> Result<bool> {
    let output =
        remote_shell::capture_ssh_bash_output(cfg, script, "running remote shell status").await?;
    Ok(output.status.success())
}

pub(crate) async fn run_remote_shell_capture(
    cfg: &RemoteConfig,
    script: &str,
) -> Result<Option<String>> {
    let output =
        remote_shell::capture_ssh_bash_output(cfg, script, "running remote shell capture").await?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()))
    } else {
        Ok(None)
    }
}
