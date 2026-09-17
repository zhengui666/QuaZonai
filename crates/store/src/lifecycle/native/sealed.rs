//! Reserve one original Attempt's held-out access before any native capability leaves the transaction.
use super::*;

pub(super) async fn reserve(
    tx: &mut Tx<'_>,
    spec: &JobSpecV1,
    attempt: Id,
    new_spec: bool,
) -> Result<(), StoreError> {
    let held_out = spec.inputs.iter().any(|input| {
        matches!(
            input,
            RuntimeInputV1::Dataset {
                role: DataPartition::Sealed,
                ..
            }
        )
    }) || spec
        .requested_output_schemas
        .iter()
        .any(|schema| schema.name == "qz.alpha_sealed");
    if !held_out {
        return Ok(());
    }
    if !new_spec {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.sealed_opportunities o JOIN app.run_native_attempts a ON a.attempt_id=o.attempt_id WHERE a.attempt_id=$1 AND a.run_id=$2)")
            .bind(attempt.as_uuid()).bind(spec.run_id.as_uuid()).fetch_one(&mut **tx).await?;
        return if exists {
            Ok(())
        } else {
            Err(StoreError::Invalid("sealed_opportunity_missing"))
        };
    }
    // All preceding callers hold project -> cycle -> Run; the shared root is last.
    let root: uuid::Uuid = sqlx::query_scalar("SELECT lineage.id FROM app.research_lineages lineage JOIN app.projects project ON project.root_lineage_id=lineage.id JOIN app.runs run ON run.project_id=project.id WHERE run.id=$1 FOR UPDATE OF lineage")
        .bind(spec.run_id.as_uuid()).fetch_one(&mut **tx).await?;
    let binding = validate_binding(tx, spec.run_id).await?;
    if binding.try_get::<uuid::Uuid, _>("root_lineage_id")? != root {
        return Err(StoreError::Integrity);
    }
    let root = db::id(root)?;
    let dataset = db::id(binding.try_get("dataset_revision_id")?)?;
    check_opportunity(
        tx,
        root,
        dataset,
        binding.try_get("maximum_sealed_uses_per_lineage")?,
    )
    .await?;
    let exposure: uuid::Uuid = sqlx::query_scalar("INSERT INTO app.evidence_exposures(root_lineage_id,dataset_revision_id,actor_kind,actor_session_ref,exposure_kind,exposed_at,purpose) VALUES($1,$2,'EVALUATOR',$3,'RAW',clock_timestamp(),'NATIVE_SEALED_CAPABILITY_RESERVED') RETURNING id")
        .bind(root.as_uuid()).bind(dataset.as_uuid()).bind(attempt.to_string()).fetch_one(&mut **tx).await?;
    sqlx::query("INSERT INTO app.sealed_opportunities(attempt_id,exposure_id) VALUES($1,$2)")
        .bind(attempt.as_uuid())
        .bind(exposure)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn check_opportunity(
    tx: &mut Tx<'_>,
    root: Id,
    dataset: Id,
    maximum: i32,
) -> Result<(), StoreError> {
    let exposed: bool = sqlx::query_scalar(r#"
SELECT EXISTS(
 SELECT 1 FROM app.evidence_exposures e
 JOIN app.dataset_revisions previous ON previous.id=e.dataset_revision_id
 JOIN app.data_sources prior_source ON prior_source.id=previous.source_id
 JOIN app.dataset_revisions selected ON selected.id=$2
 JOIN app.data_sources current_source ON current_source.id=selected.source_id
 WHERE e.root_lineage_id=$1 AND
  (e.exposure_kind='LEGACY_UNKNOWN' OR
   (e.actor_kind IN ('OPERATOR','RESEARCH_AGENT','IMPORT')
    AND previous.partition_role='SEALED'
    AND (previous.id=selected.id OR
      (prior_source.runtime_id=current_source.runtime_id AND prior_source.native_catalog_ref=current_source.native_catalog_ref
       AND previous.native_storage_version=selected.native_storage_version AND previous.native_snapshot_ref=selected.native_snapshot_ref))))
)
"#).bind(root.as_uuid()).bind(dataset.as_uuid()).fetch_one(&mut **tx).await?;
    if exposed {
        return Err(StoreError::Invalid("sealed_independence_unavailable"));
    }
    let used: i64 = sqlx::query_scalar("SELECT count(*) FROM app.evidence_exposures e JOIN app.dataset_revisions d ON d.id=e.dataset_revision_id WHERE e.root_lineage_id=$1 AND e.actor_kind='EVALUATOR' AND e.exposure_kind='RAW' AND d.partition_role='SEALED'")
        .bind(root.as_uuid()).fetch_one(&mut **tx).await?;
    if used >= i64::from(maximum) {
        return Err(DomainError::BudgetExhausted("sealed_uses").into());
    }
    Ok(())
}

/// A persisted native capability retains its original opportunity. Only an
/// ungranted task can be rejected here, under the same root lock as reservation.
pub(super) async fn unavailable(tx: &mut Tx<'_>, run: Id, attempt: Id) -> Result<bool, StoreError> {
    let row = sqlx::query("SELECT lineage.id AS root_lineage_id,s.dataset_revision_id,p.maximum_sealed_uses_per_lineage FROM app.sealed_evaluation_tasks s JOIN app.alpha_versions v ON v.id=s.alpha_version_id JOIN app.research_lineages lineage ON lineage.id=v.root_lineage_id JOIN app.evaluation_policies p ON p.id=s.policy_id WHERE s.run_id=$1 AND NOT EXISTS(SELECT 1 FROM app.run_native_attempts WHERE attempt_id=$2) FOR UPDATE OF lineage")
        .bind(run.as_uuid()).bind(attempt.as_uuid()).fetch_optional(&mut **tx).await?;
    let Some(row) = row else {
        return Ok(false);
    };
    match check_opportunity(
        tx,
        db::id(row.try_get("root_lineage_id")?)?,
        db::id(row.try_get("dataset_revision_id")?)?,
        row.try_get("maximum_sealed_uses_per_lineage")?,
    )
    .await
    {
        Ok(()) => Ok(false),
        Err(
            StoreError::Domain(DomainError::BudgetExhausted("sealed_uses"))
            | StoreError::Invalid("sealed_independence_unavailable"),
        ) => Ok(true),
        Err(error) => Err(error),
    }
}

/// Shared by complete task admission and the later original capability reservation.
pub(in crate::lifecycle) async fn validate_binding(
    tx: &mut Tx<'_>,
    run: Id,
) -> Result<PgRow, StoreError> {
    let binding_sql = format!(
        r#"
SELECT v.root_lineage_id,s.dataset_revision_id,p.maximum_sealed_uses_per_lineage,
 r.kind='ALPHA_EVALUATE' AND r.project_id=v.project_id AND p.project_id=v.project_id
 AND pr.root_lineage_id=v.root_lineage_id AND source.root_lineage_id=v.root_lineage_id
 AND source.project_id=v.project_id AND source.alpha_id=v.alpha_id AND source.experiment_id=v.experiment_id
 AND source.model_artifact_id=v.model_artifact_id AND source.code_artifact_id=v.code_artifact_id
 AND alpha.lifecycle IN ('RESEARCH','QUALIFIED')
 AND ev.project_id=v.project_id AND ev.evaluation_kind='WALK_FORWARD'
 AND ev.execution_status='SUCCEEDED' AND ev.evidence_status='VALID' AND ev.decision='PASS'
 AND ev.valid_until>clock_timestamp()
 AND original.experiment_id=v.experiment_id AND original.policy_id=ev.policy_id
 AND t.access_class='EVALUATOR_ONLY' AND t.image_ref=v.runtime_image_ref
 AND t.origin=(CASE
     WHEN 'LEGACY_UNKNOWN' IN (d.origin,training.origin,discovery_task.origin) THEN 'LEGACY_UNKNOWN'
     WHEN 'FIXTURE' IN (d.origin,training.origin,discovery_task.origin) THEN 'FIXTURE'
     WHEN 'SYNTHETIC' IN (d.origin,training.origin,discovery_task.origin) THEN 'SYNTHETIC'
     ELSE 'REAL' END)
 AND t.output_schemas='[{{"name":"qz.alpha_sealed","version":"1"}}]'::jsonb
 AND p.sealed_metric_requirements IS NOT NULL
 AND p.split_policy->>'sealed_revision_id'=d.id::text AND d.partition_role='SEALED'
 AND EXISTS(SELECT 1 FROM app.input_sets i JOIN app.input_set_items item ON item.input_set_id=i.id
     WHERE i.id=r.input_set_id AND i.project_id=r.project_id AND i.purpose='SEALED' AND i.frozen_at IS NOT NULL
       AND item.dataset_revision_id=d.id AND item.role='SEALED')
 AND (SELECT count(*) FROM app.input_set_items WHERE input_set_id=r.input_set_id AND dataset_revision_id IS NOT NULL)=1
 AND t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','DATASET','revision_id',d.id,'role','SEALED'))
 AND t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',v.model_artifact_id,'role','MODEL'))
 AND t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',t.parameters_artifact_id,'role','PARAMETERS'))
 AND ((v.signal_kind='EXPECTED_RETURN' AND v.calibration_id IS NULL AND v.id=source.id AND jsonb_array_length(t.input_bindings)=3)
   OR (v.signal_kind='SCORE' AND calibration.validation_evaluation_id=ev.id
       AND calibration.train_input_set_id=ev.input_set_id AND calibration.horizon_kind=v.horizon_kind
       AND calibration.horizon_value=v.horizon_value AND calibration.output_unit='RETURN_PER_HORIZON'
       AND jsonb_array_length(t.input_bindings)=4
       AND t.input_bindings @> jsonb_build_array(jsonb_build_object('kind','ARTIFACT','artifact_id',calibration.model_artifact_id,'role','MODEL'))))
 AND (NOT p.require_real_data OR (d.origin='REAL' AND d.pit_status='VERIFIED'
     AND training.origin='REAL' AND training.pit_status='VERIFIED' AND discovery_task.origin='REAL'))
 AS valid
FROM app.sealed_evaluation_tasks s
JOIN app.runs r ON r.id=s.run_id
JOIN app.projects pr ON pr.id=r.project_id
JOIN app.run_native_tasks t ON t.run_id=r.id
JOIN app.alpha_versions v ON v.id=s.alpha_version_id
JOIN app.alphas alpha ON alpha.id=v.alpha_id
JOIN app.evaluation_policies p ON p.id=s.policy_id
JOIN ({}) ev ON ev.id=s.validation_evaluation_id
JOIN app.experiment_validations original ON original.run_id=ev.run_id AND original.alpha_version_id=ev.subject_alpha_version_id
JOIN app.alpha_versions source ON source.id=original.alpha_version_id
JOIN app.dataset_revisions training ON training.id=original.dataset_revision_id
JOIN app.experiment_forecasts discovery ON discovery.experiment_id=original.experiment_id
JOIN app.run_native_tasks discovery_task ON discovery_task.run_id=discovery.run_id
JOIN app.dataset_revisions d ON d.id=s.dataset_revision_id
LEFT JOIN app.calibrations calibration ON calibration.id=v.calibration_id
WHERE s.run_id=$1
"#,
        crate::evidence::EVALUATION
    );
    let binding = sqlx::query(sqlx::AssertSqlSafe(binding_sql))
        .bind(run.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::Invalid("sealed_evaluation_binding"))?;
    if binding.try_get::<Option<bool>, _>("valid")? != Some(true) {
        return Err(StoreError::Invalid("sealed_evaluation_binding"));
    }
    Ok(binding)
}
