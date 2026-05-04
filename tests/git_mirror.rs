use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::{LazyLock, Mutex};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn set_env(key: &str, value: &str) {
    unsafe {
        std::env::set_var(key, value);
    }
}

fn remove_env(key: &str) {
    unsafe {
        std::env::remove_var(key);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn push_mirror_failure_does_not_fail_primary_push() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let log_path = temp.path().join("invocations.log");
    let git_path = temp.path().join("fake-git.sh");
    let script = format!(
        "#!/usr/bin/env sh\nprintf '%s\\n' \"$*\" >> \"{}\"\nif [ \"$1\" = \"push\" ] && [ \"$2\" = \"shadow\" ]; then\n  exit 12\nfi\nexit 0\n",
        log_path.display()
    );
    fs::write(&git_path, script).unwrap();
    let mut perms = fs::metadata(&git_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&git_path, perms).unwrap();

    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(temp.path()).unwrap();
    set_env("JERYU_SYSTEM_GIT", git_path.to_str().unwrap());
    remove_env("JERYU_GIT_MODE");

    let db = jeryu::state::Db::open_memory().await.unwrap();
    let exit = jeryu::git::executor::execute_git(
        Some(&db),
        &["push".into(), "origin".into(), "HEAD".into()],
    )
    .await
    .unwrap();

    std::env::set_current_dir(cwd).unwrap();
    remove_env("JERYU_SYSTEM_GIT");

    assert_eq!(exit, 0);
    let invocations = fs::read_to_string(&log_path).unwrap();
    assert_eq!(invocations.lines().count(), 2);
    assert!(invocations.contains("push origin HEAD"));
    assert!(invocations.contains("push shadow HEAD"));
}
