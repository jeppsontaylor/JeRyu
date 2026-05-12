use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cargo_cache::{
    CargoCacheLayout, CargoLeaseRecord, CargoLeaseScan, LEASES_DIR_NAME, process_is_alive,
};

pub struct CargoLeaseGuard {
    path: PathBuf,
    parent_dir: PathBuf,
}

impl Drop for CargoLeaseGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
        let _ = fs::remove_dir(&self.parent_dir);
    }
}

pub fn write_lease(layout: &CargoCacheLayout) -> Result<Option<CargoLeaseGuard>> {
    let Some(lease_dir) = &layout.lease_dir else {
        return Ok(None);
    };

    let nonce = rand::random::<u64>();
    let path = lease_dir.join(format!("{}-{nonce:016x}.json", std::process::id()));
    let lease = CargoLeaseRecord {
        kind: "local-cargo".to_string(),
        scope_key: layout.scope_key.clone(),
        target_dir: layout.target_dir.display().to_string(),
        pid: std::process::id(),
        created_at: chrono::Utc::now().to_rfc3339(),
        rustc_key: layout.toolchain.rustc_key.clone(),
        rustc_version: layout.toolchain.rustc_version.clone(),
        host_triple: layout.toolchain.host_triple.clone(),
    };

    fs::create_dir_all(lease_dir).with_context(|| format!("creating {}", lease_dir.display()))?;
    fs::write(&path, serde_json::to_string_pretty(&lease)?)?;
    Ok(Some(CargoLeaseGuard {
        path,
        parent_dir: lease_dir.clone(),
    }))
}

pub fn lease_is_active(path: &Path) -> bool {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return false,
    };
    let Ok(record) = serde_json::from_str::<CargoLeaseRecord>(&raw) else {
        return false;
    };
    process_is_alive(record.pid)
}

pub fn scan_target_leases(target_dir: &Path) -> CargoLeaseScan {
    let lease_dir = target_dir.join(LEASES_DIR_NAME);
    let mut observed_files = 0;
    let mut stale_files = 0;
    let mut active = false;

    let Ok(entries) = fs::read_dir(&lease_dir) else {
        return CargoLeaseScan {
            active: false,
            observed_files: 0,
            stale_files: 0,
        };
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        observed_files += 1;
        if lease_is_active(&path) {
            active = true;
        } else {
            stale_files += 1;
            let _ = fs::remove_file(path);
        }
    }

    if !active && observed_files == stale_files {
        let _ = fs::remove_dir(&lease_dir);
    }

    CargoLeaseScan {
        active,
        observed_files,
        stale_files,
    }
}
