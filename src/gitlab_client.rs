//! Owner: GitLab REST Client subsystem
//! Proof: `cargo nextest run -p jeryu -- gitlab_client`
//! Invariants: HTTP calls preserve GitLab semantics, redact tokens, and surface status-specific failures.
//! GitLab REST API client for jeryu.
//!
//! Thin, purpose-built wrapper around reqwest. Every method maps to
//! one GitLab REST endpoint. No magic.

use anyhow::{Context, Result};
use reqwest::{Client, Method};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[path = "gitlab_client_branches.rs"]
mod gitlab_client_branches;
#[path = "gitlab_client_issues.rs"]
mod gitlab_client_issues;
#[path = "gitlab_client_jobs.rs"]
mod gitlab_client_jobs;
#[path = "gitlab_client_merge_requests.rs"]
mod gitlab_client_merge_requests;
#[path = "gitlab_client_pipelines.rs"]
mod gitlab_client_pipelines;
#[path = "gitlab_client_projects.rs"]
mod gitlab_client_projects;
#[path = "gitlab_client_runners.rs"]
mod gitlab_client_runners;
#[path = "gitlab_client_tls.rs"]
mod gitlab_client_tls;
#[path = "gitlab_client_webhooks.rs"]
mod gitlab_client_webhooks;

// ---------------------------------------------------------------------------
// Request / Response types (all at module level for derive macro bridge)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct CreateProjectPatReq<'a> {
    name: &'a str,
    scopes: &'a [&'a str],
    access_level: i32,
    expires_at: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct ProjectPatResp {
    pub id: i64,
    pub name: String,
    pub token: String,
    pub user_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct RunnerCreated {
    pub id: i64,
    pub token: String,
}

#[derive(Serialize)]
struct CreateRunnerReq<'a> {
    description: &'a str,
    tag_list: &'a [&'a str],
    run_untagged: bool,
    runner_type: &'a str,
}

#[derive(Serialize)]
struct SetPausedReq {
    paused: bool,
}

#[derive(Debug, Deserialize)]
pub struct RunnerManager {
    pub system_id: Option<String>,
    pub status: Option<String>,
    pub contacted_at: Option<String>,
}

#[derive(Deserialize)]
struct ResetTokenResp {
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct Job {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub stage: String,
    #[serde(default)]
    pub allow_failure: bool,
    #[serde(skip)]
    pub pipeline_id: Option<i64>,
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub web_url: Option<String>,
    pub queued_duration: Option<f64>,
    pub duration: Option<f64>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub runner: Option<JobRunner>,
}

#[derive(Debug, Deserialize)]
pub struct JobRunner {
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Pipeline {
    pub id: i64,
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub status: String,
    pub web_url: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineBridge {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub downstream_pipeline: Option<PipelineRef>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineVariableValue {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct PipelineRef {
    pub id: i64,
    pub sha: Option<String>,
    #[serde(rename = "ref")]
    pub ref_name: Option<String>,
    pub status: Option<String>,
    pub web_url: Option<String>,
}

#[derive(Serialize)]
struct CreateWebhookReq<'a> {
    url: &'a str,
    token: &'a str,
    job_events: bool,
    pipeline_events: bool,
    push_events: bool,
    merge_requests_events: bool,
}

#[derive(Deserialize)]
struct WebhookResp {
    id: i64,
}

#[derive(Debug, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub path_with_namespace: String,
    pub web_url: String,
}

#[derive(Debug, Deserialize)]
pub struct Issue {
    pub id: i64,
    pub iid: i64,
    pub title: String,
    pub state: String,
    pub labels: Vec<String>,
    pub web_url: String,
}

#[derive(Serialize)]
struct CreateIssueReq<'a> {
    title: &'a str,
    description: &'a str,
    labels: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee_ids: Option<Vec<i64>>,
}

#[derive(Serialize)]
struct UpdateLabelsReq {
    labels: String,
}

#[derive(Serialize)]
struct NoteReq<'a> {
    body: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct MergeRequest {
    pub id: i64,
    pub iid: i64,
    pub title: String,
    pub state: String,
    pub web_url: String,
    pub source_branch: String,
    pub target_branch: String,
}

#[derive(Serialize)]
struct CreateMrReq<'a> {
    source_branch: &'a str,
    target_branch: &'a str,
    title: &'a str,
    description: &'a str,
    remove_source_branch: bool,
}

#[derive(Serialize)]
struct CreateBranchReq<'a> {
    branch: &'a str,
    #[serde(rename = "ref")]
    ref_name: &'a str,
}

#[derive(Serialize)]
struct CreateProjectReq<'a> {
    name: &'a str,
    visibility: &'a str,
    initialize_with_readme: bool,
}

#[derive(Serialize)]
struct CommitAction<'a> {
    action: &'a str,
    file_path: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct CreateCommitReq<'a> {
    branch: &'a str,
    commit_message: &'a str,
    actions: Vec<CommitAction<'a>>,
}

#[derive(Deserialize)]
struct CreateCommitResp {
    id: String,
}

