# tui — Ratatui TUI Dashboard

## Invariants

- `run_tui_once` is the smoke-test entry point — must render without panicking on empty state.
- All tab variants must be covered in `renders_all_primary_tabs_with_empty_state`.
- No business logic — all data via `state::Db` through `App::refresh_now()`.

## Proof Commands

```bash
cargo check -p jeryu --message-format=json
cargo test -p jeryu -- tui
```

Change type: `leaf-bugfix`. Promote to `cross-module` if `app.rs` state types change.
