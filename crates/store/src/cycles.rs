//! Formal Brief freeze and Cycle start. Existing authorization, data grant,
//! budget, Run and PGMQ transactions remain the only publication authorities.
use crate::{
    authority::{self, Actor},
    commands,
    control::page,
    db,
    lifecycle::RunSubmission,
    Store, StoreError,
};
use chrono::{DateTime, Utc};
use contracts::{
    brief::{BriefState, BriefView, HorizonKind},
    control::{CommandResult, ListQuery, MachineScope, OperatorOperation, Page},
    cycles::*,
    lifecycle::JobLimitsV1,
    research::{
        DataPartition, InputItemV1, InputPurpose, InputSetCreate, SelectionRuleV1, SplitPolicyV1,
    },
    runs::RunKind,
    runtime::RuntimeCapabilitiesV1,
    DbCounter, Id, SchemaV1,
};
use domain::{research::invalid, DomainError};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use std::collections::BTreeSet;

type Tx<'a> = Transaction<'a, Postgres>;

pub(crate) async fn execution_context(
    tx: &mut Tx<'_>,
    brief: Id,
) -> Result<BriefExecutionContextV1, StoreError> {
    let r = sqlx::query("SELECT * FROM app.brief_execution_contexts WHERE brief_id=$1")
        .bind(brief.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| invalid("brief_id", "BRIEF_EXECUTION_CONTEXT_MISSING"))?;
    Ok(BriefExecutionContextV1 {
        schema_version: SchemaV1,
        runtime_id: db::id(r.try_get("runtime_id")?)?,
        runtime_revision: db::revision(r.try_get("runtime_revision")?)?,
        discovery_input_set_id: db::id(r.try_get("discovery_input_set_id")?)?,
        validation_input_set_id: db::id(r.try_get("validation_input_set_id")?)?,
        sealed_input_set_id: db::id(r.try_get("sealed_input_set_id")?)?,
    })
}

