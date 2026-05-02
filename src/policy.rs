//! Owner: Trust Policy (TrustTier, Cache Promotion Gates)
//! Proof: `cargo test -p vgit -- policy`
//! Invariants: PolicyEngine::can_consume is the single gate for cache promotion across trust boundaries; higher-trust tier may not consume lower-trust output without explicit approval

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrustTier {
    Trusted,
    #[default]
    Untrusted,
    Quarantine,
}

pub struct PolicyEngine;

impl PolicyEngine {
    /// Determines whether a cached object produced by `source_tier` can be safely
    /// consumed by `target_tier` without a promotion approval.
    pub fn can_consume(source_tier: &TrustTier, target_tier: &TrustTier) -> bool {
        match (source_tier, target_tier) {
            // Quarantine artifacts can never be consumed
            (TrustTier::Quarantine, _) => false,
            // A trusted artifact can be consumed by anyone
            (TrustTier::Trusted, _) => true,
            // An untrusted artifact can only be consumed by other untrusted builders
            (TrustTier::Untrusted, TrustTier::Untrusted) => true,
            (TrustTier::Untrusted, TrustTier::Trusted) => false,
            // You can consume untrusted if you're in quarantine (though why would you)
            (TrustTier::Untrusted, TrustTier::Quarantine) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trust_policy_boundaries() {
        assert!(PolicyEngine::can_consume(
            &TrustTier::Trusted,
            &TrustTier::Trusted
        ));
        assert!(PolicyEngine::can_consume(
            &TrustTier::Trusted,
            &TrustTier::Untrusted
        ));

        assert!(PolicyEngine::can_consume(
            &TrustTier::Untrusted,
            &TrustTier::Untrusted
        ));
        assert!(!PolicyEngine::can_consume(
            &TrustTier::Untrusted,
            &TrustTier::Trusted
        ));

        assert!(!PolicyEngine::can_consume(
            &TrustTier::Quarantine,
            &TrustTier::Trusted
        ));
        assert!(!PolicyEngine::can_consume(
            &TrustTier::Quarantine,
            &TrustTier::Untrusted
        ));
    }
}
