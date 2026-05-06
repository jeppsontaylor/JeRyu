# Database

jeryu is a single-binary CLI control plane. Durable state is a local
SQLite (or in-memory) cache, declared inline at startup in
`src/state.rs`, `src/epoch.rs`, and `src/cache_brain.rs`. There is no
remote database, no application-owned transactional Postgres surface,
and no cross-runtime data contract.

This directory exists so the `[db]` boundary in `agent/boundaries.toml`
routes to real artifacts:

- `db/migrations/` — versioned schema migrations. Today a single
  `0001_inline_schema.sql` marker points at the in-code schema. When
  the schema is extracted into discrete files, replace the marker and
  continue numbering forward.
- `db/constraints/` — durable constraint declarations (PKs, UNIQUE,
  NOT NULL). Today a single marker file documents that constraints are
  inline with the table definitions in `src/state.rs`.

Adapter code that talks to sqlx lives in `src/cache_brain.rs` and
`src/state.rs`. If the cache backend ever grows into a real database
boundary, move sqlx use behind a typed adapter under
`crates/adapters/` and lift the inline schema into versioned files
here.
