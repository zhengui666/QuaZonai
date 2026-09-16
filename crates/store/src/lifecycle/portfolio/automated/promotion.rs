//! Freeze every current Paper stream; never sum streams or substitute evidence at Claim.
use super::*;

pub(super) async fn evidence(
    tx: &mut Tx<'_>,
    policy: &AutomationPolicyViewV1,
    candidate: Id,
) -> Result<(Vec<Id>, DateTime<Utc>), StoreError> {
    if policy.content.mode != AutomationModeV1::AutoHandoff {
        return Err(StoreError::Invalid("automation_live_mode"));
    }
    let (_, mut until) = crate::automation::active_policy(
        tx,
        policy.id,
        policy.project_id,
        policy.content.mandate_id,
        policy.content.downstream_id,
    )
    .await?;
    // No policy field chooses a preferred stream. Every reported original Paper
    // stream is required, including one whose latest source is not evaluated yet.
    let rows = sqlx::query("SELECT streams.handoff_id,streams.stream_id,observed.* FROM (SELECT DISTINCT h.id AS handoff_id,m.stream_id FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id JOIN app.handoff_transfers transfer ON transfer.handoff_id=h.id AND transfer.external_claim_id=h.external_claim_id AND transfer.claimed_at=h.claimed_at AND transfer.provenance='RECORDED_TRANSITION' JOIN app.forward_messages m ON m.handoff_id=h.id WHERE c.id=$1 AND c.project_id=$2 AND c.mandate_id=$3 AND h.downstream_id=$4 AND h.environment='PAPER' AND h.state IN ('CLAIMED','ACKNOWLEDGED')) streams LEFT JOIN LATERAL (SELECT o.id AS observation_id,o.classification,e.id AS evaluation_id,e.input_set_id,e.execution_status,e.evidence_status,e.valid_until,w.window_start,w.window_end,w.complete_observations,w.is_contiguous,w.freshness_deadline,f.runtime_id FROM app.forward_evaluation_inputs f JOIN app.runs run ON run.input_set_id=f.input_set_id AND run.project_id=f.project_id AND run.kind='FORWARD_EVALUATE' JOIN app.forward_observation_publications published ON published.run_id=run.id JOIN app.degradation_observations o ON o.id=published.observation_id AND o.policy_id=f.policy_id JOIN app.evaluations e ON e.id=o.evaluation_id AND e.run_id=run.id AND e.input_set_id=f.input_set_id AND e.subject_candidate_id=$1 AND e.evaluation_kind='FORWARD' JOIN app.forward_evidence_windows w ON w.evaluation_id=e.id AND w.release_id=o.release_id AND w.input_set_id=e.input_set_id WHERE f.handoff_id=streams.handoff_id AND f.request->'request'->'window'->>'stream_id'=streams.stream_id AND f.policy_id=$5 ORDER BY o.observed_at DESC,o.id DESC LIMIT 1) observed ON true ORDER BY streams.handoff_id,streams.stream_id LIMIT 256")
        .bind(candidate.as_uuid()).bind(policy.project_id.as_uuid()).bind(policy.content.mandate_id.as_uuid()).bind(policy.content.downstream_id.as_uuid()).bind(policy.id.as_uuid()).fetch_all(&mut **tx).await?;
    if rows.is_empty() || rows.len() > 255 {
        return Err(StoreError::Invalid(
            "automation_live_complete_paper_required",
        ));
    }
    let current = now(tx).await?;
    let mut observations = Vec::with_capacity(rows.len());
    for row in rows {
        let observation = db::optional_id(&row, "observation_id")?
            .ok_or(StoreError::Invalid("automation_live_unevaluated_stream"))?;
        let valid_until: Option<DateTime<Utc>> = row.try_get("valid_until")?;
        let deadline: DateTime<Utc> = row.try_get("freshness_deadline")?;
        let start: DateTime<Utc> = row.try_get("window_start")?;
        let end: DateTime<Utc> = row.try_get("window_end")?;
        if row.try_get::<String, _>("classification")? != "HEALTHY"
            || row.try_get::<String, _>("execution_status")? != "SUCCEEDED"
            || row.try_get::<String, _>("evidence_status")? != "VALID"
            || valid_until.is_none_or(|v| v <= current)
            || deadline <= current
            || !row.try_get::<bool, _>("is_contiguous")?
            || row.try_get::<i64, _>("complete_observations")?
                < i64::from(policy.content.required_paper_observations)
            || u64::try_from((end - start).num_seconds())
                .ok()
                .is_none_or(|seconds| seconds < policy.content.minimum_paper_elapsed_seconds.get())
        {
            return Err(StoreError::Invalid("automation_live_paper_not_qualified"));
        }
        crate::forward::revalidate(
            tx,
            db::id(row.try_get("input_set_id")?)?,
            policy.project_id,
            db::id(row.try_get("runtime_id")?)?,
        )
        .await?;
        let evaluation = db::id(row.try_get("evaluation_id")?)?;
        let metrics = sqlx::query(
            "SELECT * FROM app.metric_values WHERE evaluation_id=$1 ORDER BY metric_code,scope",
        )
        .bind(evaluation.as_uuid())
        .fetch_all(&mut **tx)
        .await?
        .iter()
        .map(crate::evidence::metric)
        .collect::<Result<Vec<_>, _>>()?;
        let classified = domain::forward::observation::classify(
            evaluation,
            true,
            &policy.content.degradation_metric_requirements,
            &policy.content.promotion_metric_requirements,
            &metrics,
        )?;
        if classified.classification != domain::forward::observation::Classification::Healthy {
            return Err(StoreError::Invalid("automation_live_metrics_not_qualified"));
        }
        until = until
            .min(deadline)
            .min(valid_until.ok_or(StoreError::Integrity)?);
        observations.push(observation);
    }
    observations.sort_by_key(|id| id.as_uuid());
    if now(tx).await? >= until {
        return Err(StoreError::Invalid("automation_live_expiry"));
    }
    Ok((observations, until))
}