/// Called with the project and exact Brief locked. No native network call, raw
/// sealed-data access, or immutable data-origin mutation occurs here.
pub(crate) async fn validate_execution_context(
    tx: &mut Tx<'_>,
    brief: &BriefView,
    context: &BriefExecutionContextV1,
) -> Result<RuntimeCapabilitiesV1, StoreError> {
    domain::brief::content(&brief.content, &brief.bindings)?;
    crate::brief::validate_refs(tx, brief.project_id, &brief.content, &brief.bindings).await?;
    let policy = sqlx::query("SELECT selection_rule,split_policy,require_real_data,required_capabilities,minimum_observations FROM app.evaluation_policies WHERE id=$1 AND project_id=$2")
        .bind(brief.content.evaluation_policy_id.as_uuid()).bind(brief.project_id.as_uuid())
        .fetch_one(&mut **tx).await?;
    let selection: SelectionRuleV1 = serde_json::from_value(policy.try_get("selection_rule")?)
        .map_err(|_| StoreError::Integrity)?;
    let split: SplitPolicyV1 = serde_json::from_value(policy.try_get("split_policy")?)
        .map_err(|_| StoreError::Integrity)?;
    if selection.comparison_input_set_id != context.validation_input_set_id {
        return Err(invalid(
            "execution_context.validation_input_set_id",
            "POLICY_COMPARISON_INPUT_MISMATCH",
        )
        .into());
    }
    let family: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.experiment_families WHERE id=$1 AND project_id=$2 AND selection_policy_id=$3)")
        .bind(selection.family_id.as_uuid()).bind(brief.project_id.as_uuid())
        .bind(brief.content.evaluation_policy_id.as_uuid()).fetch_one(&mut **tx).await?;
    if !family {
        return Err(invalid("content.evaluation_policy_id", "POLICY_FAMILY_MISMATCH").into());
    }
    let require_real: bool = policy.try_get("require_real_data")?;
    if brief.content.horizon_kind == HorizonKind::FixedBars
        && split.label_horizon_observations != brief.content.horizon_value
    {
        return Err(invalid("content.horizon_value", "POLICY_LABEL_HORIZON_MISMATCH").into());
    }
    if split.kind == contracts::research::SplitKind::CpcvFixedHorizon
        && brief.content.horizon_kind != HorizonKind::FixedBars
    {
        return Err(invalid(
            "content.horizon_kind",
            "CPCV_REQUIRES_FIXED_OBSERVATION_HORIZON",
        )
        .into());
    }
    let mut cutoff = None;
    let mut previous_end: Option<DateTime<Utc>> = None;
    for (input, purpose, role) in [
        (
            context.discovery_input_set_id,
            "DISCOVERY",
            DataPartition::Discovery,
        ),
        (
            context.validation_input_set_id,
            "VALIDATION",
            DataPartition::Validation,
        ),
        (context.sealed_input_set_id, "SEALED", DataPartition::Sealed),
    ] {
        crate::research::revalidate_frozen_inputs(tx, input, brief.project_id, context.runtime_id)
            .await?;
        let r = sqlx::query("SELECT purpose,decision_cutoff,frozen_at FROM app.input_sets WHERE id=$1 AND project_id=$2 FOR SHARE")
            .bind(input.as_uuid()).bind(brief.project_id.as_uuid()).fetch_one(&mut **tx).await?;
        let decision: DateTime<Utc> = r.try_get("decision_cutoff")?;
        if r.try_get::<String, _>("purpose")? != purpose
            || r.try_get::<Option<DateTime<Utc>>, _>("frozen_at")?
                .is_none()
            || cutoff.is_some_and(|previous| previous != decision)
        {
            return Err(invalid(
                "execution_context",
                "INPUT_PURPOSE_CUTOFF_OR_FREEZE_MISMATCH",
            )
            .into());
        }
        cutoff = Some(decision);
        let expected = brief
            .bindings
            .iter()
            .filter(|binding| binding.role == role)
            .map(|binding| binding.dataset_revision_id)
            .collect::<BTreeSet<_>>();
        let items = sqlx::query("SELECT d.id,d.origin AS data_origin,d.pit_status,d.revision_policy,d.row_count,d.event_start,d.event_end,i.role FROM app.input_set_items i JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id WHERE i.input_set_id=$1")
            .bind(input.as_uuid()).fetch_all(&mut **tx).await?;
        let actual = items
            .iter()
            .map(|r| db::id(r.try_get("id")?))
            .collect::<Result<BTreeSet<_>, StoreError>>()?;
        if expected.is_empty()
            || expected != actual
            || items
                .iter()
                .any(|r| r.try_get::<String, _>("role").ok().as_deref() != Some(purpose))
        {
            return Err(
                invalid("execution_context", "EXACT_BRIEF_DATASET_BINDING_REQUIRED").into(),
            );
        }
        if role == DataPartition::Sealed && !actual.contains(&split.sealed_revision_id) {
            return Err(invalid(
                "execution_context.sealed_input_set_id",
                "POLICY_SEALED_REVISION_MISMATCH",
            )
            .into());
        }
        let starts = items
            .iter()
            .map(|r| r.try_get::<DateTime<Utc>, _>("event_start"))
            .collect::<Result<Vec<_>, _>>()?;
        let ends = items
            .iter()
            .map(|r| r.try_get::<DateTime<Utc>, _>("event_end"))
            .collect::<Result<Vec<_>, _>>()?;
        let start = starts.into_iter().min().ok_or(StoreError::Integrity)?;
        let end = ends.into_iter().max().ok_or(StoreError::Integrity)?;
        if previous_end.is_some_and(|previous| previous > start) {
            return Err(invalid("execution_context", "RESEARCH_PARTITIONS_OVERLAP").into());
        }
        previous_end = Some(end);
        for r in &items {
            if r.try_get::<i64, _>("row_count")?
                < i64::from(policy.try_get::<i32, _>("minimum_observations")?)
            {
                return Err(
                    invalid("execution_context", "INSUFFICIENT_REGISTERED_OBSERVATIONS").into(),
                );
            }
            if require_real
                && (r.try_get::<String, _>("data_origin")? != "REAL"
                    || r.try_get::<String, _>("pit_status")? != "VERIFIED"
                    || r.try_get::<String, _>("revision_policy")? != "AS_KNOWN_THEN")
            {
                return Err(
                    invalid("execution_context", "REAL_POINT_IN_TIME_DATA_REQUIRED").into(),
                );
            }
        }
    }
    let forward = brief
        .bindings
        .iter()
        .filter(|binding| binding.role == DataPartition::Forward)
        .map(|binding| InputItemV1::Dataset {
            dataset_revision_id: binding.dataset_revision_id,
            role: binding.role,
        })
        .collect::<Vec<_>>();
    if !forward.is_empty() {
        crate::research::validate_inputs(
            tx,
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: brief.project_id,
                purpose: InputPurpose::Forward,
                decision_cutoff: cutoff.ok_or(StoreError::Integrity)?,
                items: forward,
            },
            None,
            Some(context.runtime_id),
        )
        .await?;
    }
    let caps = crate::runtime::require_capabilities(
        tx,
        context.runtime_id,
        context.runtime_revision,
        RunKind::AlphaEvaluate,
    )
    .await?;
    if !caps.job_kinds.contains(&RunKind::DataValidate) {
        return Err(DomainError::CapabilityUnavailable("data_validation_not_supported").into());
    }
    let intervals = &caps.label_interval_support;
    let supported = match brief.content.horizon_kind {
        HorizonKind::FixedBars => intervals.fixed_bars,
        HorizonKind::FixedDuration => intervals.fixed_duration,
        HorizonKind::VariableInterval => intervals.variable_interval,
    };
    if !supported {
        return Err(invalid("content.horizon_kind", "UNSUPPORTED_LABEL_INTERVALS").into());
    }
    let required: Vec<String> = policy.try_get("required_capabilities")?;
    let mut available = caps
        .job_kinds
        .iter()
        .map(db::code)
        .collect::<Result<Vec<_>, _>>()?;
    available.extend(caps.solver_capabilities.iter().cloned());
    if required
        .iter()
        .any(|capability| !available.contains(capability))
    {
        return Err(invalid(
            "content.evaluation_policy_id",
            "REQUIRED_NATIVE_CAPABILITY_UNAVAILABLE",
        )
        .into());
    }
    let assumptions = sqlx::query("SELECT engine_image_ref,cost_assumption_status,calendar_version FROM app.execution_assumptions WHERE id=$1")
        .bind(brief.content.execution_assumptions_id.as_uuid()).fetch_one(&mut **tx).await?;
    let image: String = assumptions.try_get("engine_image_ref")?;
    if !caps
        .image_refs
        .iter()
        .any(|native| native.job_kind == RunKind::AlphaEvaluate && native.image_ref == image)
        || assumptions.try_get::<String, _>("cost_assumption_status")? == "INSUFFICIENT"
    {
        return Err(invalid(
            "content.execution_assumptions_id",
            "NATIVE_IMAGE_OR_COST_ASSUMPTION_UNAVAILABLE",
        )
        .into());
    }
    let universe = sqlx::query("SELECT selection_asof,calendar_version,has_historical_membership FROM app.universe_versions WHERE id=$1")
        .bind(brief.content.universe_version_id.as_uuid()).fetch_one(&mut **tx).await?;
    if universe.try_get::<DateTime<Utc>, _>("selection_asof")?
        > cutoff.ok_or(StoreError::Integrity)?
        || universe.try_get::<String, _>("calendar_version")?
            != assumptions.try_get::<String, _>("calendar_version")?
        || (require_real && !universe.try_get::<bool, _>("has_historical_membership")?)
    {
        return Err(invalid(
            "content.universe_version_id",
            "UNIVERSE_TIME_OR_CALENDAR_MISMATCH",
        )
        .into());
    }
    Ok(caps)
}

