# test_intel — VTI Smart Test Selection

Maps changed files to the minimal test set needed to validate a change.
Reads `dougx/.jeryu/testmap.toml` (shared map — JeRyu never writes it).

## Modules

| Module | Responsibility |
|---|---|
| `subsystem.rs` | Subsystem graph, path → owner resolution |
| `testmap.rs` | Parses `.jeryu/testmap.toml` |
| `planner.rs` | Changed files → deterministic test plan |
| `cache.rs` | Caches plans across runs by testmap hash |
| `ci_gen.rs` | Emits GitLab CI pipeline fragments |
| `nightly.rs` | Nightly full-sweep oracle |
| `explain.rs` | Human-readable plan explanation |

## Invariants

- Never write `dougx/.jeryu/testmap.toml`.
- Planner output is deterministic for identical inputs.
- Cache invalidates on testmap hash change.

## Proof Commands

```bash
cargo check -p jeryu --message-format=json
cargo test -p jeryu -- test_intel
```

Change type: `api-change` (see `proof-lanes.toml [module_hints]`).
