use crate::cli::SecretsCommands;
use anyhow::Result;
use jeryu::{secrets, state};

pub(crate) async fn execute_secrets_commands(subcmd: SecretsCommands) -> Result<()> {
    let db = state::Db::open().await?;
    match subcmd {
        SecretsCommands::Init => {
            let report = secrets::run_secrets_init(Some(&db)).await?;
            println!(
                "Vault initialized at {} (mount={}, prefix={})",
                report.addr, report.mount, report.prefix
            );
        }
        SecretsCommands::Status { json } => {
            let report = secrets::vault_status(Some(&db)).await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("━━━ jeryu secrets status ━━━");
                println!("  Vault:       {}", report.addr);
                println!("  Initialized: {}", report.initialized);
                println!("  Sealed:      {}", report.sealed);
                println!("  Healthy:     {}", report.healthy);
                println!("  Token:       {}", report.token_present);
                println!("  Mount:       {}", report.mount);
                println!("  Prefix:      {}", report.prefix);
                println!("  Bootstrap:   {}", report.bootstrap_file);
                println!("  Env file:    {}", report.env_file);
                if let Some(secret_set) = db
                    .latest_release_secret_set(&crate::cli::infer_repo_name())
                    .await?
                {
                    println!("\n  Latest release secret set:");
                    println!("    Version:   {}", secret_set.version);
                    println!("    Target:    {}", secret_set.target);
                    println!("    Status:    {}", secret_set.status);
                    println!("    Runtime:   {}", secret_set.rendered_runtime_env_path);
                    if let Some(report_path) = secret_set.report_path {
                        println!("    Report:    {}", report_path);
                    }
                }
            }
        }
        SecretsCommands::Rotate {
            repo,
            version,
            target,
        } => {
            let target = target.parse::<secrets::SecretTarget>()?;
            let (repo_root, deploy_env, runtime_env) = secrets::default_release_paths();
            let outcome = secrets::rotate_release_secrets(
                &db,
                &repo_root,
                &repo,
                &version,
                target,
                &deploy_env,
                &runtime_env,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&outcome)?);
        }
        SecretsCommands::Finalize {
            repo,
            version,
            target,
        } => {
            let target = target.parse::<secrets::SecretTarget>()?;
            let (repo_root, deploy_env, runtime_env) = secrets::default_release_paths();
            let path = secrets::finalize_release_secrets(
                &db,
                &repo_root,
                &repo,
                &version,
                target,
                &deploy_env,
                &runtime_env,
            )
            .await?;
            println!("Finalized runtime env: {}", path.display());
        }
        SecretsCommands::Report { repo, version } => {
            let (repo_root, _, _) = secrets::default_release_paths();
            let path =
                secrets::build_release_secret_report(&db, &repo_root, &repo, &version).await?;
            println!("Release report: {}", path.display());
        }
        SecretsCommands::Recover { repo, version } => {
            let (repo_root, _, _) = secrets::default_release_paths();
            secrets::recover_release_secrets(&db, &repo_root, &repo, &version).await?;
        }
    }
    Ok(())
}
