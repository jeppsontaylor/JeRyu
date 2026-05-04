//! Owner: Git event plane and passthrough executor
//! Proof: `cargo test -p jeryu -- git_`
//! Invariants: Git runs exactly once per command invocation; records are additive and redacted.

pub mod classify;
pub mod event;
pub mod executor;
pub mod invocation;
pub mod mirror;
pub mod policy;
pub mod receipt;
pub mod shim;
pub mod snapshot;
pub mod store;
pub mod system;

pub use classify::{GitCommandClass, GitRisk, classify_argv};
pub use event::GitCommandEvent;
pub use executor::execute_git;
pub use invocation::GitInvocation;
pub use policy::GitMode;
pub use snapshot::GitSnapshot;
