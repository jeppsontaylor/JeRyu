use super::*;
use tracing::info;

impl GitlabClient {
    pub async fn create_group_webhook(
        &self,
        group_id: i64,
        url: &str,
        secret_token: &str,
    ) -> Result<i64> {
        let resp: WebhookResp = self
            .api_post_json(
                self.api_url(&format!("/groups/{}/hooks", group_id)),
                &CreateWebhookReq {
                    url,
                    token: secret_token,
                    job_events: true,
                    pipeline_events: true,
                    push_events: true,
                    merge_requests_events: true,
                },
            )
            .await?;
        info!(webhook_id = resp.id, "created group webhook");
        Ok(resp.id)
    }
}
