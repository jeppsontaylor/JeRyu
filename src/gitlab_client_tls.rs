pub(crate) fn insecure_tls_enabled_from_env() -> bool {
    std::env::var("JERYU_GITLAB_INSECURE_TLS")
        .ok()
        .is_some_and(|value| insecure_tls_enabled_from_value(&value))
}

pub(crate) fn insecure_tls_enabled_from_value(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitlab_client::GitlabClient;
    use reqwest::Method;

    #[test]
    fn insecure_tls_is_opt_in_only() {
        assert!(!insecure_tls_enabled_from_value(""));
        assert!(!insecure_tls_enabled_from_value("false"));
        assert!(!insecure_tls_enabled_from_value("0"));
        assert!(insecure_tls_enabled_from_value("1"));
        assert!(insecure_tls_enabled_from_value("true"));
    }

    #[test]
    fn client_constructor_keeps_explicit_tls_policy() {
        let secure = GitlabClient::new_with_tls_policy("http://localhost:8929/", None, false);
        assert_eq!(secure.base_url, "http://localhost:8929");
        let insecure = GitlabClient::new_with_tls_policy("http://localhost:8929/", None, true);
        assert_eq!(insecure.base_url, "http://localhost:8929");
    }

    #[test]
    fn authed_request_url_preserves_gitlab_prefix_and_token_header() {
        let client = GitlabClient::new_with_tls_policy(
            "http://localhost:8929/",
            Some("pat123".into()),
            false,
        );
        let request = client
            .authed_request_url(Method::POST, client.api_url("/projects/42"))
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(request.method(), Method::POST);
        assert_eq!(
            request.url().as_str(),
            "http://localhost:8929/api/v4/projects/42"
        );
        assert_eq!(request.headers().get("PRIVATE-TOKEN").unwrap(), "pat123");
    }
}
