//! Owner: Cache Gateway subsystem (module root)
//! Proof: `cargo nextest run -p jeryu -- gateway`
//! Invariants: Gateway modules preserve namespace isolation, singleflight behavior, and upstream recovery semantics.
pub mod cargo;
pub mod git;
pub mod npm;
pub mod oci;
pub mod singleflight;