fn cycle_view(row: &PgRow) -> Result<CycleViewV1, StoreError> {
    let outcome = row
        .try_get::<Option<String>, _>("outcome")?
        .map(|value| {
            serde_json::from_value(serde_json::Value::String(value))
                .map_err(|_| StoreError::Integrity)
        })
        .transpose()?;
    let counter = |name| -> Result<DbCounter, StoreError> {
        DbCounter::new(
            u64::try_from(row.try_get::<i64, _>(name)?).map_err(|_| StoreError::Integrity)?,
        )
        .map_err(|_| StoreError::Integrity)
    };
    Ok(CycleViewV1 {
        schema_version: SchemaV1,
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        brief_id: db::id(row.try_get("brief_id")?)?,
        ordinal: u32::try_from(row.try_get::<i32, _>("ordinal")?)
            .map_err(|_| StoreError::Integrity)?,
        revision: db::revision(row.try_get("revision")?)?,
        trigger: db::enum_value(row, "trigger")?,
        state: db::enum_value(row, "state")?,
        outcome,
        budget: serde_json::from_value(row.try_get("budget_snapshot")?)
            .map_err(|_| StoreError::Integrity)?,
        reserved_experiments: u32::try_from(row.try_get::<i64, _>("reserved_experiments")?)
            .map_err(|_| StoreError::Integrity)?,
        used_experiments: u32::try_from(row.try_get::<i64, _>("used_experiments")?)
            .map_err(|_| StoreError::Integrity)?,
        reserved_cpu_seconds: counter("reserved_cpu_seconds")?,
        initial_run_id: db::optional_id(row, "initial_run_id")?,
        next_action: row.try_get("next_action")?,
        started_at: row.try_get("started_at")?,
        ended_at: row.try_get("ended_at")?,
        created_at: row.try_get("created_at")?,
        available_actions: vec![
            CycleReadAction::ViewBrief,
            CycleReadAction::ViewRuns,
            CycleReadAction::ViewExperiments,
        ],
    })
}
const CYCLE: &str = "SELECT c.*,s.initial_run_id FROM app.research_cycles c LEFT JOIN app.cycle_startups s ON s.cycle_id=c.id";

