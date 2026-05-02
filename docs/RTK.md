# RTK - Rust Token Killer

RTK is the default shell entrypoint for `vgit` work. Use it to reduce terminal noise while keeping the failure boundary, job identifiers, and raw evidence recoverable.

## Default Rule

Prefix routine shell commands with `rtk`.

```bash
rtk git status
rtk cargo check -p vgit --message-format=json
rtk cargo test -p vgit --lib
rtk cargo run -p vgit -- progress --json
```

## When To Use `rtk proxy`

Use `rtk proxy` when you need unfiltered output from tools that stream progress or produce timing details RTK would compress.

```bash
rtk proxy docker build ...
rtk proxy docker compose up ...
rtk proxy journalctl -u gitlab-runner
rtk proxy gitlab-runner run ...
```

## Evidence Rules

RTK output must preserve:

- exit code
- failing command
- decisive output tail
- raw log path if output is spooled
- GitLab job IDs and pipeline IDs when present
- evidence file paths under `.vgit/`, release reports, or local ledgers

For CI and release investigation, prefer structured commands:

```bash
rtk cargo check -p vgit --message-format=json
rtk cargo run -p vgit -- pipeline jobs --json --pipeline-id <id>
rtk cargo run -p vgit -- progress --json
```

## Recovery

If RTK compresses too aggressively:

1. Re-run with `rtk proxy <cmd>`.
2. Pull the structured source of truth first: `vgit progress --json`, `vgit pipeline jobs --json`, `agent-index.json`.
3. Open raw logs only after you have the failing job or phase identified.

## Meta Commands

```bash
rtk gain
rtk gain --history
rtk proxy <cmd>
```

## Verification

```bash
rtk --version
rtk gain
which rtk
```
