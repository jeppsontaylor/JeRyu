<div align="center">
  <pre>
      __     ___                 
   _ / /___ / _ \__ __ __ __     
  // // -_) |   / // // // /     
 \___/\__/__|_\_\_, / \_,_/      
               /___/             
  </pre>
  <h3>The Git-Compatible Version Control Layer for the AI Era</h3>
</div>

---

`JeRyu` is a single-binary Rust control plane that seamlessly wraps Git, injecting autonomous AI orchestration without breaking your existing muscle memory or tooling. It provides a phased migration path from traditional Git workflows to an intelligent, agent-driven CI/CD ecosystem.

## 🚀 The JeRyu Promise

The product strategy is simple: **JeRyu wraps Git first, then replaces it.**

1. **Passthrough Layer**: Type `git status` or `git commit`. The command flows through JeRyu, wrapping the system's Git binary securely. Your muscle memory is preserved.
2. **Native Wrappers**: Start using `jeryu save` (add + commit), `jeryu sync` (pull --rebase + push), and `jeryu undo`.
3. **Dual-Use Sync**: Type `jeryu ship` to instantly push to your normal origin remote AND a local `shadow` CI remote simultaneously, kicking off isolated AI pipelines effortlessly.

## ⚡ Quickstart

Get your autonomous environment running in less than 60 seconds.

Our interactive installer works flawlessly on macOS and Linux. It will automatically check for dependencies (Git, Rust, Build Tools) and offer to install them for you.

```bash
# Clone the repository
git clone git@github.com:jeppsontaylor/JeRyu.git
cd JeRyu

# Run the interactive installer
chmod +x install.sh
./install.sh
```

During installation, you will be prompted to select a global installation (`/usr/local/bin`) or a local one (`~/.cargo/bin`).

### 🛠 The Shell Shim

To achieve the seamless Git compatibility layer, add this shim to your `~/.bashrc` or `~/.zshrc`:

```bash
git() {
    if command -v jeryu >/dev/null 2>&1; then
        command jeryu git "$@"
    else
        command git "$@"
    fi
}
```

Reload your shell (`source ~/.bashrc`) and type `git status`. You're now running JeRyu!

## 🪄 Magic Commands Reference

JeRyu introduces several "magic" commands designed to simplify version control into a fast, intent-driven experience:

- `jeryu save "message"`: Instantly stages all changes and commits them locally.
- `jeryu sync`: Pulls from remote with rebase, and pushes local changes back up.
- `jeryu undo`: Rolls back the last commit but keeps all changes staged and ready.
- `jeryu ship`: Pushes your branch to your primary `origin` remote AND automatically promotes it to your local runner's `shadow` remote to execute AI validation logic.
- `jeryu system`: Opens the JeRyu system dashboard to monitor runner pools, Vault, and the GitLab control plane.
- `jeryu tui`: Launches the powerful JeRyu Terminal User Interface for live pipeline monitoring.

## 🏗 Architecture

JeRyu isn't just a CLI; it's a massive autonomous control plane built on a modern async Rust stack:

- **Tokio**: High-concurrency runtime.
- **SQLx + Postgres/SQLite**: State persistence for concurrent agent fleets.
- **Agent Intelligence**: VTI smart test selection, shadow remotes, and risk-gated merge decisions.
- **Fail-Closed Sandbox**: Strict network isolation using `bwrap`/`unshare` to isolate untrusted AI tasks safely.

For a full breakdown of the engine, read our [Architecture Guide](docs/ARCHITECTURE.md).
