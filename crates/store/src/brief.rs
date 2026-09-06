//! Draft authoring shares the existing operator command transaction and parent locks.
use crate::{
    authority::{self, Actor},
    commands,
    control::page,
    db, Store, StoreError,
};
use contracts::{
    brief::*,
    control::{CommandResult, ListQuery, MachineScope, OperatorOperation, Page},
    research::SelectionRuleV1,
    DbCounter, Id,
};
use domain::research::invalid;
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;
const FIELDS: &str = "id,project_id,version,revision,state,hypothesis,economic_rationale,universe_version_id,target_kind,horizon_kind,horizon_value,base_currency,benchmark_ref,evaluation_policy_id,execution_assumptions_id,budget,stop_rule,supersedes_id,frozen_at,created_at,updated_at";
async fn row(tx: &mut Tx<'_>, id: Id, write: bool) -> Result<PgRow, StoreError> {
    sqlx::query(&format!(
        "SELECT {FIELDS} FROM app.research_briefs WHERE id=$1 FOR {}",
        if write { "UPDATE" } else { "SHARE" }
    ))
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::NotFound)
}
async fn view(tx: &mut Tx<'_>, r: &PgRow) -> Result<BriefView, StoreError> {
    let id = db::id(r.try_get("id")?)?;
    let bindings = sqlx::query("SELECT dataset_revision_id,role,access_policy FROM app.brief_data_bindings WHERE brief_id=$1 ORDER BY dataset_revision_id")
        .bind(id.as_uuid()).fetch_all(&mut **tx).await?.iter().map(|b| Ok(BriefBindingV1 {
            dataset_revision_id: db::id(b.try_get("dataset_revision_id")?)?,
            role: db::enum_value(b,"role")?, access_policy: db::enum_value(b,"access_policy")?,
        })).collect::<Result<Vec<_>,StoreError>>()?;
    Ok(BriefView {
        id,
        project_id: db::id(r.try_get("project_id")?)?,
        version: u32::try_from(r.try_get::<i32, _>("version")?)
            .map_err(|_| StoreError::Integrity)?,
        revision: db::revision(r.try_get("revision")?)?,
        state: db::enum_value(r, "state")?,
        content: BriefContentV1 {
            hypothesis: r.try_get("hypothesis")?,
            economic_rationale: r.try_get("economic_rationale")?,
            universe_version_id: db::id(r.try_get("universe_version_id")?)?,
            target_kind: db::enum_value(r, "target_kind")?,
            horizon_kind: db::enum_value(r, "horizon_kind")?,
            horizon_value: r
                .try_get::<Option<i64>, _>("horizon_value")?
                .map(|v| {
                    let value = u64::try_from(v).map_err(|_| StoreError::Integrity)?;
                    DbCounter::new(value).map_err(|_| StoreError::Integrity)
                })
                .transpose()?,
            base_currency: r.try_get("base_currency")?,
            benchmark_ref: db::optional_id(r, "benchmark_ref")?,
            evaluation_policy_id: db::id(r.try_get("evaluation_policy_id")?)?,
            execution_assumptions_id: db::id(r.try_get("execution_assumptions_id")?)?,
            budget: serde_json::from_value(r.try_get("budget")?)
                .map_err(|_| StoreError::Integrity)?,
            stop_rule: serde_json::from_value(r.try_get("stop_rule")?)
                .map_err(|_| StoreError::Integrity)?,
        },
        bindings,
        supersedes_id: db::optional_id(r, "supersedes_id")?,
        frozen_at: r.try_get("frozen_at")?,
        created_at: r.try_get("created_at")?,
        updated_at: r.try_get("updated_at")?,
    })
}
async fn validate_refs(
    tx: &mut Tx<'_>,
    project: Id,
    content: &BriefContentV1,
    bindings: &[BriefBindingV1],
) -> Result<(), StoreError> {
    let universe: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.universe_versions WHERE id=$1)")
            .bind(content.universe_version_id.as_uuid())
            .fetch_one(&mut **tx)
            .await?;
    if !universe {
        return Err(invalid("content.universe_version_id", "REFERENCE_UNAVAILABLE").into());
    }
    let policy = sqlx::query(
        "SELECT selection_rule FROM app.evaluation_policies WHERE id=$1 AND project_id=$2",
    )
    .bind(content.evaluation_policy_id.as_uuid())
    .bind(project.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("content.evaluation_policy_id", "REFERENCE_UNAVAILABLE"))?;
    let selection: SelectionRuleV1 = serde_json::from_value(policy.try_get("selection_rule")?)
        .map_err(|_| StoreError::Integrity)?;
    if selection.execution_assumptions_id != content.execution_assumptions_id {
        return Err(invalid(
            "content.execution_assumptions_id",
            "POLICY_EXECUTION_MISMATCH",
        )
        .into());
    }
    let currency: Option<String> =
        sqlx::query_scalar("SELECT base_currency FROM app.execution_assumptions WHERE id=$1")
            .bind(content.execution_assumptions_id.as_uuid())
            .fetch_optional(&mut **tx)
            .await?;
    if currency.as_deref() != Some(&content.base_currency) {
        return Err(invalid("content.base_currency", "EXECUTION_CURRENCY_MISMATCH").into());
    }
    if let Some(benchmark) = content.benchmark_ref {
        let currency: Option<String> =
            sqlx::query_scalar("SELECT currency FROM app.benchmark_versions WHERE id=$1")
                .bind(benchmark.as_uuid())
                .fetch_optional(&mut **tx)
                .await?;
        if currency.as_deref() != Some(&content.base_currency) {
            return Err(invalid("content.benchmark_ref", "BENCHMARK_CURRENCY_MISMATCH").into());
        }
    }
    for (index, binding) in bindings.iter().enumerate() {
        let field = format!("bindings.{index}.dataset_revision_id");
        let r = sqlx::query(
            "SELECT universe_version_id,partition_role FROM app.dataset_revisions WHERE id=$1",
        )
        .bind(binding.dataset_revision_id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| invalid(&field, "REFERENCE_UNAVAILABLE"))?;
        if db::id(r.try_get("universe_version_id")?)? != content.universe_version_id
            || r.try_get::<String, _>("partition_role")? != binding.role.code()
        {
            return Err(invalid(field, "UNIVERSE_OR_PARTITION_MISMATCH").into());
        }
    }
    Ok(())
}
async fn bind_members(
    tx: &mut Tx<'_>,
    id: Id,
    bindings: &[BriefBindingV1],
) -> Result<(), StoreError> {
    for b in bindings {
        sqlx::query("INSERT INTO app.brief_data_bindings(brief_id,dataset_revision_id,role,access_policy) VALUES($1,$2,$3,$4)")
            .bind(id.as_uuid()).bind(b.dataset_revision_id.as_uuid()).bind(b.role.code()).bind(db::code(&b.access_policy)?).execute(&mut **tx).await?;
    }
    Ok(())
}
impl Store {
    pub async fn briefs(
        &self,
        actor: &Actor,
        project: Id,
        q: &ListQuery,
    ) -> Result<Page<BriefView>, StoreError> {
        domain::control::list(q)?;
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, project, MachineScope::ResearchRead).await?;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.projects WHERE id=$1)")
                .bind(project.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let rows=sqlx::query(&format!("SELECT {FIELDS} FROM app.research_briefs WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3 FOR SHARE"))
            .bind(project.as_uuid()).bind(q.cursor.map(Id::as_uuid)).bind(i64::from(q.limit)+1).fetch_all(&mut *tx).await?;
        let mut items = Vec::with_capacity(rows.len());
        for r in rows {
            items.push(view(&mut tx, &r).await?);
        }
        let result = page(items, q.limit, |b| b.id);
        tx.commit().await?;
        Ok(result)
    }
    pub async fn brief(&self, actor: &Actor, id: Id) -> Result<BriefView, StoreError> {
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.research_briefs WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        authority::read_project(&mut tx, actor, db::id(project)?, MachineScope::ResearchRead)
            .await?;
        let r = row(&mut tx, id, false).await?;
        let result = view(&mut tx, &r).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn create_brief(
        &self,
        actor: &Actor,
        key: &str,
        request: &BriefCreateIntent,
    ) -> Result<CommandResult<BriefView>, StoreError> {
        let body = &request.request;
        let c = &body.content;
        domain::brief::content(c, &body.bindings)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::BriefCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        crate::research::project_for_write(&mut tx, request.project_id).await?;
        if let Some(supersedes) = body.supersedes_id {
            let same: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM app.research_briefs WHERE id=$1 AND project_id=$2)",
            )
            .bind(supersedes.as_uuid())
            .bind(request.project_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
            if !same {
                return Err(invalid("supersedes_id", "REFERENCE_UNAVAILABLE").into());
            }
        }
        validate_refs(&mut tx, request.project_id, c, &body.bindings).await?;
        let previous: i32 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version),0) FROM app.research_briefs WHERE project_id=$1",
        )
        .bind(request.project_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let version = previous.checked_add(1).ok_or(StoreError::Integrity)?;
        let id = prepared.target;
        sqlx::query("INSERT INTO app.research_briefs(id,project_id,version,hypothesis,economic_rationale,universe_version_id,target_kind,horizon_kind,horizon_value,base_currency,benchmark_ref,evaluation_policy_id,execution_assumptions_id,budget,stop_rule,state,supersedes_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,'DRAFT',$16)")
            .bind(id.as_uuid()).bind(request.project_id.as_uuid()).bind(version).bind(&c.hypothesis).bind(&c.economic_rationale).bind(c.universe_version_id.as_uuid()).bind(db::code(&c.target_kind)?).bind(db::code(&c.horizon_kind)?).bind(c.horizon_value.map(|v|v.get() as i64)).bind(&c.base_currency).bind(c.benchmark_ref.map(Id::as_uuid)).bind(c.evaluation_policy_id.as_uuid()).bind(c.execution_assumptions_id.as_uuid()).bind(db::json(&c.budget)?).bind(db::json(&c.stop_rule)?).bind(body.supersedes_id.map(Id::as_uuid)).execute(&mut *tx).await?;
        bind_members(&mut tx, id, &body.bindings).await?;
        let r = row(&mut tx, id, false).await?;
        let resource = view(&mut tx, &r).await?;
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn update_brief(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &BriefUpdate,
    ) -> Result<CommandResult<BriefView>, StoreError> {
        let c = &request.content;
        domain::brief::content(c, &request.bindings)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::BriefUpdate,
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
        let old = row(&mut tx, id, true).await?;
        let revision = db::revision(old.try_get("revision")?)?;
        if revision != request.expected_revision {
            return Err(StoreError::RevisionConflict { current: revision });
        }
        if old.try_get::<String, _>("state")? != "DRAFT" {
            return Err(invalid("state", "BRIEF_ALREADY_FROZEN").into());
        }
        validate_refs(&mut tx, project, c, &request.bindings).await?;
        sqlx::query("UPDATE app.research_briefs SET hypothesis=$2,economic_rationale=$3,universe_version_id=$4,target_kind=$5,horizon_kind=$6,horizon_value=$7,base_currency=$8,benchmark_ref=$9,evaluation_policy_id=$10,execution_assumptions_id=$11,budget=$12,stop_rule=$13 WHERE id=$1")
            .bind(id.as_uuid()).bind(&c.hypothesis).bind(&c.economic_rationale).bind(c.universe_version_id.as_uuid()).bind(db::code(&c.target_kind)?).bind(db::code(&c.horizon_kind)?).bind(c.horizon_value.map(|v|v.get() as i64)).bind(&c.base_currency).bind(c.benchmark_ref.map(Id::as_uuid)).bind(c.evaluation_policy_id.as_uuid()).bind(c.execution_assumptions_id.as_uuid()).bind(db::json(&c.budget)?).bind(db::json(&c.stop_rule)?).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM app.brief_data_bindings WHERE brief_id=$1")
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await?;
        bind_members(&mut tx, id, &request.bindings).await?;
        let r = row(&mut tx, id, false).await?;
        let resource = view(&mut tx, &r).await?;
        let result = commands::finish(&mut tx, prepared, resource, 200).await?;
        tx.commit().await?;
        Ok(result)
    }
}
