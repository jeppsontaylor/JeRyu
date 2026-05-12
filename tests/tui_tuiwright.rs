//! Owner: Interactive TUI subsystem — Tuiwright black-box integration tests
//! Proof: `TERM=xterm-256color cargo test --test tui_tuiwright -- --test-threads=1`
//! Invariants: Each test spawns a real PTY session; tests are serial to avoid port contention.

use std::path::Path;
use std::time::Duration;
use tuiwright::{Page, SpawnConfig};

/// Locate the `jeryu` binary built by cargo.
fn jeryu_bin() -> String {
    // When run via `cargo test`, CARGO_BIN_EXE_jeryu is set automatically.
    match std::env::var("CARGO_BIN_EXE_jeryu") {
        Ok(path) => path,
        Err(_) => {
            // Fallback: look in target/debug
            let manifest = std::env::var("CARGO_MANIFEST_DIR")
                .expect("CARGO_MANIFEST_DIR must be set by cargo");
            format!("{manifest}/target/debug/jeryu")
        }
    }
}

fn spawn_tui(tab: &str) -> anyhow::Result<Page> {
    let bin = jeryu_bin();
    let page = Page::spawn(
        SpawnConfig::new(&bin)
            .arg("tui")
            .arg("--screenshot")
            .arg("--tab")
            .arg(tab)
            .arg("--screenshot-hold-ms")
            .arg("10000")
            .size(120, 36)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .timeout(Duration::from_secs(8)),
    )?;
    // Wait for the TUI to finish its first render.
    std::thread::sleep(Duration::from_millis(800));
    Ok(page)
}

fn fake_jankurai_path() -> anyhow::Result<(tempfile::TempDir, String)> {
    let dir = tempfile::tempdir()?;
    let script_name = if cfg!(windows) {
        "jankurai.cmd"
    } else {
        "jankurai"
    };
    let script_path = dir.path().join(script_name);
    #[cfg(windows)]
    std::fs::write(&script_path, "@echo off\r\nexit /b 0\r\n")?;
    #[cfg(not(windows))]
    std::fs::write(&script_path, "#!/bin/sh\nexit 0\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&script_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script_path, perms)?;
    }

    let mut paths = vec![dir.path().to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    let path = std::env::join_paths(paths)?;
    Ok((dir, path.to_string_lossy().into_owned()))
}

fn spawn_tui_with_path(tab: &str, path: &str, cwd: &Path) -> anyhow::Result<Page> {
    let bin = jeryu_bin();
    let page = Page::spawn(
        SpawnConfig::new(&bin)
            .cwd(cwd)
            .arg("tui")
            .arg("--screenshot")
            .arg("--tab")
            .arg(tab)
            .arg("--screenshot-hold-ms")
            .arg("10000")
            .size(120, 36)
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .env("PATH", path)
            .timeout(Duration::from_secs(8)),
    )?;
    std::thread::sleep(Duration::from_millis(800));
    Ok(page)
}

fn fixture_jankurai_repo() -> anyhow::Result<tempfile::TempDir> {
    let repo_dir = tempfile::tempdir()?;
    std::fs::write(
        repo_dir.path().join("Cargo.toml"),
        "[workspace]\nmembers=[]\n",
    )?;
    std::fs::create_dir_all(repo_dir.path().join("agent"))?;
    std::fs::write(
        repo_dir.path().join("agent/repo-score.json"),
        r#"{
            "generated_at":"2026-05-11T12:00:00Z",
            "score":64,
            "raw_score":82,
            "finding_count":1,
            "hard_findings":1,
            "soft_findings":0,
            "decision":{"status":"advisory","minimum_score":85,"hard_findings":1,"soft_findings":0},
            "conformance_decision":"block",
            "dimensions":[
                {"name":"Fixture determinism","weight":7,"score":64,"weighted_points":4.48,"evidence":["fixture score file loaded"],"notes":["stable test data"]}
            ],
            "caps_applied":["fixture-cap"],
            "findings":[
                {"severity":"high","hardness":"hard","path":"agent/generated-zones.toml","problem":"generated zone fixture finding","agent_fix":"keep screenshot tests on fixture artifacts","evidence":["fixture generated zone evidence"],"rule_id":"fixture-generated-zone","lane":"audit","owner":"agent"}
            ]
        }"#,
    )?;
    std::fs::write(
        repo_dir.path().join("agent/score-history.jsonl"),
        r#"{"generated_at":"2026-05-10T12:00:00Z","score":61,"raw_score":80,"decision":"advisory"}
{"generated_at":"2026-05-11T12:00:00Z","score":64,"raw_score":82,"decision":"block"}
"#,
    )?;
    Ok(repo_dir)
}

