//! Trusted degradation consumption; no Operator impersonation or second queue.
use super::*;

async fn source(tx: &mut Tx<'_>, wake: Id) -> Result<PgRow, StoreError> {
    let row = sqlx::query("SELECT w.project_id,e.input_set_id,f.runtime_id,c.run_id AS candidate_run_id FROM app.wake_events w JOIN app.degradation_observations o ON o.id=w.observation_id AND o.project_id=w.project_id AND o.classification='DEGRADED' JOIN app.forward_observation_publications published ON published.observation_id=o.id JOIN app.evaluations e ON e.id=o.evaluation_id AND e.run_id=published.run_id AND e.evidence_status='VALID' AND e.execution_status='SUCCEEDED' AND e.valid_until>clock_timestamp() JOIN app.forward_evaluation_inputs f ON f.input_set_id=e.input_set_id AND f.project_id=w.project_id AND f.policy_id=o.policy_id JOIN app.handoff_offers h ON h.id=f.handoff_id AND h.release_id=o.release_id JOIN app.releases r ON r.id=h.release_id AND r.candidate_id=e.subject_candidate_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id JOIN app.forward_evidence_windows fw ON fw.evaluation_id=e.id AND fw.release_id=r.id AND fw.input_set_id=e.input_set_id AND fw.is_contiguous AND fw.complete_observations>0 AND fw.freshness_deadline>clock_timestamp() WHERE w.id=$1 AND w.trigger='DEGRADATION' AND w.state='PENDING'")
        .bind(wake.as_uuid()).fetch_optional(&mut **tx).await?
        .ok_or(StoreError::Invalid("wake_source_not_current"))?;
    crate::forward::revalidate(
        tx,
        db::id(row.try_get("input_set_id")?)?,
        db::id(row.try_get("project_id")?)?,
        db::id(row.try_get("runtime_id")?)?,
    )
    .await?;
    Ok(row)
}

async fn context(tx: &mut Tx<'_>, run: Id) -> Result<CycleStartIntent, StoreError> {
    // An automatic descendant retains the exact original human context. A later
    // unrelated human start cannot retroactively authorize a relational source.
    let row = sqlx::query("SELECT p.id AS project_id,p.revision,c.brief_id,s.researcher_profile_id,s.researcher_profile_revision,s.reviewer_profile_id,s.reviewer_profile_revision FROM app.runs run JOIN app.research_cycles c ON c.id=run.cycle_id AND c.project_id=run.project_id JOIN app.projects p ON p.id=c.project_id AND p.current_brief_id=c.brief_id JOIN app.cycle_startups s ON s.cycle_id=c.id WHERE run.id=$1 AND EXISTS(SELECT 1 FROM app.research_cycles human JOIN app.cycle_startups hs ON hs.cycle_id=human.id JOIN app.command_receipts receipt ON receipt.operation='CYCLE_START' AND receipt.resource_id=human.id WHERE human.project_id=c.project_id AND human.brief_id=c.brief_id AND human.trigger='OPERATOR' AND human.created_at<=c.created_at AND (hs.researcher_profile_id,hs.researcher_profile_revision,hs.reviewer_profile_id,hs.reviewer_profile_revision)=(s.researcher_profile_id,s.researcher_profile_revision,s.reviewer_profile_id,s.reviewer_profile_revision) AND receipt.response_nonsecret_body->'resource'->'cycle'->>'id'=human.id::text AND receipt.normalized_nonsecret_request->>'project_id'=p.id::text AND receipt.normalized_nonsecret_request->'request'->>'brief_id'=c.brief_id::text AND receipt.normalized_nonsecret_request->'request'->'researcher_profile'->>'profile_id'=s.researcher_profile_id::text AND receipt.normalized_nonsecret_request->'request'->'researcher_profile'->>'expected_revision'=s.researcher_profile_revision::text AND receipt.normalized_nonsecret_request->'request'->'reviewer_profile'->>'profile_id'=s.reviewer_profile_id::text AND receipt.normalized_nonsecret_request->'request'->'reviewer_profile'->>'expected_revision'=s.reviewer_profile_revision::text AND (c.id=human.id OR (c.trigger='DEGRADATION' AND EXISTS(SELECT 1 FROM app.wake_events ancestor WHERE ancestor.id=c.wake_id AND ancestor.state='CONSUMED' AND ancestor.consumed_cycle_id=c.id))))")
        .bind(run.as_uuid()).fetch_optional(&mut **tx).await?
        .ok_or(StoreError::Invalid("wake_human_context_required"))?;
    Ok(CycleStartIntent {
        schema_version: SchemaV1,
        project_id: db::id(row.try_get("project_id")?)?,
        request: CycleStartV1 {
            schema_version: SchemaV1,
            brief_id: db::id(row.try_get("brief_id")?)?,
            expected_revision: db::revision(row.try_get("revision")?)?,
            researcher_profile: CodexProfileChoiceV1 {
                profile_id: db::id(row.try_get("researcher_profile_id")?)?,
                expected_revision: db::revision(row.try_get("researcher_profile_revision")?)?,
            },
            reviewer_profile: CodexProfileChoiceV1 {
                profile_id: db::id(row.try_get("reviewer_profile_id")?)?,
                expected_revision: db::revision(row.try_get("reviewer_profile_revision")?)?,
            },
        },
    })
}

