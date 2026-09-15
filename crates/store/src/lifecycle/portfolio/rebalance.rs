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
        if !due(&mut tx, &mandate, previous, cutoff, &bindings[0], &mut read).await? {
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
        let weights: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM app.forward_weight_snapshots WHERE project_id=$1 AND downstream_id=$2 AND environment=$3 AND (content->>'asof_ns')::bigint<=$4 AND (content->>'available_ns')::bigint<=$4 AND (content->>'valid_until_ns')::bigint>$4 ORDER BY (content->>'asof_ns')::bigint DESC,(content->>'available_ns')::bigint DESC,id DESC LIMIT 1")
            .bind(project.as_uuid()).bind(downstream.as_uuid()).bind(db::code(&request.environment)?).bind(cutoff_ns).fetch_optional(&mut *tx).await?;
        let Some(weights) = weights else {
            return Ok(None);
        };
        request.current_weights_source = PortfolioBuildWeightsV1::ForwardSnapshot {
            snapshot_id: db::id(weights)?,
        };
        domain::portfolio::build_selection(&request)?;
        let (mut tx, run, window) =
            admit_build(tx, &request, &mut read, publish, "RUNTIME").await?;
        crate::automation::active_policy(&mut tx, policy_id, project, mandate_id, downstream)
            .await?;
        if window.decision_asof != cutoff {
            return Err(StoreError::Integrity);
        }
        window.recheck(&mut tx, &request).await?;
        if !due(&mut tx, &mandate, previous, cutoff, &bindings[0], &mut read).await? {
            return Err(StoreError::Conflict);
        }
        sqlx::query("INSERT INTO app.portfolio_rebalances(run_id,project_id,mandate_id,downstream_id,policy_id,source_candidate_id,input_set_id,decision_cutoff) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(run.id.as_uuid()).bind(project.as_uuid()).bind(mandate_id.as_uuid()).bind(downstream.as_uuid()).bind(policy_id.as_uuid()).bind(source.as_uuid()).bind(request.input_set_id.as_uuid()).bind(cutoff).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(run))
    }
}

impl Store {
    /// Advance only the successful original automatic Build, once per Build.
    pub async fn automate_rebalance_study<R, Read, P, Published>(
        &self,
        project: Id,
        read: R,
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
        let row = sqlx::query("SELECT b.run_id,b.mandate_id,b.downstream_id,c.id AS candidate_id,t.request FROM app.portfolio_rebalances b JOIN app.portfolio_build_tasks t ON t.run_id=b.run_id JOIN app.runs r ON r.id=b.run_id AND r.state='SUCCEEDED' JOIN app.portfolio_candidates c ON c.run_id=r.id AND c.project_id=b.project_id AND c.mandate_id=b.mandate_id AND c.evidence_status='VALID' JOIN app.candidate_publications published ON published.candidate_id=c.id WHERE b.project_id=$1 AND b.policy_id=$2 AND NOT EXISTS(SELECT 1 FROM app.portfolio_rebalance_studies s WHERE s.build_run_id=b.run_id) AND b.source_candidate_id=(SELECT seed.candidate_id FROM app.releases seed JOIN app.portfolio_candidates origin ON origin.id=seed.candidate_id WHERE origin.project_id=b.project_id AND origin.mandate_id=b.mandate_id ORDER BY seed.id DESC LIMIT 1) ORDER BY b.decision_cutoff DESC,b.run_id DESC LIMIT 1")
            .bind(project.as_uuid()).bind(policy_id.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else { return Ok(None) };
        let build_id = db::id(row.try_get("run_id")?)?;
        let mandate = db::id(row.try_get("mandate_id")?)?;
        let downstream = db::id(row.try_get("downstream_id")?)?;
        crate::automation::active_policy(&mut tx, policy_id, project, mandate, downstream).await?;
        let build: PortfolioBuildRequestV1 =
            serde_json::from_value(row.try_get("request")?).map_err(|_| StoreError::Integrity)?;
        if build.mandate_id != mandate {
            return Err(StoreError::Integrity);
        }
        let request = PortfolioStudyRequestV1 {
            schema_version: SchemaV1,
            candidate_id: db::id(row.try_get("candidate_id")?)?,
            cycle_id: build.cycle_id,
            runtime_id: build.runtime_id,
            expected_runtime_revision: build.expected_runtime_revision,
            limits: build.limits,
        };
        let (mut tx, run, window) =
            Box::pin(study::admit_study(tx, &request, read, publish, "RUNTIME")).await?;
        crate::automation::active_policy(&mut tx, policy_id, project, mandate, downstream).await?;
        window.recheck(&mut tx).await?;
        sqlx::query(
            "INSERT INTO app.portfolio_rebalance_studies(build_run_id,study_run_id) VALUES($1,$2)",
        )
        .bind(build_id.as_uuid())
        .bind(run.id.as_uuid())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(run))
    }
}

