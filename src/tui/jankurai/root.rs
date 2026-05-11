use std::path::{Path, PathBuf};

pub(crate) fn repo_root_from_runtime() -> Result<PathBuf, String> {
    if let Ok(root) = repo_root_from_current_dir() {
        return Ok(root);
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
        && let Some(root) = repo_root_from(parent)
    {
        return Ok(root);
    }

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR")
        && let Some(root) = repo_root_from(Path::new(&manifest_dir))
    {
        return Ok(root);
    }

    Err("could not locate repository root from current directory, executable, or CARGO_MANIFEST_DIR".into())
}

pub(crate) fn fs_read_optional(path: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(Some(raw)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.to_string()),
    }
}

pub(crate) fn is_jankurai_installed() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let candidates = if cfg!(windows) {
        vec!["jankurai.exe", "jankurai.cmd", "jankurai.bat", "jankurai"]
    } else {
        vec!["jankurai"]
    };

    for dir in std::env::split_paths(&path) {
        for candidate in &candidates {
            let probe = dir.join(candidate);
            if probe.is_file() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = probe.metadata()
                        && meta.permissions().mode() & 0o111 != 0
                    {
                        return true;
                    }
                }
                #[cfg(not(unix))]
                {
                    return true;
                }
            }
        }
    }

    false
}

fn repo_root_from_current_dir() -> Result<PathBuf, String> {
    let current = std::env::current_dir().map_err(|err| err.to_string())?;
    match repo_root_from(&current) {
        Some(root) => Ok(root),
        None => Err(format!(
            "could not find Cargo.toml while walking up from {}",
            current.display()
        )),
    }
}

fn repo_root_from(start: &Path) -> Option<PathBuf> {
    let mut cargo_roots = Vec::new();
    let mut workspace_root = None;

    for ancestor in start.ancestors() {
        if ancestor.join("Cargo.toml").is_file() {
            if has_repo_agent_artifacts(ancestor) {
                return Some(ancestor.to_path_buf());
            }
            if workspace_root.is_none() && cargo_manifest_declares_workspace(ancestor) {
                workspace_root = Some(ancestor.to_path_buf());
            }
            cargo_roots.push(ancestor.to_path_buf());
        }
    }

    match workspace_root {
        Some(root) => Some(root),
        None => cargo_roots.into_iter().next(),
    }
}

fn has_repo_agent_artifacts(root: &Path) -> bool {
    root.join("agent/JANKURAI_STANDARD.md").is_file()
        || root.join("agent/generated-zones.toml").is_file()
        || root.join("agent/repo-score.json").is_file()
}

fn cargo_manifest_declares_workspace(root: &Path) -> bool {
    match std::fs::read_to_string(root.join("Cargo.toml")) {
        Ok(raw) => raw.lines().any(|line| line.trim() == "[workspace]"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn repo_root_prefers_workspace_agent_artifacts_over_nested_member_manifest() {
        let repo_dir = tempfile::tempdir().expect("repo dir");
        fs::write(
            repo_dir.path().join("Cargo.toml"),
            "[workspace]\nmembers=['crates/member']\n",
        )
        .unwrap();
        fs::create_dir_all(repo_dir.path().join("agent")).unwrap();
        fs::write(
            repo_dir.path().join("agent/JANKURAI_STANDARD.md"),
            "# standard\n",
        )
        .unwrap();

        let member_dir = repo_dir.path().join("crates/member/src");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(
            repo_dir.path().join("crates/member/Cargo.toml"),
            "[package]\nname='member'\nversion='0.1.0'\n",
        )
        .unwrap();

        let root = repo_root_from(&member_dir).expect("repo root");
        assert_eq!(root, repo_dir.path());
    }
}