impl Store {
    /// Fair retry hint only. Consumption performs all authority checks again.
    pub async fn prepare_degradation_wake(&self, project: Id) -> Result<Option<Id>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let active: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM app.projects WHERE id=$1 AND state='ACTIVE' FOR UPDATE",
        )
        .bind(project.as_uuid())
        .fetch_optional(&mut *tx)
        .await?;
        if active.is_none() {
            return Ok(None);
        }
        let wake: Option<uuid::Uuid> = sqlx::query_scalar("UPDATE app.wake_events SET not_before=clock_timestamp()+interval '30 seconds' WHERE id=(SELECT id FROM app.wake_events WHERE project_id=$1 AND trigger='DEGRADATION' AND state='PENDING' AND not_before<=clock_timestamp() ORDER BY not_before,id LIMIT 1 FOR UPDATE) RETURNING id")
            .bind(project.as_uuid()).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        wake.map(db::id).transpose()
    }

    /// Native Worker only. The caller cannot select a Brief, profile or budget.
    pub async fn consume_degradation_wake<R, Read, P, Published>(
        &self,
        wake: Id,
        read: R,
        publish: P,
    ) -> Result<Option<CommandResult<Id>>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(crate::lifecycle::native::NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.wake_events WHERE id=$1")
                .bind(wake.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let state: String =
            sqlx::query_scalar("SELECT state FROM app.projects WHERE id=$1 FOR UPDATE")
                .bind(project)
                .fetch_one(&mut *tx)
                .await?;
        let row = sqlx::query(
            "SELECT state,consumed_cycle_id FROM app.wake_events WHERE id=$1 FOR UPDATE",
        )
        .bind(wake.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if row.try_get::<String, _>("state")? == "CONSUMED" {
            let cycle = db::id(row.try_get("consumed_cycle_id")?)?;
            tx.commit().await?;
            return Ok(Some(CommandResult {
                schema_version: SchemaV1,
                replayed: true,
                resource: cycle,
            }));
        }
        if state != "ACTIVE" || row.try_get::<String, _>("state")? != "PENDING" {
            return Ok(None);
        }
        let original = match source(&mut tx, wake).await {
            Ok(row) => row,
            Err(StoreError::Invalid(_) | StoreError::Domain(_) | StoreError::NotFound) => {
                sqlx::query("UPDATE app.wake_events SET state='CANCELLED',reason='ORIGINAL_FORWARD_AUTHORITY_NO_LONGER_CURRENT' WHERE id=$1")
                    .bind(wake.as_uuid()).execute(&mut *tx).await?;
                tx.commit().await?;
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let request = context(&mut tx, db::id(original.try_get("candidate_run_id")?)?).await?;
        let timing = sqlx::query("SELECT clock_timestamp() AS now,date_trunc('day',clock_timestamp(),'UTC')+interval '1 day' AS tomorrow,max(c.created_at)+make_interval(secs=>(b.budget->>'min_cycle_interval_seconds')::double precision) AS cooldown,count(c.id) FILTER(WHERE c.created_at>=date_trunc('day',clock_timestamp(),'UTC')) AS today,(b.budget->>'max_cycles_per_day')::bigint AS maximum FROM app.research_briefs b LEFT JOIN app.research_cycles c ON c.project_id=b.project_id WHERE b.id=$1 GROUP BY b.id")
            .bind(request.request.brief_id.as_uuid()).fetch_one(&mut *tx).await?;
        let now: DateTime<Utc> = timing.try_get("now")?;
        let cooldown: Option<DateTime<Utc>> = timing.try_get("cooldown")?;
        let quota = timing.try_get::<i64, _>("today")? >= timing.try_get::<i64, _>("maximum")?;
        if quota || cooldown.is_some_and(|until| until > now) {
            let until = if quota {
                timing.try_get::<DateTime<Utc>, _>("tomorrow")?
            } else {
                now
            };
            let until = cooldown.map_or(until, |cooldown| cooldown.max(until));
            sqlx::query("UPDATE app.wake_events SET not_before=$2,reason=$3 WHERE id=$1")
                .bind(wake.as_uuid())
                .bind(until)
                .bind(if quota {
                    "CYCLES_PER_DAY"
                } else {
                    "CYCLE_COOLDOWN"
                })
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(None);
        }
        let cycle = Id::new();
        let (mut tx, _) = Self::admit_cycle(
            tx,
            &format!("WAKE:{wake}"),
            &request,
            cycle,
            Some(wake),
            read,
            publish,
        )
        .await?;
        source(&mut tx, wake).await?;
        sqlx::query("UPDATE app.wake_events SET state='CONSUMED',consumed_cycle_id=$2,reason='NATIVE_CYCLE_STARTED' WHERE id=$1")
            .bind(wake.as_uuid()).bind(cycle.as_uuid()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(CommandResult {
            schema_version: SchemaV1,
            replayed: false,
            resource: cycle,
        }))
    }
}