impl Store {
    /// Freeze the original automatic Study's formal PASS, without approving delivery.
    pub async fn automate_rebalance_release<R, Read, P, Published>(
        &self,
        project: Id,
        read: R,
        publish: P,
    ) -> Result<Option<contracts::delivery::ReleaseViewV1>, StoreError>
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
        let row = sqlx::query("SELECT b.run_id,b.mandate_id,b.downstream_id,s.candidate_id,e.id AS evaluation_id FROM app.portfolio_rebalances b JOIN app.portfolio_rebalance_studies child ON child.build_run_id=b.run_id JOIN app.portfolio_study_tasks s ON s.run_id=child.study_run_id JOIN app.evaluations e ON e.run_id=s.run_id AND e.subject_candidate_id=s.candidate_id AND e.project_id=b.project_id AND e.evaluation_kind='PORTFOLIO' AND e.execution_status='SUCCEEDED' AND e.evidence_status='VALID' AND e.decision='PASS' AND e.valid_until>clock_timestamp() JOIN app.evaluation_publications p ON p.evaluation_id=e.id WHERE b.project_id=$1 AND b.policy_id=$2 AND NOT EXISTS(SELECT 1 FROM app.portfolio_rebalance_releases done WHERE done.build_run_id=b.run_id) AND b.source_candidate_id=(SELECT seed.candidate_id FROM app.releases seed JOIN app.portfolio_candidates origin ON origin.id=seed.candidate_id WHERE origin.project_id=b.project_id AND origin.mandate_id=b.mandate_id ORDER BY seed.id DESC LIMIT 1) ORDER BY b.decision_cutoff DESC,b.run_id DESC LIMIT 1")
            .bind(project.as_uuid()).bind(policy_id.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(row) = row else { return Ok(None) };
        let build_id = db::id(row.try_get("run_id")?)?;
        let mandate = db::id(row.try_get("mandate_id")?)?;
        let downstream = db::id(row.try_get("downstream_id")?)?;
        crate::automation::active_policy(&mut tx, policy_id, project, mandate, downstream).await?;
        let request = contracts::delivery::ReleaseCreateV1 {
            schema_version: SchemaV1,
            candidate_id: db::id(row.try_get("candidate_id")?)?,
            evaluation_id: db::id(row.try_get("evaluation_id")?)?,
        };
        let (mut tx, released) = Box::pin(release::freeze_release(
            tx, &request, read, publish, "RUNTIME",
        ))
        .await?;
        crate::automation::active_policy(&mut tx, policy_id, project, mandate, downstream).await?;
        if released.valid_until <= now(&mut tx).await? {
            return Err(StoreError::Conflict);
        }
        sqlx::query(
            "INSERT INTO app.portfolio_rebalance_releases(build_run_id,release_id) VALUES($1,$2)",
        )
        .bind(build_id.as_uuid())
        .bind(released.id.as_uuid())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(released))
    }
}

async fn due<R, Read>(
    tx: &mut Tx<'_>,
    mandate: &MandateViewV1,
    previous: DateTime<Utc>,
    cutoff: DateTime<Utc>,
    dataset: &crate::data_validation::DatasetBinding,
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
            let (calendar, _) = crate::data_registration::registered_calendar(
                tx,
                mandate.content.universe_version_id,
                &dataset.metadata.universe,
                dataset.origin,
                read,
            )
            .await?;
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
