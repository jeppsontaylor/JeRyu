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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn set_env_var<K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr>>(key: K, value: V) {
        // SAFETY: these tests serialize environment mutation with ENV_LOCK and
        // restore previous values before releasing the lock.
        unsafe {
            std::env::set_var(key, value);
        }
    }

    fn remove_env_var<K: AsRef<std::ffi::OsStr>>(key: K) {
        // SAFETY: these tests serialize environment mutation with ENV_LOCK and
        // restore previous values before releasing the lock.
        unsafe {
            std::env::remove_var(key);
        }
    }

    fn write_fake_ssh(bin_dir: &TempDir, log_path: &std::path::Path) {
        let script = r#"#!/bin/sh
set -eu

log_file=${FAKE_SSH_LOG:?}
printf '%s\n' "$*" >> "$log_file"

while [ "$#" -gt 0 ]; do
    case "$1" in
        -o|-i)
            shift 2
            ;;
        --)
            shift
            break
            ;;
        -*)
            shift
            ;;
        *)
            break
            ;;
    esac
done

target="${1:-}"
shift || true
cmd="$*"

case "$cmd" in
    *"uname -s"*)
        printf 'Linux\n'
        exit 0
        ;;
    *"uname -m"*)
        printf 'x86_64\n'
        exit 0
        ;;
    *"docker info"*)
        exit 0
        ;;
    *"systemctl --user is-system-running"*)
        exit 1
        ;;
    *"df -Pk"*)
        printf '86.39\n'
        exit 0
        ;;
    *"mkdir -p"*"jeryu.tmp"*"install -m 0755"*)
        cat >/dev/null
        exit 0
        ;;
    *"~/.jeryu/bin/jeryu --version"*)
        printf 'jeryu 1.0.0\n'
        exit 0
        ;;
    *"~/.jeryu/bin/jeryu init"*)
        echo "unexpected init bootstrap invocation" >&2
        exit 99
        ;;
    *)
        echo "unexpected ssh command for target ${target}: ${cmd}" >&2
        exit 98
        ;;
esac
"#
        .to_string();
        let ssh = bin_dir.path().join("ssh");
        std::fs::write(&ssh, script).unwrap();
        let mut perms = std::fs::metadata(&ssh).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&ssh, perms).unwrap();

        let _ = std::fs::write(log_path, "");
    }

    fn write_fake_ssh_keygen(bin_dir: &TempDir) {
        let keygen = bin_dir.path().join("ssh-keygen");
        std::fs::write(&keygen, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&keygen).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&keygen, perms).unwrap();
    }

    fn sample_remote_config(alias: &str) -> RemoteConfig {
        RemoteConfig {
            connection: build_remote_connection(
                alias.to_string(),
                format!("{alias}@example.com"),
                2222,
                None,
            ),
            created_at_utc: "2026-05-12T00:00:00Z".into(),
            service_mode: ServiceMode::Auto,
        }
    }

    #[tokio::test]
    async fn remote_install_without_setup_key_skips_bootstrap_and_succeeds() {
        let _guard = crate::test_sync::PATH_ENV_LOCK.lock().unwrap();
        let temp = TempDir::new().unwrap();
        let bin_dir = TempDir::new().unwrap();
        let log_path = temp.path().join("ssh.log");
        write_fake_ssh(&bin_dir, &log_path);
        write_fake_ssh_keygen(&bin_dir);

        let original_path = std::env::var_os("PATH");
        let original_home = std::env::var_os("HOME");
        let mut path_entries = vec![bin_dir.path().to_path_buf()];
        if let Some(path) = &original_path {
            path_entries.extend(std::env::split_paths(path));
        }
        set_env_var("PATH", std::env::join_paths(path_entries).unwrap());
        set_env_var("HOME", temp.path());
        set_env_var("FAKE_SSH_LOG", &log_path);

        let cfg = sample_remote_config("ci-sshd");
        let opts = RemoteCommonOptions {
            dry_run: false,
            json: false,
            yes: true,
            color: ColorMode::Never,
            interactive: InteractiveMode::Never,
            service_mode: ServiceMode::Manual,
            verbose: false,
        };

        let result = remote_install(cfg, false, &opts).await;

        match original_path {
            Some(value) => set_env_var("PATH", value),
            None => remove_env_var("PATH"),
        }
        match original_home {
            Some(value) => set_env_var("HOME", value),
            None => remove_env_var("HOME"),
        }
        remove_env_var("FAKE_SSH_LOG");

        assert!(result.is_ok(), "{result:?}");
        let log = std::fs::read_to_string(&log_path).unwrap();
        assert!(log.contains("uname -s"));
        assert!(log.contains("uname -m"));
        assert!(log.contains("docker info"));
        assert!(log.contains("~/.jeryu/bin/jeryu --version"));
        assert!(
            !log.contains(" init"),
            "remote install should not bootstrap init"
        );
        assert!(
            temp.path().join(".jeryu/remotes/ci-sshd.toml").exists(),
            "remote install should persist metadata"
        );
    }
}

#[path = "remote_ops_support.rs"]
mod support;
pub(crate) use support::*;
