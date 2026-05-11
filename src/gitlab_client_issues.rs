use super::*;
use tracing::info;

impl GitlabClient {
    pub async fn create_issue(
        &self,
        project_id: i64,
        title: &str,
        description: &str,
        labels: &[&str],
        assignee_id: Option<i64>,
    ) -> Result<Issue> {
        let assignee_ids = assignee_id.map(|id| vec![id]);
        let issue: Issue = self
            .api_post_json(
                self.api_url(&format!("/projects/{}/issues", project_id)),
                &CreateIssueReq {
                    title,
                    description,
                    labels: labels.join(","),
                    assignee_ids,
                },
            )
            .await?;
        info!(project_id, issue_iid = issue.iid, "created issue");
        Ok(issue)
    }

    pub async fn list_issues_by_labels(
        &self,
        project_id: i64,
        labels: &[&str],
        state: Option<&str>,
    ) -> Result<Vec<Issue>> {
        let mut params = vec!["per_page=100".to_string()];
        if !labels.is_empty() {
            params.push(format!("labels={}", urlencoding::encode(&labels.join(","))));
        }
        if let Some(state) = state {
            params.push(format!("state={}", urlencoding::encode(state)));
        }
        let issues = self
            .api_get_json(self.api_url(&format!(
                "/projects/{}/issues?{}",
                project_id,
                params.join("&")
            )))
            .await?;
        Ok(issues)
    }

    pub async fn update_issue_labels(
        &self,
        project_id: i64,
        issue_iid: i64,
        labels: &[&str],
    ) -> Result<()> {
        self.api_put_void(
            self.api_url(&format!("/projects/{}/issues/{}", project_id, issue_iid)),
            &UpdateLabelsReq {
                labels: labels.join(","),
            },
        )
        .await
    }

    pub async fn comment_on_issue(
        &self,
        project_id: i64,
        issue_iid: i64,
        body: &str,
    ) -> Result<()> {
        self.api_post_void(
            self.api_url(&format!(
                "/projects/{}/issues/{}/notes",
                project_id, issue_iid
            )),
            &NoteReq { body },
        )
        .await
    }
}
