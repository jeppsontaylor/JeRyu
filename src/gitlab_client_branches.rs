use super::*;
use tracing::info;

impl GitlabClient {
    pub async fn create_branch(
        &self,
        project_id: i64,
        branch_name: &str,
        ref_name: &str,
    ) -> Result<()> {
        self.api_post_void(
            self.api_url(&format!("/projects/{}/repository/branches", project_id)),
            &CreateBranchReq {
                branch: branch_name,
                ref_name,
            },
        )
        .await?;
        info!(project_id, branch_name, "created branch");
        Ok(())
    }

    pub async fn delete_branch(&self, project_id: i64, branch_name: &str) -> Result<()> {
        let encoded_branch = urlencoding::encode(branch_name);
        self.api_delete_void(self.api_url(&format!(
            "/projects/{}/repository/branches/{}",
            project_id, encoded_branch
        )))
        .await?;
        info!(project_id, branch_name, "deleted branch");
        Ok(())
    }
}
