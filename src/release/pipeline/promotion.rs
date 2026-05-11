use super::*;

pub async fn trigger_production_promotion(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    version: Option<String>,
) -> Result<i64> {
    let report = build_release_status_report(
        db,
        ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.to_string()),
            sha: None,
            limit: 20,
        },
    )
    .await?;
    let view = report
        .recent
        .iter()
        .find(|view| {
            version
                .as_deref()
                .map(|wanted| view.attempt.version == wanted)
                .unwrap_or(true)
        })
        .context("no release attempt found for production promotion")?;
    if view.canary_state != "e2e-passed" {
        return Err(ReleaseError::CanaryGateRejected {
            version: view.attempt.version.clone(),
            state: view.canary_state.clone(),
        }
        .into());
    }

    // Phase 4: Admission Control Enforcement - C Artifact Handoff validation.
    let release_root = release_dir(&view.attempt.version);
    let c_handoff_path = release_root.join("rendered/c-handoff.json");
    let c_validation_path = release_root.join("c-validation.json");

    if !c_handoff_path.exists() {
        return Err(ReleaseError::MissingHandoff {
            version: view.attempt.version.clone(),
            path: c_handoff_path,
        }
        .into());
    }
    if !c_validation_path.exists() {
        return Err(ReleaseError::MissingValidation {
            version: view.attempt.version.clone(),
            path: c_validation_path,
        }
        .into());
    }

    let sha = view.attempt.sha.clone();
    if let Some(existing_id) =
        production_promotion_pipeline_id(client, project_id, ref_name, &sha).await?
    {
        info!(
            project_id,
            pipeline_id = existing_id,
            ref_name = %ref_name,
            sha = %sha,
            version = %view.attempt.version,
            "production-promotion pipeline already exists"
        );
        return Ok(existing_id);
    }

    crate::cache::ensure_root_disk_headroom(
        crate::cache::ROOT_DISK_HEADROOM_MIN_FREE_BYTES,
        "production promotion",
    )
    .await?;

    let release_version = view.attempt.version.clone();
    let release_pipeline_id_str = match view.attempt.release_pipeline_id {
        Some(id) => id.to_string(),
        None => String::new(),
    };
    let mut trigger_vars = vec![
        ("CI_PIPELINE_PRODUCT", "production-promotion"),
        ("JERYU_PROD_APPROVED", "1"),
        ("JERYU_RELEASE_SHA", sha.as_str()),
        ("JERYU_RELEASE_VERSION", release_version.as_str()),
    ];
    if !release_pipeline_id_str.is_empty() {
        trigger_vars.push((
            "JERYU_RELEASE_PIPELINE_ID",
            release_pipeline_id_str.as_str(),
        ));
        trigger_vars.push((
            "JERYU_RELEASE_PIPELINE_ID",
            release_pipeline_id_str.as_str(),
        ));
    }
    let pipeline_id = client
        .trigger_pipeline(project_id, ref_name, trigger_vars)
        .await?;

    db.attach_production_pipeline(project_id, ref_name, &sha, pipeline_id, "created")
        .await?;

    let _ = db
        .upsert_tracked_pipeline(&crate::state::TrackedPipeline {
            pipeline_id,
            project_id,
            ref_name: ref_name.to_string(),
            sha: sha.clone(),
            status: "created".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        })
        .await;

    Ok(pipeline_id)
}

