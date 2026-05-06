//! Owner: Epoch DB Adapter (sqlx-backed cache_epochs table access)
//! Proof: `cargo test -p epoch-adapter`
//! Invariants: All SQL/sqlx access for the cache_epochs table lives here; callers see only the trait.
//!
//! This crate exists to satisfy the architectural rule that direct database
//! access (sqlx + raw SQL) must live under `crates/adapters/` (or `db/`),
//! not in the application layer (`src/`).

use anyhow::Result;
use async_trait::async_trait;

/// Re-export of `sqlx::AnyPool` so the application layer never has to name `sqlx`
/// directly when only a pool handle is required.
pub use sqlx::AnyPool;

/// Backend kind for SQL bind-parameter dialect rewriting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterBackend {
    Sqlite,
    Postgres,
}

/// Adapter trait the application uses to query and update the cache_epochs table.
///
/// Implementations are responsible for any DB-specific dialect handling
/// (e.g. rewriting `?` to `$N` for Postgres) and for the actual SQL.
#[async_trait]
pub trait EpochStore: Send + Sync {
    /// Retrieve the current epoch for the given scope, returning 0 if no row exists.
    async fn get_epoch(&self, scope: &str) -> Result<u64>;

    /// Upsert the epoch for the given scope to `next_epoch`, recording author and reason.
    async fn set_epoch(
        &self,
        scope: &str,
        next_epoch: u64,
        author_job_id: i64,
        reason: &str,
    ) -> Result<()>;
}

/// sqlx-backed implementation of [`EpochStore`].
pub struct SqlxEpochStore {
    pool: sqlx::AnyPool,
    backend: AdapterBackend,
}

impl SqlxEpochStore {
    pub fn new(pool: sqlx::AnyPool, backend: AdapterBackend) -> Self {
        Self { pool, backend }
    }

    /// Construct as an `Arc<dyn EpochStore>` suitable for the application layer.
    pub fn boxed(
        pool: sqlx::AnyPool,
        backend: AdapterBackend,
    ) -> std::sync::Arc<dyn EpochStore> {
        std::sync::Arc::new(Self::new(pool, backend))
    }

    fn rewrite(&self, sql: &str) -> String {
        match self.backend {
            AdapterBackend::Sqlite => sql.to_string(),
            AdapterBackend::Postgres => postgres_bind_params(sql),
        }
    }
}

fn postgres_bind_params(sql: &str) -> String {
    let mut converted = String::with_capacity(sql.len() + 16);
    let mut next = 1usize;
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
impl EpochStore for SqlxEpochStore {
    async fn get_epoch(&self, scope: &str) -> Result<u64> {
        let sql = self.rewrite("SELECT current_epoch FROM cache_epochs WHERE scope = ?");
        let epoch: Option<i64> = sqlx::query_scalar(&sql)
            .bind(scope)
            .fetch_optional(&self.pool)
            .await?;
        Ok(epoch.unwrap_or(0) as u64)
    }

    async fn set_epoch(
        &self,
        scope: &str,
        next_epoch: u64,
        author_job_id: i64,
        reason: &str,
    ) -> Result<()> {
        let sql = self.rewrite(
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
            .bind(next_epoch as i64)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(author_job_id)
            .bind(reason)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
