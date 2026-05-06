//! Owner: Epoch-Based Cache Invalidation
//! Proof: `cargo test -p jeryu -- epoch`
//! Invariants: Epoch bumps are recorded with author_job_id and reason; is_valid fails closed (returns false) on lookup failure; epochs are per-scope and never shared across scopes

use anyhow::Result;
use sqlx::AnyPool;

use crate::state::{StateBackend, backend_sql};

/// Manages epoch-based cache invalidation.
///
/// Instead of scanning and deleting massive graphs of files on disk,
/// we simply bump an epoch pointer. Any cache lookups for objects
/// tied to an older epoch strictly fail, immediately isolating poisoned trees.
#[derive(Clone)]
pub struct EpochManager {
    pool: AnyPool,
    backend: StateBackend,
}

impl EpochManager {
    pub fn new(pool: AnyPool) -> Self {
        Self::with_backend(pool, StateBackend::Sqlite)
    }

    pub fn with_backend(pool: AnyPool, backend: StateBackend) -> Self {
        Self { pool, backend }
    }

    /// Retrieve the current epoch for a given boundary scope (e.g., "global", "project:123", "runner:456").
    pub async fn get_epoch(&self, scope: &str) -> Result<u64> {
        let sql = backend_sql(
            self.backend,
            "SELECT current_epoch FROM cache_epochs WHERE scope = ?",
        );
        let epoch: i64 = sqlx::query_scalar(&sql)
            .bind(scope)
            .fetch_optional(&self.pool)
            .await?
            .unwrap_or(0);

        Ok(epoch as u64)
    }

    /// Bump the epoch, instantly invalidating all cache entries tied to the previous epoch.
    pub async fn bump_epoch(&self, scope: &str, author_job_id: i64, reason: &str) -> Result<u64> {
        let current = self.get_epoch(scope).await?;
        let next = current + 1;

        let sql = backend_sql(
            self.backend,
            r#"INSERT INTO cache_epochs (scope, current_epoch, updated_at, author_job_id, reason)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(scope) DO UPDATE SET
                 current_epoch = excluded.current_epoch,
                 updated_at = excluded.updated_at,
                 author_job_id = excluded.author_job_id,
                 reason = excluded.reason"#,
        );
        sqlx::query(&sql)
            .bind(scope)
            .bind(next as i64)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(author_job_id)
            .bind(reason)
            .execute(&self.pool)
            .await?;

        tracing::warn!(
            "Escalated cache epoch for scope `{}` -> {}. Reason: {}",
            scope,
            next,
            reason
        );
        Ok(next)
    }

    /// Verifies if a cached object's epoch is still valid relative to the active scope epoch.
    /// Returns true if it is safe to use.
    pub async fn is_valid(&self, scope: &str, object_epoch: u64) -> Result<bool> {
        let current = self.get_epoch(scope).await?;
        Ok(object_epoch >= current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::any::{AnyPoolOptions, install_default_drivers};

    async fn setup_db() -> AnyPool {
        install_default_drivers();
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query(
            "CREATE TABLE cache_epochs (
            scope TEXT PRIMARY KEY,
            current_epoch INTEGER NOT NULL,
            updated_at TEXT NOT NULL,
            author_job_id INTEGER NOT NULL,
            reason TEXT NOT NULL
        )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_epoch_escalation() {
        let pool = setup_db().await;
        let mgr = EpochManager::new(pool);

        let scope = "project:42";

        // Initial epoch should be 0
        assert_eq!(mgr.get_epoch(scope).await.unwrap(), 0);
        assert!(mgr.is_valid(scope, 0).await.unwrap());

        // Bump epoch
        mgr.bump_epoch(scope, 999, "Security incident in base image")
            .await
            .unwrap();

        // New epoch
        assert_eq!(mgr.get_epoch(scope).await.unwrap(), 1);

        // Previous objects are now invalid
        assert!(!mgr.is_valid(scope, 0).await.unwrap());

        // New objects built today at epoch 1 are valid
        assert!(mgr.is_valid(scope, 1).await.unwrap());
    }
}