// ── Test: Workflow tab renders on startup ────────────────────────────────

#[test]
fn workflow_tab_renders_header_and_content() -> anyhow::Result<()> {
    let page = spawn_tui("workflow")?;

    // The header bar must show the Workflow tab label.
    page.wait_for_text("Workflow", Duration::from_secs(5))?;

    // The demo fixture renders phase rows with "Phase 0" visible.
    page.wait_for_text("Phase 0", Duration::from_secs(3))?;

    // Take a screenshot for visual inspection.
    std::fs::create_dir_all("target/tuiwright")?;
    page.screenshot("target/tuiwright/workflow-default.png")?;

    Ok(())
}

// ── Test: Workflow tab shows demo node labels ───────────────────────────

#[test]
fn workflow_demo_shows_node_labels() -> anyhow::Result<()> {
    let page = spawn_tui("workflow")?;

    // Demo snapshot includes "cargo check" and "VTI plan" nodes.
    page.wait_for_text("cargo check", Duration::from_secs(5))?;

    // The summary banner should show status glyphs.
    let screen = page.screen();
    let text = screen.plain_text();

    // Verify at least one status glyph is present (✓ for passed nodes).
    assert!(
        text.contains('✓') || text.contains("RAN"),
        "expected passed node marker in workflow view"
    );

    page.screenshot("target/tuiwright/workflow-nodes.png")?;
    Ok(())
}

// ── Test: Mission tab renders ───────────────────────────────────────────

#[test]
fn mission_tab_renders() -> anyhow::Result<()> {
    let page = spawn_tui("mission")?;

    page.wait_for_text("Mission", Duration::from_secs(5))?;

    page.screenshot("target/tuiwright/mission.png")?;
    Ok(())
}

// ── Test: Jobs tab renders ──────────────────────────────────────────────

#[test]
fn jobs_tab_renders() -> anyhow::Result<()> {
    let page = spawn_tui("jobs")?;
    // The Jobs tab shows a Pipeline Progress panel in its content area.
    page.wait_for_text("Pipeline", Duration::from_secs(5))?;

    page.screenshot("target/tuiwright/jobs.png")?;
    Ok(())
}

// ── Test: Jank tab renders when jankurai is available on PATH ──────────

#[test]
fn jank_tab_renders_when_tool_is_available() -> anyhow::Result<()> {
    let (_dir, path) = fake_jankurai_path()?;
    let repo_dir = fixture_jankurai_repo()?;
    let page = spawn_tui_with_path("jank", &path, repo_dir.path())?;

    page.wait_for_text("Jankurai Summary", Duration::from_secs(5))?;
    page.wait_for_text("Jank", Duration::from_secs(5))?;
    page.wait_for_text("Score History", Duration::from_secs(5))?;
    page.wait_for_text("PATH: installed", Duration::from_secs(5))?;
    page.wait_for_text(
        "findings: 1 total / 0 hard / 1 soft",
        Duration::from_secs(5),
    )?;

    std::fs::create_dir_all("target/tuiwright")?;
    page.screenshot("target/tuiwright/jank.png")?;
    Ok(())
}

// ── Test: Screenshot is deterministic PNG ───────────────────────────────

#[test]
fn screenshot_produces_valid_png() -> anyhow::Result<()> {
    let page = spawn_tui("workflow")?;
    page.wait_for_text("Workflow", Duration::from_secs(5))?;

    let path = "target/tuiwright/workflow-deterministic.png";
    std::fs::create_dir_all("target/tuiwright")?;
    page.screenshot(path)?;

    // Verify the file exists and has valid PNG header.
    let data = std::fs::read(path)?;
    assert!(data.len() > 100, "PNG file too small");
    assert_eq!(&data[1..4], b"PNG", "not a valid PNG file");

    Ok(())
}
