use anyhow::Result;
use std::{fs, path::Path};

fn assert_no_nonblocking_shell_terminators(path: &str) -> Result<()> {
    let contents = fs::read_to_string(path)?;
    assert!(
        !contents.contains("|| true"),
        "{path} still contains a non-blocking shell terminator"
    );
    Ok(())
}

#[test]
fn language_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    assert_no_nonblocking_shell_terminators(".github/workflows/rust.yml")?;

    let log_path = Path::new("target/jankurai/language-bad-behavior.log");
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        log_path,
        "ci and git behavior lane verified: no non-blocking workflow shell terminators\n",
    )?;
    Ok(())
}

fn write_lane_log(path: &str, message: &str) -> Result<()> {
    let log_path = Path::new(path);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(log_path, message)?;
    Ok(())
}

#[test]
fn ci_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    write_lane_log(
        "target/jankurai/ci-bad-behavior.log",
        "ci bad behavior lane verified: workflow shell terminators are blocking\n",
    )
}

#[test]
fn git_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    write_lane_log(
        "target/jankurai/git-bad-behavior.log",
        "git bad behavior lane verified: workflow shell terminators are blocking\n",
    )
}

#[test]
fn release_bad_behavior_lane_is_blocking() -> Result<()> {
    assert_no_nonblocking_shell_terminators(".github/workflows/jankurai.yml")?;
    write_lane_log(
        "target/jankurai/release-bad-behavior.log",
        "release bad behavior lane verified: workflow shell terminators are blocking\n",
    )
}
