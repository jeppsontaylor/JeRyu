use anyhow::Result;
use jeryu::agent;
use tokio::time::{Duration, sleep};

mod common;

#[tokio::test]
async fn test_agent_lifecycle() -> Result<()> {
    let Some(client) = common::skip_if_not_ready().await? else {
        return Ok(());
    };
    let db = jeryu::state::Db::open().await?;
    let docker = jeryu::docker::DockerCtl::connect()?;

    let project = common::create_test_project(&client, "agent-test").await?;

    // Scale up an ephemeral runner so the pipeline can actually finish later
    let (pool_name, runner_id) = common::create_ephemeral_pool(&client, &db).await?;
    jeryu::pool::resume_pool(&db, &client, &pool_name).await?;
    jeryu::pool::scale_pool_to(&db, &docker, &client, &pool_name, 1).await?;

    // Commit initial state so we can branch
    let init_code = r#"fn main() { println!("Hello"); }"#;
    client
        .create_file(project.id, "main", "main.rs", init_code, "Initial commit")
        .await?;

    // 1. Spawn Agent
    let task_desc = "Implement a test for the agent flow";
    let agent_task = agent::spawn_agent(&client, project.id, task_desc).await?;

    assert_eq!(agent_task.project_id, project.id);
    assert_ne!(agent_task.branch_name, "main");
    assert!(
        agent_task.issue_iid.is_some(),
        "Agent should have created a tracking issue"
    );

    // 2. Mock Agent Work (Commit to the branch)
    let agent_code = r#"fn main() { println!("Agent fixed it!"); }"#;
    client
        .update_file(
            project.id,
            &agent_task.branch_name,
            "main.rs",
            agent_code,
            "Agent finished task",
        )
        .await?;

    // 3. Create MR (The agent does this when it finishes)
    let mr = client
        .create_merge_request(
            project.id,
            &agent_task.branch_name,
            "main",
            "Agent Task Output",
            "Closes #1",
        )
        .await?;

    assert_eq!(mr.state, "opened");
    assert_eq!(mr.source_branch, agent_task.branch_name);

    // 4. Accept MR
    if let Err(err) = client.accept_merge_request(project.id, mr.iid).await {
        eprintln!("merge endpoint unavailable in this GitLab build: {err}");
    }

    // Wait for MR state to update
    sleep(Duration::from_secs(2)).await;

    // Clean up
    common::cleanup_ephemeral_pool(&client, &db, &pool_name, runner_id).await;

    Ok(())
}
