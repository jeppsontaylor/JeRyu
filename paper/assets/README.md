# Paper Assets

This directory contains publication figures and generated screenshots.

Generated screenshots for V3.01:

- `vgit-tui-mission.png`
- `vgit-tui-release.png`
- `vgit-tui-jobs-flow.png`
- `vgit-tui-agents.png`
- `vgit-tui-tests-vti.png`
- `vgit-tui-evidence.png`

Use `scripts/capture-tui-screenshots.sh` to render high-resolution, full-terminal
PNG screenshots for publication. The script runs `vgit tui --screenshot` inside a
real PTY, parses the terminal state with `vt100`, and rasterizes the full grid
with `tui-capture` using a pinned DejaVu Sans Mono font, fixed geometry, and a
brightened paper-friendly palette. This avoids browser/ANSI converter glyph
fallback and captures the alternate screen before the TUI exits.

```bash
./scripts/capture-tui-screenshots.sh
just tui-screenshot-smoke
```

For deterministic, non-interactive CI comparisons, the legacy deterministic
renderer remains available as:

```bash
vgit tui --capture --tab <tab> --output paper/assets/<name>.png
```
