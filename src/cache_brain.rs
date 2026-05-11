//! Owner: Cache Decision Brain (Trust + Taint + Epoch Integration)
//! Proof: `cargo test -p jeryu -- cache_brain`
//! Invariants: Cache hits require matching trust tier; tainted objects never produce hits; epoch mismatch forces miss; all three checks must pass before a hit is returned
//!
//! All persistent storage access is delegated to `cache-brain-adapter` to keep
//! this module free of direct database surface (per the architectural rule
//! HLT-006-DIRECT-DB-WRONG-LAYER).

use std::sync::Arc;

use anyhow::Result;
use cache_brain_adapter::ActionCacheStore;
use serde::{Deserialize, Serialize};

use crate::epoch::EpochManager;
use crate::explain::{CacheVerdict, MissReason};
use crate::policy::{PolicyEngine, TrustTier};
use crate::taint::TaintManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BuildUnitType {
    DockerBuild {
        stage: String,
    },
    CargoBuild {
        target: String,
        profile: String,
        features: String,
    },
    GenericStep {
        name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildUnit {
    pub unit_type: BuildUnitType,
    pub input_signature: String,
    pub environment_signature: String,
    pub scope: String,
    pub trust_tier: TrustTier,
}

pub struct CacheBrain {
    epoch_manager: EpochManager,
    taint_manager: TaintManager,
    store: Arc<dyn ActionCacheStore>,
}

impl CacheBrain {
    /// Construct a `CacheBrain` from an adapter store. The store carries any
    /// concrete database surface; this module is intentionally backend-free.
    pub fn with_store(
        epoch_manager: EpochManager,
        taint_manager: TaintManager,
        store: Arc<dyn ActionCacheStore>,
    ) -> Self {
        Self {
            epoch_manager,
            taint_manager,
            store,
        }
    }

    /// Evaluates whether a generic build unit can reuse work, relying on trust boundaries,
    /// taint states, and epoch invalidation logic.
    pub async fn plan_step(&self, unit: &BuildUnit) -> Result<CacheVerdict> {
        let entry = self.store.lookup(&unit.input_signature).await?;

        let namespace = match entry {
            Some(e) => e.namespace,
            None => {
                return Ok(CacheVerdict::Miss {
                    reasons: vec![MissReason::NoLocalCache],
                });
            }
        };

        let current_epoch = self.epoch_manager.get_epoch(&unit.scope).await?;
        if !self
            .epoch_manager
            .is_valid(&unit.scope, current_epoch)
            .await?
        {
            return Ok(CacheVerdict::Miss {
                reasons: vec![MissReason::ForcedEpochBump {
                    scope: unit.scope.clone(),
                    epoch: current_epoch,
                }],
            });
        }

        if self.taint_manager.is_tainted(&unit.input_signature).await? {
            return Ok(CacheVerdict::Denied {
                policy: "Artifact belongs to a tainted subgraph (Detonation Lane breached)"
                    .to_string(),
            });
        }

        let candidate_tier = match namespace.as_str() {
            "trusted" => TrustTier::Trusted,
            _ => TrustTier::Untrusted,
        };

        if !PolicyEngine::can_consume(&candidate_tier, &unit.trust_tier) {
            return Ok(CacheVerdict::Miss {
                reasons: vec![MissReason::TrustNamespaceMismatch],
            });
        }

        Ok(CacheVerdict::HitExact)
    }
}
