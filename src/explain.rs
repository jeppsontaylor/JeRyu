//! Owner: Pipeline Explain subsystem
//! Proof: `cargo nextest run -p jeryu -- explain`
//! Invariants: Explanations are derived from recorded state and do not mutate control-plane data.
use serde::{Deserialize, Serialize};

/// Indicates the outcome of a fetch or execution cache evaluation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CacheVerdict {
    /// The exact required output was found and fully restored without execution.
    HitExact,
    /// Partial reuse was achieved (e.g. some layers/crates cached, but others needed building).
    HitPartial {
        reused: Vec<String>,
        rebuilt: Vec<String>,
    },
    /// A rebuild was required. Includes the exact reasons *why* it was required.
    Miss { reasons: Vec<MissReason> },
    /// Caching was explicitly bypassed by user/system rule.
    Bypass { rule: String },
    /// Access to the cached object was denied due to trust, namespace, or quarantine policy.
    Denied { policy: String },
}

/// A structured reason why an action missed the cache.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MissReason {
    InputDigestChanged {
        component: String,
        old: String,
        new: String,
    },
    ToolchainChanged {
        field: String,
        old: String,
        new: String,
    },
    EnvChanged {
        var: String,
    },
    LockfileChanged,
    BaseImageDigestChanged {
        tag: String,
        old_digest: String,
        new_digest: String,
    },
    BuildScriptRerunTriggered {
        path: String,
    },
    ForcedEpochBump {
        scope: String,
        epoch: u64,
    },
    TrustNamespaceMismatch,
    SecretEpochChanged {
        id: String,
    },
    NotCacheable {
        reason: String,
    },
    NoLocalCache,
}

impl CacheVerdict {
    pub fn is_hit(&self) -> bool {
        matches!(self, Self::HitExact | Self::HitPartial { .. })
    }
}
