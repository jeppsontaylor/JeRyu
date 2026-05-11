//! Owner: CLI Git wrappers
//! Proof: `cargo test -p jeryu -- git_passthrough`
//! Invariants: These wrappers never terminate the process directly; `main` owns final exit handling.

use anyhow::Result;
use jeryu::{git, state};

fn code_from_result(code: i32) -> i32 {
    code
}

pub async fn execute_git_passthrough(db: Option<&state::Db>, args: &[String]) -> Result<i32> {
    git::executor::execute_git(db, args).await
}

pub async fn execute_save(db: Option<&state::Db>, message: &str) -> Result<i32> {
    println!("Saving work...");
    let add_code = git::executor::execute_git(db, &["add".into(), ".".into()]).await?;
    if add_code != 0 {
        println!("Failed to stage changes.");
        return Ok(code_from_result(add_code));
    }

    let commit_code =
        git::executor::execute_git(db, &["commit".into(), "-m".into(), message.into()]).await?;
    if commit_code == 0 {
        println!("✅ Work saved locally.");
    } else {
        println!("Failed to save changes.");
    }
    Ok(code_from_result(commit_code))
}

pub async fn execute_sync(db: Option<&state::Db>) -> Result<i32> {
    println!("Syncing with remote...");
    let pull_code = git::executor::execute_git(db, &["pull".into(), "--rebase".into()]).await?;
    if pull_code != 0 {
        return Ok(code_from_result(pull_code));
    }

    let push_code = git::executor::execute_git(db, &["push".into()]).await?;
    if push_code == 0 {
        println!("✅ Synced successfully.");
    }
    Ok(code_from_result(push_code))
}

pub async fn execute_undo(db: Option<&state::Db>) -> Result<i32> {
    println!("Undoing last save...");
    let code =
        git::executor::execute_git(db, &["reset".into(), "HEAD~1".into(), "--soft".into()]).await?;
    if code == 0 {
        println!("✅ Last commit undone (changes kept in staging).");
    }
    Ok(code)
}
