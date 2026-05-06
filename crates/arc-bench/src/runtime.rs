#![allow(clippy::ptr_arg)]
use std::collections::HashMap;
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::runtime::Builder;
use tokio::sync::{mpsc, oneshot};
use tokio::task::LocalSet;

use crate::model::{BenchVariantResult, ScenarioReport};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RuntimeVariant {
    Baseline,
    ActorAsync,
}

impl RuntimeVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            RuntimeVariant::Baseline => "baseline-locks",
            RuntimeVariant::ActorAsync => "actor-async",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "baseline-locks" => Ok(Self::Baseline),
            "actor-async" => Ok(Self::ActorAsync),
            _ => anyhow::bail!("unknown runtime variant: {value}"),
        }
    }
}

pub fn run(output: &std::path::Path) -> Result<ScenarioReport> {
    let exe = std::env::current_exe().context("failed to locate arc-bench executable")?;
    let variants = [RuntimeVariant::Baseline, RuntimeVariant::ActorAsync];
    let mut results = Vec::new();
    for variant in variants {
        let output = Command::new(&exe)
            .arg("internal-runtime")
            .arg("--variant")
            .arg(variant.as_str())
            .arg("--ops")
            .arg("12000")
            .arg("--workers")
            .arg("4")
            .arg("--key-space")
            .arg("1024")
            .output()
            .with_context(|| format!("failed to run {}", variant.as_str()))?;
        if !output.status.success() {
            anyhow::bail!(
                "runtime child failed for {}: {}",
                variant.as_str(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let result: BenchVariantResult = serde_json::from_slice(&output.stdout)
            .with_context(|| format!("failed to parse runtime output for {}", variant.as_str()))?;
        results.push(result);
    }

    let report = ScenarioReport {
        scenario: "runtime".to_string(),
        generated_at: Utc::now().format("%Y-%m-%d").to_string(),
        results,
        cases: Vec::new(),
        notes: vec![
            "The baseline uses a shared Mutex<HashMap<..>> across worker threads.".to_string(),
            "The actor-async variant uses a current-thread Tokio runtime with an actor-owned state map.".to_string(),
            "This benchmark reports throughput and memory tradeoffs for the current workload.".to_string(),
        ],
    };
    std::fs::write(output, serde_json::to_string_pretty(&report)?)
        .with_context(|| format!("failed to write {}", output.display()))?;
    Ok(report)
}

pub fn run_internal(
    variant: RuntimeVariant,
    ops: usize,
    workers: usize,
    key_space: u64,
) -> Result<BenchVariantResult> {
    match variant {
        RuntimeVariant::Baseline => baseline_runtime(ops, workers, key_space),
        RuntimeVariant::ActorAsync => actor_runtime(ops, workers, key_space),
    }
}

fn baseline_runtime(ops: usize, workers: usize, key_space: u64) -> Result<BenchVariantResult> {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let shared = Arc::new(Mutex::new(HashMap::<u64, u64>::new()));
    let ops_per_worker = ops / workers.max(1);
    let start = Instant::now();
    let mut handles = Vec::new();
    for worker in 0..workers {
        let shared = Arc::clone(&shared);
        handles.push(thread::spawn(move || {
            let mut latencies = Vec::with_capacity(ops_per_worker);
            for step in 0..ops_per_worker {
                let key = ((worker * 37 + step) as u64) % key_space;
                let op_start = Instant::now();
                {
                    let mut map = shared.lock().expect("lock");
                    let entry = map.entry(key).or_insert(0);
                    *entry += 1;
                }
                latencies.push(op_start.elapsed().as_secs_f64() * 1000.0);
            }
            latencies
        }));
    }
    let mut latencies = Vec::new();
    for handle in handles {
        latencies.extend(handle.join().expect("worker panic"));
    }
    let wall = start.elapsed();
    Ok(build_result(
        RuntimeVariant::Baseline,
        wall,
        workers as u64,
        ops as f64 / wall.as_secs_f64(),
        latencies,
        vec!["Shared-state baseline with lock contention under concurrent mutation.".to_string()],
    ))
}

fn actor_runtime(ops: usize, workers: usize, key_space: u64) -> Result<BenchVariantResult> {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to build current-thread runtime")?;
    let local = LocalSet::new();
    let result = local.block_on(&runtime, async move {
        let (tx, mut rx) = mpsc::unbounded_channel::<(u64, oneshot::Sender<u64>)>();
        let actor = tokio::task::spawn_local(async move {
            let mut map = HashMap::<u64, u64>::new();
            while let Some((key, reply)) = rx.recv().await {
                let entry = map.entry(key).or_insert(0);
                *entry += 1;
                let _ = reply.send(*entry);
            }
        });
        let ops_per_worker = ops / workers.max(1);
        let start = Instant::now();
        let mut tasks = Vec::new();
        for worker in 0..workers {
            let tx = tx.clone();
            tasks.push(tokio::task::spawn_local(async move {
                let mut latencies = Vec::with_capacity(ops_per_worker);
                for step in 0..ops_per_worker {
                    let key = ((worker * 37 + step) as u64) % key_space;
                    let op_start = Instant::now();
                    let (reply_tx, reply_rx) = oneshot::channel();
                    tx.send((key, reply_tx)).expect("actor channel open");
                    let _ = reply_rx.await.expect("actor reply");
                    latencies.push(op_start.elapsed().as_secs_f64() * 1000.0);
                }
                latencies
            }));
        }
        drop(tx);
        let mut latencies = Vec::new();
        for task in tasks {
            latencies.extend(task.await.expect("task panic"));
        }
        actor.await.expect("actor panic");
        let wall = start.elapsed();
        build_result(
            RuntimeVariant::ActorAsync,
            wall,
            1,
            ops as f64 / wall.as_secs_f64(),
            latencies,
            vec![
                "Single-thread actor runtime with message-passing instead of shared locks."
                    .to_string(),
            ],
        )
    });
    Ok(result)
}

fn build_result(
    variant: RuntimeVariant,
    wall: std::time::Duration,
    thread_count_max: u64,
    throughput: f64,
    mut latencies: Vec<f64>,
    notes: Vec<String>,
) -> BenchVariantResult {
    BenchVariantResult {
        scenario: "runtime".to_string(),
        variant: variant.as_str().to_string(),
        wall_time_ms: wall.as_millis() as u64,
        peak_rss_kb: Some(peak_rss_kb()),
        thread_count_max: Some(thread_count_max),
        throughput: Some(throughput),
        latency_p50_ms: percentile(&mut latencies.clone(), 0.50),
        latency_p95_ms: percentile(&mut latencies, 0.95),
        context_files: None,
        context_bytes: None,
        selected_tests: None,
        selected_arcs: None,
        notes,
    }
}

fn percentile(values: &mut Vec<f64>, quantile: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((values.len() as f64 - 1.0) * quantile).round() as usize;
    values.get(index).copied()
}

fn peak_rss_kb() -> u64 {
    let mut usage = zero_rusage();
    // SAFETY: `usage` points to writable memory for `rusage`, and `getrusage`
    // initializes it before we inspect the value when the return status is zero.
    let status = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if status != 0 {
        return 0;
    }
    #[cfg(target_os = "macos")]
    {
        (usage.ru_maxrss as u64) / 1024
    }
    #[cfg(not(target_os = "macos"))]
    {
        usage.ru_maxrss as u64
    }
}

fn zero_rusage() -> libc::rusage {
    libc::rusage {
        ru_utime: Default::default(),
        ru_stime: Default::default(),
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    }
}
