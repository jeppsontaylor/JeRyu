# gateway — Registry Proxy (Cargo / Git / npm / OCI)

## Invariants

- All proxy modules route through `singleflight` for concurrent request deduplication.
- Never cache registry auth credentials — proxy response bodies only.

## Proof Commands

```bash
cargo check -p vgit --message-format=json
cargo test -p vgit -- gateway
```

Change type: `leaf-bugfix`. Promote to `api-change` if proxy endpoint config changes.
