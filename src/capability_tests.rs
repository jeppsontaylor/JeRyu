use super::*;

#[test]
fn typed_ci_yaml_rejects_unknown_scope() {
    let err = dynamic_ci_yaml("unit; rm -rf /").unwrap_err().to_string();
    assert!(err.contains("unsupported test scope"));
}

#[test]
fn typed_ci_yaml_emits_command_list() {
    let yaml = dynamic_ci_yaml("lint").unwrap();
    assert!(yaml.contains("dynamic-lint-job"));
    assert!(yaml.contains("cargo clippy --all-targets"));
    assert!(yaml.contains("cargo fmt -- --check"));
}

#[test]
fn parses_framed_enveloped_request() {
    let request = AgentActionRequest {
        protocol_version: CAPABILITY_PROTOCOL_VERSION.to_string(),
        request_id: format!("req-{}", uuid::Uuid::new_v4()),
        actor: "agent:test".to_string(),
        nonce: uuid::Uuid::new_v4().to_string(),
        expires_at: Some((chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()),
        project_id: Some(1),
        base_ref: Some("main".to_string()),
        base_sha: Some("abc".to_string()),
        idempotency_key: None,
        budget: None,
        grant: None,
        intent: AgentIntent::ListAllowedActions,
    };
    let body = serde_json::to_vec(&request).unwrap();
    let mut framed = (body.len() as u32).to_be_bytes().to_vec();
    framed.extend(body);
    let parsed = parse_capability_request(&framed).unwrap();
    let (intent, ctx) = validate_capability_request(parsed).unwrap();
    assert!(matches!(intent, AgentIntent::ListAllowedActions));
    assert!(!ctx.bridge_mode);
    assert_eq!(ctx.actor, "agent:test");
}

#[test]
fn expired_enveloped_request_is_rejected() {
    let request = AgentActionRequest {
        protocol_version: CAPABILITY_PROTOCOL_VERSION.to_string(),
        request_id: format!("req-{}", uuid::Uuid::new_v4()),
        actor: "agent:test".to_string(),
        nonce: uuid::Uuid::new_v4().to_string(),
        expires_at: Some((chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339()),
        project_id: Some(1),
        base_ref: None,
        base_sha: None,
        idempotency_key: None,
        budget: None,
        grant: None,
        intent: AgentIntent::ListAllowedActions,
    };
    let parsed = ParsedCapabilityRequest::Enveloped(Box::new(request));
    let err = validate_capability_request(parsed).unwrap_err().to_string();
    assert!(err.contains("request expired"));
}
