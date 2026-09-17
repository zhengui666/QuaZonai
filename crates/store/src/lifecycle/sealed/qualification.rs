//! Qualification consumes original sealed evidence, never a model's self-reported PASS.
use super::*;

const INSERT: &str = "INSERT INTO app.qualifications(alpha_version_id,policy_id,qualifying_evaluation_id,granted_at,valid_until) SELECT e.subject_alpha_version_id,e.policy_id,e.id,clock_timestamp(),least(e.valid_until,source.valid_until,(SELECT min(g.valid_until) FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id JOIN app.data_use_grants g ON g.id=d.data_use_grant_id WHERE i.input_set_id=ANY($2))) FROM app.evaluations e JOIN app.sealed_evaluation_tasks task ON task.run_id=e.run_id JOIN app.evaluations source ON source.id=task.validation_evaluation_id WHERE e.id=$1 AND least(e.valid_until,source.valid_until,(SELECT min(g.valid_until) FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id JOIN app.data_use_grants g ON g.id=d.data_use_grant_id WHERE i.input_set_id=ANY($2)))>clock_timestamp() RETURNING id";

pub(in crate::lifecycle) async fn grant(
    tx: &mut Tx<'_>,
    locked: &LockedRun,
) -> Result<(), StoreError> {
    if locked.run.kind != RunKind::AlphaEvaluate || locked.run.state != RunState::Succeeded {
        return Ok(());
    }
    let row = sqlx::query("SELECT e.id AS evaluation_id,e.subject_alpha_version_id AS alpha_version_id,e.policy_id,e.valid_until,v.alpha_id,source_cycle.brief_id,admission.runtime_id FROM app.evaluations e JOIN app.evaluation_publications published ON published.evaluation_id=e.id JOIN app.sealed_evaluation_tasks task ON task.run_id=e.run_id AND task.alpha_version_id=e.subject_alpha_version_id AND task.policy_id=e.policy_id JOIN app.mission_sealed_evaluations continuation ON continuation.run_id=task.run_id JOIN app.mission_reviews answer ON answer.reservation_id=continuation.review_reservation_id AND answer.decision='PASS' JOIN app.alpha_versions v ON v.id=task.alpha_version_id JOIN app.evaluations original ON original.id=task.validation_evaluation_id JOIN app.runs source_run ON source_run.id=original.run_id JOIN app.research_cycles source_cycle ON source_cycle.id=source_run.cycle_id JOIN app.run_admissions admission ON admission.run_id=e.run_id JOIN app.artifacts report ON report.id=e.report_artifact_id AND report.id=e.method_versions_artifact_id AND report.project_id=e.project_id AND report.producer_run_id=e.run_id AND report.producer_attempt_id=$2 AND report.origin='REAL' AND report.schema_name='qz.alpha_evaluation' AND report.schema_version='1' AND report.access_class='EVALUATOR_ONLY' JOIN app.sealed_opportunities opportunity ON opportunity.attempt_id=$2 JOIN app.run_native_attempts native ON native.attempt_id=opportunity.attempt_id AND native.run_id=e.run_id WHERE e.run_id=$1 AND e.project_id=$3 AND e.evaluation_kind='SEALED' AND e.execution_status='SUCCEEDED' AND e.evidence_status='VALID' AND e.decision='PASS' AND e.valid_until>clock_timestamp() AND NOT EXISTS(SELECT 1 FROM app.qualifications q WHERE q.alpha_version_id=e.subject_alpha_version_id AND q.policy_id=e.policy_id AND q.qualifying_evaluation_id=e.id)")
        .bind(locked.run.id.as_uuid()).bind(locked.run.active_attempt_id.map(Id::as_uuid))
        .bind(locked.run.project_id.as_uuid()).fetch_optional(&mut **tx).await?;
    let Some(row) = row else {
        return Ok(());
    };
    let context = crate::cycles::execution_context(tx, db::id(row.try_get("brief_id")?)?).await?;
    let sets = [
        context.discovery_input_set_id,
        context.validation_input_set_id,
        locked.run.input_set_id,
    ];
    let ids: Vec<_> = sets.iter().map(|id| id.as_uuid()).collect();
    let real: bool = sqlx::query_scalar("SELECT count(DISTINCT item.input_set_id)=3 AND bool_and(d.origin='REAL' AND d.pit_status='VERIFIED' AND d.revision_policy='AS_KNOWN_THEN') FROM app.input_set_items item JOIN app.dataset_revisions d ON d.id=item.dataset_revision_id WHERE item.input_set_id=ANY($1)")
        .bind(&ids).fetch_one(&mut **tx).await?;
    if !real {
        return Ok(());
    }
    for input in sets {
        match crate::research::revalidate_frozen_inputs(
            tx,
            input,
            locked.run.project_id,
            db::id(row.try_get("runtime_id")?)?,
        )
        .await
        {
            Ok(()) => {}
            Err(StoreError::Domain(DomainError::Fields(_))) => return Ok(()),
            Err(error) => return Err(error),
        }
    }
    match native::sealed::validate_binding(tx, locked.run.id).await {
        Ok(_) => {}
        Err(StoreError::Invalid("sealed_evaluation_binding")) => return Ok(()),
        Err(error) => return Err(error),
    }
    // Original license endpoints are immutable and their revocation locks are
    // held above. A late grant never extends the scientific/license window.
    let qualification: Option<uuid::Uuid> = sqlx::query_scalar(INSERT)
        .bind(row.try_get::<uuid::Uuid, _>("evaluation_id")?)
        .bind(&ids)
        .fetch_optional(&mut **tx)
        .await?;
    if qualification.is_some() {
        sqlx::query("UPDATE app.alphas SET lifecycle='QUALIFIED' WHERE id=$1 AND active_version_id=$2 AND lifecycle='RESEARCH'")
            .bind(row.try_get::<uuid::Uuid,_>("alpha_id")?).bind(row.try_get::<uuid::Uuid,_>("alpha_version_id")?)
            .execute(&mut **tx).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;

    #[sqlx::test(migrations = "../../migrations")]
    async fn native_postgres_checks_the_grant_statement_without_forging_real_evidence(
        pool: sqlx::PgPool,
    ) {
        pool.prepare(sqlx::SqlStr::from_static(INSERT))
            .await
            .unwrap();
    }
}
