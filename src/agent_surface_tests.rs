use super::*;

#[test]
fn header_value_extracts_doc_headers() {
    let body = "//! Owner: Agent Surface\n//! Proof: cargo check\n//! Invariants: keep routing derivable\n";
    assert_eq!(
        header_value(body, "Owner").as_deref(),
        Some("Agent Surface")
    );
    assert_eq!(header_value(body, "Proof").as_deref(), Some("cargo check"));
}

#[test]
fn header_value_or_empty_defaults_to_empty_string() {
    let body = "//! Owner: Agent Surface\n";
    assert_eq!(header_value_or_empty(body, "Proof"), "");
}

#[test]
fn module_change_type_honors_hints() {
    let proof = ProofLanesFile {
        lane: BTreeMap::new(),
        change_type: BTreeMap::new(),
        module_hints: BTreeMap::from([
            ("test_intel/".to_string(), "api-change".to_string()),
            ("secrets.rs".to_string(), "security-relevant".to_string()),
        ]),
    };
    assert_eq!(
        module_change_type("src/test_intel/mod.rs", &proof),
        "api-change"
    );
    assert_eq!(
        module_change_type("src/secrets.rs", &proof),
        "security-relevant"
    );
}

#[test]
fn proof_lanes_for_change_type_defaults_to_empty_vec() {
    let proof = ProofLanesFile {
        lane: BTreeMap::new(),
        change_type: BTreeMap::new(),
        module_hints: BTreeMap::new(),
    };
    assert!(proof_lanes_for_change_type(&proof, "leaf-bugfix").is_empty());
}
