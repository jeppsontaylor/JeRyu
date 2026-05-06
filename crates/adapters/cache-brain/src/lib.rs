//! Owner: Cache Brain DB Adapter (sqlx-backed action_cache lookup)
//! Proof: `cargo test -p cache-brain-adapter`
//! Invariants: All SQL/sqlx access for the action_cache table lives here; callers see only the trait.
//!
//! This crate exists to satisfy the architectural rule that direct database
//! access (sqlx + raw SQL) must live under `crates/adapters/` (or `db/`),
//! not in the application layer (`src/`).

use anyhow::Result;
use async_trait::async_trait;

/// Backend kind for SQL bind-parameter dialect rewriting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterBackend {
    Sqlite,
    Postgres,
}

/// A row in the `action_cache` table mapped to its trust namespace.
#[derive(Debug, Clone)]
pub struct ActionCacheEntry {
    pub namespace: String,
}

/// Adapter trait the application uses to query the action_cache.
///
/// Implementations are responsible for any DB-specific dialect handling
/// (e.g. rewriting `?` to `$N` for Postgres) and for the actual SQL.
#[async_trait]
pub trait ActionCacheStore: Send + Sync {
    /// Look up an action_cache row by its action key (input signature).
    async fn lookup(&self, action_key: &str) -> Result<Option<ActionCacheEntry>>;
}

/// sqlx-backed implementation of `ActionCacheStore`.
///
/// Holds an `sqlx::AnyPool` and emits the canonical
/// `SELECT namespace, created_at FROM action_cache WHERE action_key = ?`
/// query, rewriting bind parameters for Postgres when needed.
pub struct SqlxActionCacheStore {
    pool: sqlx::AnyPool,
    backend: AdapterBackend,
}

impl SqlxActionCacheStore {
    pub fn new(pool: sqlx::AnyPool, backend: AdapterBackend) -> Self {
        Self { pool, backend }
    }

    /// Construct as a trait object suitable for handing to the application layer.
    pub fn boxed(
        pool: sqlx::AnyPool,
        backend: AdapterBackend,
    ) -> std::sync::Arc<dyn ActionCacheStore> {
        std::sync::Arc::new(Self::new(pool, backend))
    }
}

fn postgres_bind_params(sql: &str) -> String {
    let mut converted = String::with_capacity(sql.len() + 16);
    let mut next = 1;
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = sql.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => {
                converted.push(ch);
                if in_single && chars.peek() == Some(&'\'') {
                    converted.push(chars.next().expect("peeked escaped quote"));
                } else {
                    in_single = !in_single;
                }
            }
            '"' if !in_single => {
                in_double = !in_double;
                converted.push(ch);
            }
            '?' if !in_single && !in_double => {
                converted.push('$');
                converted.push_str(&next.to_string());
                next += 1;
            }
            _ => converted.push(ch),
        }
    }

    converted
}

#[async_trait]
impl ActionCacheStore for SqlxActionCacheStore {
    async fn lookup(&self, action_key: &str) -> Result<Option<ActionCacheEntry>> {
        let base = "SELECT namespace, created_at FROM action_cache WHERE action_key = ?";
        let sql = match self.backend {
            AdapterBackend::Sqlite => base.to_string(),
            AdapterBackend::Postgres => postgres_bind_params(base),
        };
        let row: Option<(String, String)> = sqlx::query_as(&sql)
            .bind(action_key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(namespace, _)| ActionCacheEntry { namespace }))
    }
}

/// Count active rows in the `cache_taints` table.
///
/// Lives in this adapter so the application layer never issues raw SQL.
/// The query takes no bind parameters, so dialect rewriting is not required.
pub async fn count_active_cache_taints(pool: &sqlx::AnyPool) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cache_taints")
        .fetch_one(pool)
        .await?;
    Ok(count)
}
