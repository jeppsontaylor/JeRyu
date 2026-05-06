use super::*;

pub(crate) fn parse_capability_request(bytes: &[u8]) -> anyhow::Result<ParsedCapabilityRequest> {
    if bytes.len() >= 4 {
        let frame_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        if frame_len > 0 && frame_len <= MAX_CAPABILITY_FRAME_BYTES && bytes.len() == frame_len + 4
        {
            let request = serde_json::from_slice::<AgentActionRequest>(&bytes[4..])?;
            return Ok(ParsedCapabilityRequest::Enveloped(Box::new(request)));
        }
    }

    if let Ok(request) = serde_json::from_slice::<AgentActionRequest>(bytes) {
        return Ok(ParsedCapabilityRequest::Enveloped(Box::new(request)));
    }

    let intent = serde_json::from_slice::<AgentIntent>(bytes)?;
    Ok(ParsedCapabilityRequest::Bridge(intent))
}

pub(crate) fn validate_capability_request(
    parsed: ParsedCapabilityRequest,
) -> anyhow::Result<(AgentIntent, CapabilityContext)> {
    match parsed {
        ParsedCapabilityRequest::Bridge(intent) => Ok((intent, CapabilityContext::bridge())),
        ParsedCapabilityRequest::Enveloped(request) => {
            if request.protocol_version != CAPABILITY_PROTOCOL_VERSION {
                anyhow::bail!(
                    "unsupported protocol_version '{}', expected '{}'",
                    request.protocol_version,
                    CAPABILITY_PROTOCOL_VERSION
                );
            }
            if request.request_id.trim().is_empty() {
                anyhow::bail!("request_id is required");
            }
            if request.actor.trim().is_empty() {
                anyhow::bail!("actor is required");
            }
            if request.nonce.trim().is_empty() {
                anyhow::bail!("nonce is required");
            }
            if let Some(expires_at) = &request.expires_at {
                let expiry = chrono::DateTime::parse_from_rfc3339(expires_at)
                    .map_err(|e| anyhow::anyhow!("invalid expires_at: {e}"))?;
                if expiry.with_timezone(&chrono::Utc) <= chrono::Utc::now() {
                    anyhow::bail!("request expired");
                }
            }

            let cache = SEEN_NONCES.get_or_init(|| Mutex::new(HashSet::new()));
            let mut seen = cache
                .lock()
                .map_err(|_| anyhow::anyhow!("nonce cache unavailable"))?;
            let nonce_key = format!("{}:{}", request.actor, request.nonce);
            if !seen.insert(nonce_key) {
                anyhow::bail!("replayed nonce");
            }
            if seen.len() > 4096 {
                seen.clear();
            }

            let request = *request;
            let ctx = CapabilityContext {
                request_id: request.request_id,
                actor: request.actor,
                protocol_version: request.protocol_version,
                bridge_mode: false,
            };
            Ok((request.intent, ctx))
        }
    }
}
