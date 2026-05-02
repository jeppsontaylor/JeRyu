//! Owner: Cache Decision Brain (Trust + Taint + Epoch Integration)
//! Proof: `cargo test -p jeryu -- cache_brain`
//! Invariants: Cache hits require matching trust tier; tainted objects never produce hits; epoch mismatch forces miss; all three checks must pass before a hit is returned

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::epoch::EpochManager;
use crate::explain::{CacheVerdict, MissReason};
use crate::policy::{PolicyEngine, TrustTier};
use crate::state::{StateBackend, backend_sql};
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
    pool: sqlx::AnyPool,
    backend: StateBackend,
}

impl CacheBrain {
    pub fn new(
        epoch_manager: EpochManager,
        taint_manager: TaintManager,
        pool: sqlx::AnyPool,
    ) -> Self {
        Self::with_backend(epoch_manager, taint_manager, pool, StateBackend::Sqlite)
    }

    pub fn with_backend(
        epoch_manager: EpochManager,
        taint_manager: TaintManager,
        pool: sqlx::AnyPool,
        backend: StateBackend,
    ) -> Self {
        Self {
            epoch_manager,
            taint_manager,
            pool,
            backend,
        }
    }

    /// Evaluates whether a generic build unit can reuse work, relying on trust boundaries,
    /// taint states, and epoch invalidation logic.
    pub async fn plan_step(&self, unit: &BuildUnit) -> Result<CacheVerdict> {
        let sql = backend_sql(
            self.backend,
            "SELECT namespace, created_at FROM action_cache WHERE action_key = ?",
        );
        let row: Option<(String, String)> = sqlx::query_as(&sql)
            .bind(&unit.input_signature)
            .fetch_optional(&self.pool)
            .await?;

        let namespace = match row {
            Some((ns, _)) => ns,
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
