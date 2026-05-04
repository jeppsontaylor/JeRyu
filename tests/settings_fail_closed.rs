use std::fs;
use std::sync::{LazyLock, Mutex};

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn set_env(key: &str, value: &str) {
    unsafe {
        std::env::set_var(key, value);
    }
}

#[test]
fn corrupt_settings_fail_closed_and_backup() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp_home = tempfile::tempdir().unwrap();
    let settings_dir = temp_home.path().join(".jeryu");
    fs::create_dir_all(&settings_dir).unwrap();
    let settings_path = settings_dir.join("settings.json");
    fs::write(&settings_path, "{not valid json").unwrap();

    let home = temp_home.path().to_str().unwrap().to_string();
    set_env("HOME", &home);

    let err = jeryu::settings::load().unwrap_err();
    let backup_exists = fs::read_dir(&settings_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("settings.json.bad.")
        });

    assert!(
        backup_exists,
        "expected a timestamped backup next to settings.json"
    );
    assert!(err.to_string().contains("settings.json parse error"));
}
