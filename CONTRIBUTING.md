# Contributing to jeryu

First off, thanks for taking the time to contribute! 🎉

The following is a set of guidelines for contributing to jeryu. These are mostly guidelines, not rules. Use your best judgment, and feel free to propose changes to this document in a pull request.

## How Can I Contribute?

### Reporting Bugs
Bugs are tracked as GitHub issues. Create an issue and provide the following information:
* A quick summary of the issue
* Steps to reproduce
* The expected out and the actual output 
* OS and jeryu version

### Suggesting Enhancements
Enhancement suggestions are also tracked as GitHub issues. We love new ideas!

### Pull Requests
* Fill in the required template
* Do not include issue numbers in the PR title
* Include screenshots and animated GIFs in your pull request whenever possible
* End files with a newline
* Favor strict typing and immutability where possible in Rust

## Development Environment Setup

1. Install Rust (latest stable) via `rustup`
2. Install Docker (required to spawn the local GitLab test cluster)
3. Clone the repo and run: `cargo build`

### Tests
Make sure the entire suite passes before raising a PR:
```bash
# Formatter
cargo fmt --all -- --check

# Linter
cargo clippy --all-targets --all-features -- -D warnings

# Tests
cargo test --all
```
