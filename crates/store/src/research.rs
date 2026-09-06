//! Atomic research preparation using existing operator receipts and native row locks.
//! No data bytes, split algorithm, native capability claims or model side effects.
use crate::{
    authority::{self, Actor},
    commands,
    control::page,
    db, Store, StoreError,
};
use contracts::{
    control::{CommandResult, MachineScope, OperatorOperation, Page},
    research::*,
    DbCounter, Id, SchemaV1, Timestamp,
};
use domain::research::invalid;
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use std::collections::BTreeSet;

const INPUT: &str = "id,project_id,purpose,decision_cutoff,frozen_at,revision,created_at";
const POLICY: &str = "p.id,p.project_id,p.version,p.created_at,p.selection_rule,p.split_policy,p.metric_requirements,p.minimum_observations,p.maximum_missing_fraction,p.require_real_data,p.required_capabilities,p.maximum_sealed_uses_per_lineage,p.validity_seconds,f.question,f.project_id AS family_project_id,f.root_lineage_id AS family_root_id,f.selection_policy_id AS family_policy_id";
const FAMILY: &str =
    "JOIN app.experiment_families f ON f.id=p.family_id AND f.project_id=p.project_id AND f.selection_policy_id=p.id AND f.root_lineage_id=p.root_lineage_id";