#[derive(Serialize)]
struct CreatePipelineReq<'a> {
    #[serde(rename = "ref")]
    pub ref_name: &'a str,
    pub variables: Vec<PipelineVariable<'a>>,
}

#[derive(Serialize)]
struct PipelineVariable<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

#[derive(Deserialize)]
struct PipelineResp {
    pub id: i64,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct GitlabClient {
    base_url: String,
    client: Client,
    pat: Option<String>,
}

impl GitlabClient {
    pub fn new(base_url: &str, pat: Option<String>) -> Self {
        let insecure_tls = insecure_tls_enabled_from_env();
        Self::new_with_tls_policy(base_url, pat, insecure_tls)
    }

    pub fn new_with_tls_policy(base_url: &str, pat: Option<String>, insecure_tls: bool) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::builder()
                .danger_accept_invalid_certs(insecure_tls)
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build HTTP client"),
            pat,
        }
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/api/v4{}", self.base_url, path)
    }

    async fn get_paginated_json<T>(&self, path: &str) -> Result<Vec<T>>
    where
        T: DeserializeOwned,
    {
        let mut page = 1_u32;
        let per_page = 100_u32;
        let mut items = Vec::new();

        loop {
            let url = self.paginated_url(path, page, per_page);
            let resp = self
                .authed_request_url(Method::GET, url)?
                .send()
                .await?
                .error_for_status()?;
            let next_page = resp
                .headers()
                .get("x-next-page")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| {
                    let trimmed = value.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        trimmed.parse::<u32>().ok()
                    }
                });
            let batch: Vec<T> = resp.json().await?;
            let batch_len = batch.len();
            items.extend(batch);
            match next_page {
                Some(next_page) if next_page > page => {
                    page = next_page;
                }
                _ if batch_len == per_page as usize => {
                    page += 1;
                }
                _ => break,
            }
        }

        Ok(items)
    }

    fn paginated_url(&self, path: &str, page: u32, per_page: u32) -> String {
        let url = self.api_url(path);
        if url.contains('?') {
            format!("{url}&per_page={per_page}&page={page}")
        } else {
            format!("{url}?per_page={per_page}&page={page}")
        }
    }

    fn pat_value(&self) -> Result<String> {
        match self.pat.clone() {
            Some(value) => Ok(value),
            None => Err(anyhow::anyhow!(
                "no PAT configured — run `jeryu bootstrap` first"
            )),
        }
    }

    fn authed_request_url(&self, method: Method, url: String) -> Result<reqwest::RequestBuilder> {
        let pat = self.pat_value()?;
        Ok(self
            .client
            .request(method, url)
            .header("PRIVATE-TOKEN", pat))
    }

    pub fn pat_value_for_clone(&self) -> Option<String> {
        self.pat.clone()
    }

    // -- Private HTTP helpers -----------------------------------------------

    async fn api_post_json<Req, Resp>(&self, url: impl AsRef<str>, body: &Req) -> Result<Resp>
    where
        Req: serde::Serialize,
        Resp: serde::de::DeserializeOwned,
    {
        let resp: Resp = self
            .authed_request_url(Method::POST, url.as_ref().to_string())?
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }

    async fn api_get_json<Resp>(&self, url: impl AsRef<str>) -> Result<Resp>
    where
        Resp: serde::de::DeserializeOwned,
    {
        let resp: Resp = self
            .authed_request_url(Method::GET, url.as_ref().to_string())?
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }

    async fn api_post_void<Req>(&self, url: impl AsRef<str>, body: &Req) -> Result<()>
    where
        Req: serde::Serialize,
    {
        self.authed_request_url(Method::POST, url.as_ref().to_string())?
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn api_delete_void(&self, url: impl AsRef<str>) -> Result<()> {
        self.authed_request_url(Method::DELETE, url.as_ref().to_string())?
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// POST with no request body, parse JSON response.
    async fn api_post_nobody_json<Resp>(&self, url: impl AsRef<str>) -> Result<Resp>
    where
        Resp: serde::de::DeserializeOwned,
    {
        let resp: Resp = self
            .authed_request_url(Method::POST, url.as_ref().to_string())?
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp)
    }

    /// POST with no request body, discard response.
    async fn api_post_nobody_void(&self, url: impl AsRef<str>) -> Result<()> {
        self.authed_request_url(Method::POST, url.as_ref().to_string())?
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// PUT with JSON body, discard response.
    async fn api_put_void<Req>(&self, url: impl AsRef<str>, body: &Req) -> Result<()>
    where
        Req: serde::Serialize,
    {
        self.authed_request_url(Method::PUT, url.as_ref().to_string())?
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    // -- Health -------------------------------------------------------------

    pub async fn is_ready(&self) -> bool {
        for path in ["/help", "/users/sign_in"] {
            let url = format!("{}{}", self.base_url, path);
            if let Ok(resp) = self.client.get(&url).send().await {
                let status = resp.status();
                if status.is_success() || status.is_redirection() {
                    return true;
                }
            }
        }

        false
    }
}

fn insecure_tls_enabled_from_env() -> bool {
    gitlab_client_tls::insecure_tls_enabled_from_env()
}
