//! Trusted bounded continuation of the latest original Release's fixed cohort.
use super::*;

impl Store {
    /// Worker-only entry: no Actor, grant minting, or external command route.
    pub async fn automate_rebalance_build<R, Read, P, Published>(
        &self,
        project: Id,
        mut read: R,
        publish: P,
    ) -> Result<Option<RunSnapshotV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnMut(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT current_automation_policy_id FROM app.projects WHERE id=$1 AND state='ACTIVE' FOR UPDATE SKIP LOCKED")
            .bind(project.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else { return Ok(None) };
        let Some(policy_id) = db::optional_id(&row, "current_automation_policy_id")? else {
            return Ok(None);
        };
        let row =
            sqlx::query("SELECT * FROM app.automation_policies WHERE id=$1 AND project_id=$2")
                .bind(policy_id.as_uuid())
                .bind(project.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let policy = crate::automation::view(&row)?;
        if policy.content.mode == contracts::delivery::AutomationModeV1::Manual
            || !policy.content.enabled_for_new_rebalances
        {
            return Ok(None);
        }
        let mandate_id = policy.content.mandate_id;
        let downstream = policy.content.downstream_id;
        crate::automation::active_policy(&mut tx, policy_id, project, mandate_id, downstream)
            .await?;
        let row = sqlx::query("SELECT c.id,c.decision_asof,t.request FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id JOIN app.portfolio_build_tasks t ON t.run_id=c.run_id WHERE c.project_id=$1 AND c.mandate_id=$2 ORDER BY r.id DESC LIMIT 1")
            .bind(project.as_uuid()).bind(mandate_id.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else { return Ok(None) };
        let source = db::id(row.try_get("id")?)?;
        let previous: DateTime<Utc> = row.try_get("decision_asof")?;
        let mut request: PortfolioBuildRequestV1 =
            serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?;
        if request.mandate_id != mandate_id {
            return Err(StoreError::Integrity);
        }
        let row = sqlx::query("SELECT * FROM app.portfolio_mandates WHERE id=$1 AND project_id=$2")
            .bind(mandate_id.as_uuid())
            .bind(project.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let mandate = crate::portfolio::view(&row)?;
        if mandate.content.rebalance_schedule.kind == RebalanceKind::Manual {
            return Ok(None);
        }
        // A successful Build awaits its independent Study/Release. Do not flood
        // the queue while that successor is being evaluated or is inconclusive.
        let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.portfolio_rebalances b JOIN app.runs r ON r.id=b.run_id WHERE b.project_id=$1 AND b.mandate_id=$2 AND b.downstream_id=$3 AND b.source_candidate_id=$4 AND r.state NOT IN ('FAILED','CANCELLED'))")
            .bind(project.as_uuid()).bind(mandate_id.as_uuid()).bind(downstream.as_uuid()).bind(source.as_uuid()).fetch_one(&mut *tx).await?;
        if pending {
            return Ok(None);
        }
        let row = sqlx::query("SELECT i.id,i.decision_cutoff FROM app.input_sets i WHERE i.project_id=$1 AND i.purpose='FORWARD' AND i.frozen_at IS NOT NULL AND i.decision_cutoff>$2 AND i.decision_cutoff<=clock_timestamp() AND EXISTS(SELECT 1 FROM app.input_set_items item JOIN app.dataset_revisions d ON d.id=item.dataset_revision_id WHERE item.input_set_id=i.id AND d.universe_version_id=$3) ORDER BY i.decision_cutoff DESC,i.id DESC LIMIT 1")
            .bind(project.as_uuid()).bind(previous).bind(mandate.content.universe_version_id.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else { return Ok(None) };
        request.input_set_id = db::id(row.try_get("id")?)?;
        // InputSet cutoff is only an upper bound; an older native catalog must
        // not acquire a new decision time from a fresh InputSet UUID.
        let bindings = crate::data_validation::dataset_bindings(
            &mut tx,
            request.input_set_id,
            project,
            request.runtime_id,
            &[contracts::research::InputPurpose::Forward],
            &mut read,
        )
        .await?;
        if bindings.len() != 1 {
            return Err(StoreError::Invalid("portfolio_forward_dataset"));
        }
        let cutoff = publication::target_window(
            bindings[0].selection.selection.decision_cutoff_ns.get(),
            mandate.content.rebalance_schedule.target_ttl_seconds,
        )?
        .0;
        if cutoff <= previous {
            return Ok(None);
        }
        if !due(&mut tx, &mandate, previous, cutoff, &mut read).await? {
            return Ok(None);
        }
        let seen: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.portfolio_rebalances WHERE project_id=$1 AND mandate_id=$2 AND downstream_id=$3 AND decision_cutoff=$4)")
            .bind(project.as_uuid()).bind(mandate_id.as_uuid()).bind(downstream.as_uuid()).bind(cutoff).fetch_one(&mut *tx).await?;
        if seen {
            return Ok(None);
        }
        let today: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.portfolio_rebalances WHERE project_id=$1 AND downstream_id=$2 AND created_at>=(date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC')), (SELECT count(DISTINCT r.candidate_id) FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE c.project_id=$1 AND h.downstream_id=$2 AND h.offered_at>=(date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'))")
            .bind(project.as_uuid()).bind(downstream.as_uuid()).fetch_one(&mut *tx).await?;
        if today.0 >= i64::from(policy.content.max_rebalances_per_day)
            || today.1 >= i64::from(policy.content.max_rebalances_per_day)
        {
            return Err(StoreError::Invalid("automation_daily_quota"));
        }
        let cutoff_ns = cutoff.timestamp_nanos_opt().ok_or(StoreError::Integrity)?;
        let weights: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.forward_weight_snapshots WHERE project_id=$1 AND downstream_id=$2 AND environment=$3 AND (content->>'asof_ns')::bigint<=$4 AND (content->>'available_ns')::bigint<=$4 ORDER BY id DESC LIMIT 1")
            .bind(project.as_uuid()).bind(downstream.as_uuid()).bind(db::code(&request.environment)?).bind(cutoff_ns).fetch_optional(&mut *tx).await?;
        let Some(weights) = weights else {
            return Ok(None);
        };
        request.current_weights_source = PortfolioBuildWeightsV1::ForwardSnapshot {
            snapshot_id: db::id(weights)?,
        };
        domain::portfolio::build_selection(&request)?;
        let (mut tx, run, window) = admit_build(tx, &request, read, publish, "RUNTIME").await?;
        crate::automation::active_policy(&mut tx, policy_id, project, mandate_id, downstream)
            .await?;
        if window.decision_asof != cutoff {
            return Err(StoreError::Integrity);
        }
        window.recheck(&mut tx, &request).await?;
        sqlx::query("INSERT INTO app.portfolio_rebalances(run_id,project_id,mandate_id,downstream_id,policy_id,source_candidate_id,input_set_id,decision_cutoff) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(run.id.as_uuid()).bind(project.as_uuid()).bind(mandate_id.as_uuid()).bind(downstream.as_uuid()).bind(policy_id.as_uuid()).bind(source.as_uuid()).bind(request.input_set_id.as_uuid()).bind(cutoff).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(run))
    }
}

async fn due<R, Read>(
    tx: &mut Tx<'_>,
    mandate: &MandateViewV1,
    previous: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    read: &mut R,
) -> Result<bool, StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let schedule = &mandate.content.rebalance_schedule;
    domain::portfolio::rebalance_schedule(schedule)?;
    match schedule.kind {
        RebalanceKind::Manual => Ok(false),
        RebalanceKind::FixedInterval => Ok(cutoff.signed_duration_since(previous)
            >= Duration::seconds(i64::from(
                schedule.interval_seconds.ok_or(StoreError::Integrity)?,
            ))),
        RebalanceKind::CalendarSession => {
            let id: Option<uuid::Uuid> = sqlx::query_scalar(
                "SELECT calendar_artifact_id FROM app.universe_versions WHERE id=$1",
            )
            .bind(mandate.content.universe_version_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
            let id = db::id(id.ok_or(StoreError::Invalid("rebalance_calendar_missing"))?)?;
            let bytes = crate::execution_assumptions::liquidity::document(
                tx,
                mandate.project_id,
                id,
                "qz.calendar_sessions",
                1024 * 1024,
                read,
            )
            .await?;
            let calendar: NativeCalendarSessionsV1 =
                serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
            domain::catalogs::calendar_sessions(&calendar)?;
            let previous = u64::try_from(
                previous
                    .timestamp_nanos_opt()
                    .ok_or(StoreError::Integrity)?,
            )
            .map_err(|_| StoreError::Integrity)?;
            let cutoff = u64::try_from(cutoff.timestamp_nanos_opt().ok_or(StoreError::Integrity)?)
                .map_err(|_| StoreError::Integrity)?;
            let offset = i64::from(
                schedule
                    .session_offset_seconds
                    .ok_or(StoreError::Integrity)?,
            ) * 1_000_000_000;
            let start = previous
                .checked_add_signed(-offset)
                .ok_or(StoreError::Integrity)?;
            let end = cutoff
                .checked_add_signed(-offset)
                .ok_or(StoreError::Integrity)?;
            if schedule.calendar_ref.as_ref() != Some(&calendar.calendar_ref)
                || schedule.timezone != calendar.timezone
                || calendar.available_at_ns.get() > cutoff
                || calendar.coverage_start_ns.get() > start
                || calendar.coverage_end_ns.get() <= end
            {
                return Err(StoreError::Invalid("rebalance_calendar_binding"));
            }
            Ok(calendar
                .sessions
                .iter()
                .any(|session| session.close_ns.get() > start && session.close_ns.get() <= end))
        }
    }
}