fn summary(r: &PgRow) -> Result<InputSetSummary, StoreError> {
    Ok(InputSetSummary {
        id: db::id(r.try_get("id")?)?,
        project_id: db::id(r.try_get("project_id")?)?,
        purpose: db::enum_value(r, "purpose")?,
        decision_cutoff: r.try_get("decision_cutoff")?,
        frozen_at: r
            .try_get::<Option<Timestamp>, _>("frozen_at")?
            .ok_or(StoreError::Integrity)?,
        revision: db::revision(r.try_get("revision")?)?,
        created_at: r.try_get("created_at")?,
    })
}
fn policy(r: &PgRow) -> Result<EvaluationPolicyView, StoreError> {
    let selection: SelectionRuleV1 =
        serde_json::from_value(r.try_get("selection_rule")?).map_err(|_| StoreError::Integrity)?;
    let id = db::id(r.try_get("id")?)?;
    let project = db::id(r.try_get("project_id")?)?;
    if db::optional_id(r, "family_project_id")? != Some(project)
        || db::optional_id(r, "family_root_id")? != Some(selection.root_lineage_id)
        || db::optional_id(r, "family_policy_id")? != Some(id)
    {
        return Err(StoreError::Integrity);
    }
    Ok(EvaluationPolicyView {
        id,
        project_id: project,
        version: r
            .try_get::<i32, _>("version")?
            .try_into()
            .map_err(|_| StoreError::Integrity)?,
        created_at: r.try_get("created_at")?,
        question: r
            .try_get::<Option<String>, _>("question")?
            .ok_or(StoreError::Integrity)?,
        selection_rule: selection,
        split_policy: serde_json::from_value(r.try_get("split_policy")?)
            .map_err(|_| StoreError::Integrity)?,
        metric_requirements: serde_json::from_value(r.try_get("metric_requirements")?)
            .map_err(|_| StoreError::Integrity)?,
        minimum_observations: r
            .try_get::<i32, _>("minimum_observations")?
            .try_into()
            .map_err(|_| StoreError::Integrity)?,
        maximum_missing_fraction: r
            .try_get::<bigdecimal::BigDecimal, _>("maximum_missing_fraction")?
            .to_plain_string()
            .parse()
            .map_err(|_| StoreError::Integrity)?,
        require_real_data: r.try_get("require_real_data")?,
        required_capabilities: r.try_get("required_capabilities")?,
        maximum_sealed_uses_per_lineage: r
            .try_get::<i32, _>("maximum_sealed_uses_per_lineage")?
            .try_into()
            .map_err(|_| StoreError::Integrity)?,
        validity_seconds: DbCounter::new(r.try_get::<i64, _>("validity_seconds")? as u64)
            .map_err(|_| StoreError::Integrity)?,
    })
}
async fn input(tx: &mut Transaction<'_, Postgres>, id: Id) -> Result<InputSetView, StoreError> {
    let r = sqlx::query(&format!(
        "SELECT {INPUT} FROM app.input_sets WHERE id=$1 AND frozen_at IS NOT NULL"
    ))
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::NotFound)?;
    let header = summary(&r)?;
    let rows=sqlx::query("SELECT i.id,i.ordinal,i.dataset_revision_id,i.artifact_id,i.role,COALESCE(d.origin,a.origin) AS origin,d.pit_status FROM app.input_set_items i LEFT JOIN app.dataset_revisions d ON d.id=i.dataset_revision_id LEFT JOIN app.artifacts a ON a.id=i.artifact_id WHERE i.input_set_id=$1 ORDER BY i.ordinal LIMIT 257")
        .bind(id.as_uuid()).fetch_all(&mut **tx).await?;
    if !(1..=256).contains(&rows.len()) {
        return Err(StoreError::Integrity);
    }
    let mut items = Vec::with_capacity(rows.len());
    for (index, r) in rows.iter().enumerate() {
        let ordinal = r.try_get::<i32, _>("ordinal")?;
        if ordinal != index as i32 {
            return Err(StoreError::Integrity);
        }
        let item = if let Some(id) = db::optional_id(r, "dataset_revision_id")? {
            InputItemV1::Dataset {
                dataset_revision_id: id,
                role: db::enum_value(r, "role")?,
            }
        } else {
            InputItemV1::Artifact {
                artifact_id: db::optional_id(r, "artifact_id")?.ok_or(StoreError::Integrity)?,
                role: db::enum_value(r, "role")?,
            }
        };
        items.push(InputItemView {
            id: db::id(r.try_get("id")?)?,
            ordinal: ordinal as u16,
            item,
            origin: db::enum_value(r, "origin")?,
            pit_status: r
                .try_get::<Option<String>, _>("pit_status")?
                .map(|s| {
                    serde_json::from_value(serde_json::Value::String(s))
                        .map_err(|_| StoreError::Integrity)
                })
                .transpose()?,
        });
    }
    Ok(InputSetView { header, items })
}
async fn project_for_write(
    tx: &mut Transaction<'_, Postgres>,
    project: Id,
) -> Result<Id, StoreError> {
    let r = sqlx::query("SELECT root_lineage_id,state FROM app.projects WHERE id=$1 FOR UPDATE")
        .bind(project.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    if r.try_get::<String, _>("state")? == "ARCHIVED" {
        return Err(domain::DomainError::AdmissionClosed.into());
    }
    db::id(r.try_get("root_lineage_id")?)
}
// Input membership contains no permission in itself. Revalidate current source,
// runtime and grant authority after obtaining all locks, including after waits.
async fn validate_inputs(
    tx: &mut Transaction<'_, Postgres>,
    request: &InputSetCreate,
    extra_sealed: Option<Id>,
) -> Result<(), StoreError> {
    domain::research::input_set(request)?;
    let mut datasets = Vec::new();
    for (index, item) in request.items.iter().enumerate() {
        match item {
            InputItemV1::Dataset {
                dataset_revision_id,
                role,
            } => datasets.push((
                *dataset_revision_id,
                *role,
                format!("items.{index}.dataset_revision_id"),
                Some(request.decision_cutoff),
                request.purpose,
            )),
            InputItemV1::Artifact { artifact_id, role } => {
                let r = sqlx::query(
                    "SELECT project_id,kind,access_class FROM app.artifacts WHERE id=$1",
                )
                .bind(artifact_id.as_uuid())
                .fetch_optional(&mut **tx)
                .await?
                .ok_or_else(|| {
                    invalid(
                        format!("items.{index}.artifact_id"),
                        "REFERENCE_UNAVAILABLE",
                    )
                })?;
                if db::optional_id(&r, "project_id")? != Some(request.project_id)
                    || r.try_get::<String, _>("kind")? != role.code()
                    || (r.try_get::<String, _>("access_class")? == "EVALUATOR_ONLY"
                        && request.purpose != InputPurpose::Sealed)
                {
                    return Err(
                        invalid(format!("items.{index}.artifact_id"), "ARTIFACT_BINDING").into(),
                    );
                }
            }
        }
    }
    if let Some(id) = extra_sealed {
        datasets.push((
            id,
            DataPartition::Sealed,
            "split_policy.sealed_revision_id".into(),
            None,
            InputPurpose::Sealed,
        ));
    }
    let mut facts = Vec::with_capacity(datasets.len());
    let mut source_ids = BTreeSet::new();
    let mut grant_ids = BTreeSet::new();
    for (id, role, field, cutoff, purpose) in datasets {
        let r=sqlx::query("SELECT source_id,data_use_grant_id,partition_role,available_through,pit_status FROM app.dataset_revisions WHERE id=$1")
            .bind(id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or_else(|| invalid(&field,"REFERENCE_UNAVAILABLE"))?;
        let available: Timestamp = r.try_get("available_through")?;
        if r.try_get::<String, _>("partition_role")? != role.code()
            || r.try_get::<String, _>("pit_status")? == "INVALID"
            || cutoff.is_some_and(|cutoff| available > cutoff)
        {
            return Err(invalid(&field, "DATASET_PARTITION_OR_ASOF").into());
        }
        let source: uuid::Uuid = r.try_get("source_id")?;
        let grant: uuid::Uuid = r.try_get("data_use_grant_id")?;
        source_ids.insert(source);
        grant_ids.insert(grant);
        facts.push((source, grant, field, purpose));
    }
    let sources = sqlx::query(
        "SELECT id,runtime_id,enabled FROM app.data_sources WHERE id=ANY($1) ORDER BY id FOR SHARE",
    )
    .bind(source_ids.iter().copied().collect::<Vec<_>>())
    .fetch_all(&mut **tx)
    .await?;
    let runtimes = sqlx::query(
        "SELECT id,enabled FROM app.runtime_integrations WHERE id=ANY($1) ORDER BY id FOR SHARE",
    )
    .bind(
        sources
            .iter()
            .map(|r| r.try_get::<uuid::Uuid, _>("runtime_id"))
            .collect::<Result<Vec<_>, _>>()?,
    )
    .fetch_all(&mut **tx)
    .await?;
    let grants=sqlx::query("SELECT id,source_id,valid_from,valid_until,allowed_uses FROM app.data_use_grants WHERE id=ANY($1) ORDER BY id FOR SHARE")
        .bind(grant_ids.iter().copied().collect::<Vec<_>>()).fetch_all(&mut **tx).await?;
    // A separate statement AFTER LockRows waits establishes current time and
    // committed revocations, not the stale snapshot from before taking the lock.
    let now: Timestamp = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if request.decision_cutoff > now {
        return Err(invalid("decision_cutoff", "FUTURE_CUTOFF").into());
    }
    let revoked:Vec<uuid::Uuid>=sqlx::query_scalar("SELECT DISTINCT grant_id FROM app.data_use_revocations WHERE grant_id=ANY($1) AND effective_at<=$2")
        .bind(grant_ids.into_iter().collect::<Vec<_>>()).bind(now).fetch_all(&mut **tx).await?;
    for (source, grant, field, purpose) in facts {
        let s = sources
            .iter()
            .find(|r| r.try_get::<uuid::Uuid, _>("id").ok() == Some(source))
            .ok_or(StoreError::Integrity)?;
        let runtime: uuid::Uuid = s.try_get("runtime_id")?;
        let r = runtimes
            .iter()
            .find(|r| r.try_get::<uuid::Uuid, _>("id").ok() == Some(runtime))
            .ok_or(StoreError::Integrity)?;
        let g = grants
            .iter()
            .find(|r| r.try_get::<uuid::Uuid, _>("id").ok() == Some(grant))
            .ok_or(StoreError::Integrity)?;
        if !s.try_get::<bool, _>("enabled")? || !r.try_get::<bool, _>("enabled")? {
            return Err(invalid(&field, "SOURCE_DISABLED").into());
        }
        if g.try_get::<uuid::Uuid, _>("source_id")? != source
            || g.try_get::<Timestamp, _>("valid_from")? > now
            || g.try_get::<Option<Timestamp>, _>("valid_until")?
                .is_some_and(|end| end <= now)
            || revoked.contains(&grant)
        {
            return Err(invalid(&field, "DATA_USE_NOT_AUTHORIZED").into());
        }
        let allowed: DataUse = db::enum_value(g, "allowed_uses")?;
        if !allowed.permits_preparation(purpose) {
            return Err(invalid(&field, "DATA_USE_PURPOSE_NOT_AUTHORIZED").into());
        }
    }
    Ok(())
}

impl Store {
    pub async fn input_sets(
        &self,
        actor: &Actor,
        q: &ResearchListQuery,
    ) -> Result<Page<InputSetSummary>, StoreError> {
        if !(1..=100).contains(&q.limit) {
            return Err(invalid("limit", "PAGE_SIZE").into());
        }
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, q.project_id, MachineScope::ResearchRead).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.projects WHERE id=$1)")
                .bind(q.project_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let rows=sqlx::query(&format!("SELECT {INPUT} FROM app.input_sets WHERE project_id=$1 AND frozen_at IS NOT NULL AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3"))
            .bind(q.project_id.as_uuid()).bind(q.cursor.map(Id::as_uuid)).bind(i64::from(q.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(summary).collect::<Result<Vec<_>, _>>()?,
            q.limit,
            |r| r.id,
        );
        tx.commit().await?;
        Ok(result)
    }
    pub async fn input_set(&self, actor: &Actor, id: Id) -> Result<InputSetView, StoreError> {
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid = sqlx::query_scalar(
            "SELECT project_id FROM app.input_sets WHERE id=$1 AND frozen_at IS NOT NULL",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        authority::read_project(&mut tx, actor, db::id(project)?, MachineScope::ResearchRead)
            .await?;
        let result = input(&mut tx, id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn create_input_set(
        &self,
        actor: &Actor,
        key: &str,
        request: &InputSetCreate,
    ) -> Result<CommandResult<InputSetView>, StoreError> {
        domain::research::input_set(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::InputSetCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        project_for_write(&mut tx, request.project_id).await?;
        validate_inputs(&mut tx, request, None).await?;
        let id = prepared.target;
        sqlx::query(
            "INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff) VALUES($1,$2,$3,$4)",
        )
        .bind(id.as_uuid())
        .bind(request.project_id.as_uuid())
        .bind(request.purpose.code())
        .bind(request.decision_cutoff)
        .execute(&mut *tx)
        .await?;
        for (index, item) in request.items.iter().enumerate() {
            let (dataset, artifact, role) = match item {
                InputItemV1::Dataset {
                    dataset_revision_id,
                    role,
                } => (Some(dataset_revision_id.as_uuid()), None, role.code()),
                InputItemV1::Artifact { artifact_id, role } => {
                    (None, Some(artifact_id.as_uuid()), role.code())
                }
            };
            sqlx::query("INSERT INTO app.input_set_items(input_set_id,dataset_revision_id,artifact_id,role,ordinal) VALUES($1,$2,$3,$4,$5)")
                .bind(id.as_uuid()).bind(dataset).bind(artifact).bind(role).bind(index as i32).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE app.input_sets SET frozen_at=clock_timestamp() WHERE id=$1")
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await?;
        let resource = input(&mut tx, id).await?;
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn evaluation_policies(
        &self,
        actor: &Actor,
        q: &ResearchListQuery,
    ) -> Result<Page<EvaluationPolicyView>, StoreError> {
        if !(1..=100).contains(&q.limit) {
            return Err(invalid("limit", "PAGE_SIZE").into());
        }
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, q.project_id, MachineScope::ResearchRead).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.projects WHERE id=$1)")
                .bind(q.project_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let rows=sqlx::query(&format!("SELECT {POLICY} FROM app.evaluation_policies p {FAMILY} WHERE p.project_id=$1 AND ($2::uuid IS NULL OR p.id<$2) ORDER BY p.id DESC LIMIT $3"))
            .bind(q.project_id.as_uuid()).bind(q.cursor.map(Id::as_uuid)).bind(i64::from(q.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(policy).collect::<Result<Vec<_>, _>>()?,
            q.limit,
            |r| r.id,
        );
        tx.commit().await?;
        Ok(result)
    }
    pub async fn evaluation_policy(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<EvaluationPolicyView, StoreError> {
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.evaluation_policies WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        authority::read_project(&mut tx, actor, db::id(project)?, MachineScope::ResearchRead)
            .await?;
        let row = sqlx::query(&format!(
            "SELECT {POLICY} FROM app.evaluation_policies p {FAMILY} WHERE p.id=$1"
        ))
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let result = policy(&row)?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn create_evaluation_policy(
        &self,
        actor: &Actor,
        key: &str,
        request: &EvaluationPolicyCreate,
    ) -> Result<CommandResult<EvaluationPolicyView>, StoreError> {
        domain::research::evaluation_policy(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::EvaluationPolicyCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let root = project_for_write(&mut tx, request.project_id).await?;
        let comparison = input(&mut tx, request.comparison_input_set_id).await?;
        if comparison.header.project_id != request.project_id {
            return Err(invalid("comparison_input_set_id", "PROJECT_MISMATCH").into());
        }
        let purpose = match request.selection.evaluation_kind {
            SelectionEvaluationKind::WalkForward => InputPurpose::Validation,
            SelectionEvaluationKind::Sealed => InputPurpose::Sealed,
        };
        if comparison.header.purpose != purpose {
            return Err(invalid("comparison_input_set_id", "EVALUATION_PURPOSE_MISMATCH").into());
        }
        let items: Vec<_> = comparison.items.into_iter().map(|i| i.item).collect();
        let has_sealed=items.iter().any(|i| matches!(i,InputItemV1::Dataset{dataset_revision_id,..} if *dataset_revision_id==request.split_policy.sealed_revision_id));
        if (request.selection.evaluation_kind == SelectionEvaluationKind::Sealed) != has_sealed {
            return Err(invalid(
                "split_policy.sealed_revision_id",
                "SEALED_COMPARISON_BINDING",
            )
            .into());
        }
        validate_inputs(
            &mut tx,
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: request.project_id,
                purpose,
                decision_cutoff: comparison.header.decision_cutoff,
                items,
            },
            Some(request.split_policy.sealed_revision_id),
        )
        .await?;
        let assumptions=sqlx::query("SELECT a.project_id AS fee_project,l.project_id AS liquidity_project FROM app.execution_assumptions e JOIN app.artifacts a ON a.id=e.fee_schedule_artifact_id LEFT JOIN app.artifacts l ON l.id=e.liquidity_artifact_id WHERE e.id=$1")
            .bind(request.execution_assumptions_id.as_uuid()).fetch_optional(&mut *tx).await?
            .ok_or_else(|| invalid("execution_assumptions_id","REFERENCE_UNAVAILABLE"))?;
        if db::optional_id(&assumptions, "fee_project")?.is_some_and(|p| p != request.project_id)
            || db::optional_id(&assumptions, "liquidity_project")?
                .is_some_and(|p| p != request.project_id)
        {
            return Err(invalid("execution_assumptions_id", "PROJECT_MISMATCH").into());
        }
        let now: Timestamp = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if chrono::Duration::try_seconds(request.validity_seconds.get() as i64)
            .and_then(|d| now.checked_add_signed(d))
            .is_none()
        {
            return Err(invalid("validity_seconds", "TIME_RANGE").into());
        }
        let version:i64=sqlx::query_scalar("SELECT COALESCE(MAX(version),0)::bigint+1 FROM app.evaluation_policies WHERE project_id=$1")
            .bind(request.project_id.as_uuid()).fetch_one(&mut *tx).await?;
        let version =
            i32::try_from(version).map_err(|_| invalid("version", "VERSION_EXHAUSTED"))?;
        let id = prepared.target;
        let family = Id::new();
        let s = &request.selection;
        let selection = SelectionRuleV1 {
            schema_version: SchemaV1,
            comparable_scope: ComparableScope::FamilyLineage,
            root_lineage_id: root,
            family_id: family,
            comparison_input_set_id: request.comparison_input_set_id,
            execution_assumptions_id: request.execution_assumptions_id,
            evaluation_kind: s.evaluation_kind,
            metric_code: s.metric_code.clone(),
            metric_scope: s.metric_scope.clone(),
            method_id: s.method_id.clone(),
            method_version: s.method_version.clone(),
            unit: s.unit.clone(),
            frequency: s.frequency.clone(),
            direction: s.direction,
            candidate_count: s.candidate_count,
            tie_break: SelectionTieBreak::ExperimentIdAsc,
            missing_required_metric: MissingSelectionMetric::Inconclusive,
        };
        sqlx::query("INSERT INTO app.evaluation_policies(id,project_id,version,selection_rule,split_policy,metric_requirements,minimum_observations,maximum_missing_fraction,require_real_data,required_capabilities,maximum_sealed_uses_per_lineage,validity_seconds) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(id.as_uuid()).bind(request.project_id.as_uuid()).bind(version).bind(db::json(&selection)?).bind(db::json(&request.split_policy)?).bind(db::json(&request.metric_requirements)?)
            .bind(request.minimum_observations as i32).bind(request.maximum_missing_fraction.as_decimal()).bind(request.require_real_data).bind(&request.required_capabilities)
            .bind(request.maximum_sealed_uses_per_lineage as i32).bind(request.validity_seconds.get() as i64).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.experiment_families(id,project_id,root_lineage_id,question,selection_policy_id) VALUES($1,$2,$3,$4,$5)")
            .bind(family.as_uuid()).bind(request.project_id.as_uuid()).bind(root.as_uuid()).bind(&request.question).bind(id.as_uuid()).execute(&mut *tx).await?;
        let row = sqlx::query(&format!(
            "SELECT {POLICY} FROM app.evaluation_policies p {FAMILY} WHERE p.id=$1"
        ))
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let result = commands::finish(&mut tx, prepared, policy(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}
