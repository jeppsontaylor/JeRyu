//! Owner: Git execution receipts
//! Proof: `cargo test -p jeryu -- git_receipt`
//! Invariants: Receipts describe outcomes without exposing secrets or raw tokens.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitReceipt {
    pub request_id: String,
    pub exit_code: i32,
    pub mirror_status: String,
}

impl GitReceipt {
    pub fn success(request_id: impl Into<String>, mirror_status: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            exit_code: 0,
            mirror_status: mirror_status.into(),
        }
    }
}