pub async fn maybe_trigger_production_promotion(
    db: &Db,
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    sha: Option<&str>,
    version: Option<&str>,
) -> Result<Option<i64>> {
    let report = build_release_status_report(
        db,
        ReleaseStatusQuery {
            project_id: Some(project_id),
            ref_name: Some(ref_name.to_string()),
            sha: sha.map(ToOwned::to_owned),
            limit: 20,
        },
    )
    .await?;

    let matches_requested = |view: &&ReleaseAttemptView| {
        version
            .map(|wanted| view.attempt.version == wanted)
            .unwrap_or(true)
            && sha.map(|wanted| view.attempt.sha == wanted).unwrap_or(true)
    };
    let selected = if version.is_some() || sha.is_some() {
        report.recent.iter().find(matches_requested)
    } else {
        report.latest.as_ref()
    };
    let Some(view) = selected else {
        return Ok(None);
    };

    // Sync CI artifacts to local disk if release pipeline succeeded and gate files are missing.
    let gate_files_before_sync = canary_gate_files(&view.attempt.version);
    if view.attempt.release_pipeline_status.as_deref() == Some("success")
        && let Some(release_pipeline_id) = view.attempt.release_pipeline_id
        && (!gate_files_before_sync.e2e
            || !gate_files_before_sync.handoff
            || !gate_files_before_sync.validation
            || !release_dir(&view.attempt.version)
                .join("release.json")
                .is_file()
            || !release_dir(&view.attempt.version)
                .join("release-contract.json")
                .is_file())
        && let Err(err) = sync_canary_artifacts(
            client,
            project_id,
            release_pipeline_id,
            &view.attempt.version,
        )
        .await
    {
        warn!(
            project_id,
            version = %view.attempt.version,
            error = %err,
            "artifact sync failed; production promotion may be delayed"
        );
    }

    // Re-evaluate gate file presence after potential artifact sync.
    let gate_files = canary_gate_files(&view.attempt.version);
    let gate_files_ok = gate_files.promotion_ready();
    let identity_ok = release_identity_ok(&view.attempt.version, &view.attempt.sha);

    if !gate_files_ok
        || !identity_ok
        || view.attempt.release_pipeline_status.as_deref() != Some("success")
        || gate_prod_promotion_path(&view.attempt.version).is_file()
    {
        return Ok(None);
    }

    if view.attempt.canary_status != "passed" {
        db.finish_release_canary(
            project_id,
            ref_name,
            &view.attempt.sha,
            "passed",
            Some("required canary gate evidence synced from release-execution pipeline"),
        )
        .await?;
    }

    if let Some(existing_id) =
        production_promotion_pipeline_id(client, project_id, ref_name, &view.attempt.sha).await?
    {
        db.attach_production_pipeline(
            project_id,
            ref_name,
            &view.attempt.sha,
            existing_id,
            "running",
        )
        .await?;
        return Ok(Some(existing_id));
    }

    let pipeline_id = trigger_production_promotion(
        db,
        client,
        project_id,
        ref_name,
        Some(view.attempt.version.clone()),
    )
    .await?;
    Ok(Some(pipeline_id))
}

pub(crate) async fn production_promotion_pipeline_id(
    client: &GitlabClient,
    project_id: i64,
    ref_name: &str,
    sha: &str,
) -> Result<Option<i64>> {
    for pipeline in client
        .list_pipelines(project_id, Some(ref_name))
        .await?
        .into_iter()
    {
        if !pipeline_matches_release_sha(client, project_id, pipeline.id, &pipeline.sha, sha)
            .await?
        {
            continue;
        }
        let jobs = aggregate_pipeline_jobs(
            client
                .list_pipeline_jobs_with_downstream(project_id, pipeline.id)
                .await?,
        );
        let Some(job) = jobs.get("promote-production-final") else {
            continue;
        };
        if matches!(
            job.status.as_str(),
            "created" | "pending" | "running" | "success"
        ) {
            return Ok(Some(pipeline.id));
        }
    }
    Ok(None)
}

pub(crate) async fn pipeline_matches_release_sha(
    client: &GitlabClient,
    project_id: i64,
    pipeline_id: i64,
    pipeline_sha: &str,
    release_sha: &str,
) -> Result<bool> {
    if pipeline_sha == release_sha {
        return Ok(true);
    }
    match client
        .list_pipeline_variables(project_id, pipeline_id)
        .await
    {
        Ok(variables) => Ok(variables.iter().any(|variable| {
            matches!(variable.key.as_str(), "JERYU_RELEASE_SHA") && variable.value == release_sha
        })),
        Err(err) => {
            warn!(
                project_id,
                pipeline_id,
                error = %err,
                "could not inspect pipeline variables while checking production promotion"
            );
            Ok(false)
        }
    }
}
