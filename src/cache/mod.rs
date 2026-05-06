//! Owner: SmartCache & Disk Management
//! Proof: `cargo test -p jeryu -- cache`
//! Invariants: LRU GC every 30 min; active-manager caches never collected; CAS atomic store

use anyhow::{Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tracing::info;
use walkdir::WalkDir;

const MIN_GITLAB_ARTIFACT_SIZE_MB: u64 = 4096;
const POOL_TARGET_LEASE_RECOVERY_TTL_SECS: u64 = 2 * 60 * 60;
const NEXTEST_EXTRACT_RECENT_TTL_SECS: u64 = 2 * 60 * 60;
#[allow(dead_code)]
const NEXTEST_EXTRACT_FALLBACK_TTL_SECS: u64 = NEXTEST_EXTRACT_RECENT_TTL_SECS;

mod types;
mod runtime;

#[path = "../cache_reports.rs"]
mod cache_reports;

pub use cache_reports::*;
pub use runtime::*;
pub use types::*;