impl Store {
    pub async fn freeze_brief(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &BriefFreezeV1,
    ) -> Result<CommandResult<FrozenBriefV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::BriefFreeze,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.research_briefs WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let project = db::id(project)?;
        crate::research::project_for_write(&mut tx, project).await?;
        let row = crate::brief::row(&mut tx, id, true).await?;
        let brief = crate::brief::view(&mut tx, &row).await?;
        if brief.revision != request.expected_revision {
            return Err(StoreError::RevisionConflict {
                current: brief.revision,
            });
        }
        if brief.state != BriefState::Draft {
            return Err(invalid("state", "BRIEF_ALREADY_FROZEN").into());
        }
        let context = &request.execution_context;
        validate_execution_context(&mut tx, &brief, context).await?;
        sqlx::query("INSERT INTO app.brief_execution_contexts(brief_id,project_id,runtime_id,runtime_revision,discovery_input_set_id,validation_input_set_id,sealed_input_set_id) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(id.as_uuid()).bind(project.as_uuid()).bind(context.runtime_id.as_uuid())
            .bind(context.runtime_revision.get() as i64).bind(context.discovery_input_set_id.as_uuid())
            .bind(context.validation_input_set_id.as_uuid()).bind(context.sealed_input_set_id.as_uuid())
            .execute(&mut *tx).await?;
        sqlx::query(
            "UPDATE app.research_briefs SET state='FROZEN',frozen_at=clock_timestamp() WHERE id=$1",
        )
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE app.projects SET current_brief_id=$2 WHERE id=$1")
            .bind(project.as_uuid())
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await?;
        let row = crate::brief::row(&mut tx, id, false).await?;
        let resource = FrozenBriefV1 {
            schema_version: SchemaV1,
            brief: crate::brief::view(&mut tx, &row).await?,
            execution_context: context.clone(),
        };
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, resource, 200).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn frozen_brief(&self, actor: &Actor, id: Id) -> Result<FrozenBriefV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.research_briefs WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        authority::read_project(&mut tx, actor, db::id(project)?, MachineScope::ResearchRead)
            .await?;
        let row = crate::brief::row(&mut tx, id, false).await?;
        let brief = crate::brief::view(&mut tx, &row).await?;
        if brief.state != BriefState::Frozen {
            return Err(StoreError::NotFound);
        }
        let context = execution_context(&mut tx, id).await?;
        tx.commit().await?;
        Ok(FrozenBriefV1 {
            schema_version: SchemaV1,
            brief,
            execution_context: context,
        })
    }

    pub async fn start_cycle(
        &self,
        actor: &Actor,
        key: &str,
        request: &CycleStartIntent,
    ) -> Result<CommandResult<CycleStartedV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::CycleStart,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let p = sqlx::query("SELECT revision,state FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(request.project_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let current = db::revision(p.try_get("revision")?)?;
        if current != request.request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        if p.try_get::<String, _>("state")? != "ACTIVE" {
            return Err(DomainError::AdmissionClosed.into());
        }
        let row = crate::brief::row(&mut tx, request.request.brief_id, false).await?;
        let brief = crate::brief::view(&mut tx, &row).await?;
        if brief.project_id != request.project_id || brief.state != BriefState::Frozen {
            return Err(invalid("brief_id", "OWNED_FROZEN_BRIEF_REQUIRED").into());
        }
        let context = execution_context(&mut tx, brief.id).await?;
        let caps = validate_execution_context(&mut tx, &brief, &context).await?;
        crate::runtime::require_capabilities(
            &mut tx,
            context.runtime_id,
            context.runtime_revision,
            RunKind::DataValidate,
        )
        .await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let today: i64 = sqlx::query_scalar("SELECT count(*) FROM app.research_cycles WHERE project_id=$1 AND created_at>=date_trunc('day',$2::timestamptz,'UTC') AND created_at<date_trunc('day',$2::timestamptz,'UTC')+interval '1 day'")
            .bind(request.project_id.as_uuid()).bind(now).fetch_one(&mut *tx).await?;
        let budget = &brief.content.budget;
        if today >= i64::from(budget.max_cycles_per_day) {
            return Err(DomainError::BudgetExhausted("cycles_per_day").into());
        }
        let previous: i32 = sqlx::query_scalar(
            "SELECT coalesce(max(ordinal),0) FROM app.research_cycles WHERE project_id=$1",
        )
        .bind(request.project_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let ordinal = previous.checked_add(1).ok_or(StoreError::Integrity)?;
        let cycle = prepared.target;
        sqlx::query("INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot,started_at,next_action) VALUES($1,$2,$3,$4,'OPERATOR','RUNNING',$5,$6,'DATA_VALIDATE')")
            .bind(cycle.as_uuid()).bind(request.project_id.as_uuid()).bind(brief.id.as_uuid())
            .bind(ordinal).bind(db::json(budget)?).bind(now).execute(&mut *tx).await?;
        // Reserve a bounded preparation slice, not all remaining research CPU.
        let cpu = (budget.max_cpu_seconds.get() / 10).clamp(1, 300);
        let limits = JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: DbCounter::new(cpu).map_err(|_| StoreError::Integrity)?,
            wall_seconds: budget.max_wall_seconds.min(caps.max_wall_seconds).min(300),
            memory_mib: budget.max_memory_mib.min(caps.max_memory_mib).min(2048),
            output_bytes: DbCounter::new(
                budget
                    .max_output_bytes
                    .get()
                    .min(caps.max_output_bytes.get())
                    .min(2 * 1024 * 1024),
            )
            .map_err(|_| StoreError::Integrity)?,
        };
        let (mut tx, admitted) = Self::enqueue_run_in_transaction(
            tx,
            key,
            &RunSubmission {
                cycle_id: cycle,
                input_set_id: context.discovery_input_set_id,
                runtime_id: context.runtime_id,
                runtime_revision: context.runtime_revision,
                kind: RunKind::DataValidate,
                limits,
            },
        )
        .await?;
        sqlx::query(
            "INSERT INTO app.cycle_startups(cycle_id,project_id,initial_run_id) VALUES($1,$2,$3)",
        )
        .bind(cycle.as_uuid())
        .bind(request.project_id.as_uuid())
        .bind(admitted.resource.id.as_uuid())
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query(&format!("{CYCLE} WHERE c.id=$1"))
            .bind(cycle.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let resource = CycleStartedV1 {
            schema_version: SchemaV1,
            cycle: cycle_view(&row)?,
            run: admitted.resource,
        };
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, resource, 202).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn cycle(&self, actor: &Actor, id: Id) -> Result<CycleViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.research_cycles WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        authority::read_project(&mut tx, actor, db::id(project)?, MachineScope::ResearchRead)
            .await?;
        let row = sqlx::query(&format!("{CYCLE} WHERE c.id=$1"))
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let result = cycle_view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn cycles(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<CycleViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, project, MachineScope::ResearchRead).await?;
        let rows = sqlx::query(&format!("{CYCLE} WHERE c.project_id=$1 AND ($2::uuid IS NULL OR c.id<$2) ORDER BY c.id DESC LIMIT $3"))
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1)
            .fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(cycle_view).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |cycle| cycle.id,
        );
        tx.commit().await?;
        Ok(result)
    }
}
