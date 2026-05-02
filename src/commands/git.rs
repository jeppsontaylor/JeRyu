use anyhow::Result;

pub fn execute_git_passthrough(args: &[String]) -> Result<()> {
    let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
    let is_push = args.first().map(|s| s.as_str()) == Some("push");
    let status = std::process::Command::new(&git_path).args(args).status()?;
    
    if is_push && status.success() {
        let remotes = std::process::Command::new(&git_path).args(["remote"]).output();
        if let Ok(out) = remotes {
            let remote_str = String::from_utf8_lossy(&out.stdout);
            if remote_str.lines().any(|l| l.trim() == "shadow") {
                println!("🪄 JeRyu: Automatically pushing to local shadow pipeline...");
                let _ = std::process::Command::new(&git_path)
                    .args(["push", "shadow", "HEAD"])
                    .status();
            }
        }
    }
    std::process::exit(status.code().unwrap_or(1));
}

pub fn execute_save(message: &str) -> Result<()> {
    println!("Saving work...");
    let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
    std::process::Command::new(&git_path).args(["add", "."]).status()?;
    let status = std::process::Command::new(&git_path).args(["commit", "-m", message]).status()?;
    if !status.success() {
        println!("Failed to save changes.");
    } else {
        println!("✅ Work saved locally.");
    }
    Ok(())
}

pub fn execute_sync() -> Result<()> {
    println!("Syncing with remote...");
    let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
    let pull_status = std::process::Command::new(&git_path).args(["pull", "--rebase"]).status()?;
    if pull_status.success() {
        let push_status = std::process::Command::new(&git_path).args(["push"]).status()?;
        if push_status.success() {
            println!("✅ Synced successfully.");
        }
    }
    Ok(())
}

pub fn execute_undo() -> Result<()> {
    println!("Undoing last save...");
    let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
    let status = std::process::Command::new(&git_path).args(["reset", "HEAD~1", "--soft"]).status()?;
    if status.success() {
        println!("✅ Last commit undone (changes kept in staging).");
    }
    Ok(())
}

pub fn execute_ship() -> Result<()> {
    println!("Shipping code...");
    let git_path = std::env::var("JERYU_SYSTEM_GIT").unwrap_or_else(|_| "/usr/bin/git".into());
    
    println!("Pushing to origin...");
    std::process::Command::new(&git_path).args(["push", "origin", "HEAD"]).status()?;
    
    println!("Promoting to local shadow runner...");
    let shadow_status = std::process::Command::new(&git_path)
        .args(["push", "shadow", "HEAD"])
        .status();
        
    match shadow_status {
        Ok(s) if s.success() => println!("✅ Shipped to remote and local shadow."),
        _ => println!("✅ Shipped to remote (local shadow skip/fail)."),
    }
    Ok(())
}
