//! Scoped proposal publication into the existing immutable experiment ledger.
use crate::{
    authority::{self, Actor},
    commands,
    control::page,
    db, Store, StoreError,
};
use contracts::{
    artifacts::ResearchArtifactKind,
    budget::{BudgetV1, StopRuleV1},
    control::{CommandResult, MachineScope, Page, PrincipalKind},
    experiments::*,
    research::ResearchListQuery,
    Id,
};
use domain::{research::invalid, DomainError};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;

// A source's existence alone never grants disclosure of a sealed conclusion.
// All referenced input/access metadata is immutable; the experiment is locked
// while this projection is read. No raw report, verifier or storage locator is selected.
const SELECT_VIEW: &str = "SELECT e.*, a.author_run_id, a.author_attempt_id,
  (e.outcome='PENDING' AND e.outcome_reason IS NULL AND e.conclusion_artifact_id IS NULL) AS pending,
  (a.experiment_id IS NOT NULL AND EXISTS (
    SELECT 1 FROM app.runs r JOIN app.input_sets i ON i.id=r.input_set_id
    WHERE r.id=e.run_id AND r.project_id=e.project_id AND r.cycle_id=e.cycle_id
      AND r.kind='ALPHA_EVALUATE' AND r.state IN ('SUCCEEDED','FAILED','CANCELLED')
      AND i.project_id=e.project_id AND i.frozen_at IS NOT NULL
      AND i.purpose IN ('DISCOVERY','VALIDATION')
      AND NOT EXISTS (
        SELECT 1 FROM app.input_set_items m
        LEFT JOIN app.dataset_revisions d ON d.id=m.dataset_revision_id
        LEFT JOIN app.artifacts x ON x.id=m.artifact_id
        WHERE m.input_set_id=i.id AND (d.partition_role='SEALED' OR x.access_class<>'RESEARCH')
      )
  ) AND (e.conclusion_artifact_id IS NULL OR EXISTS (
    SELECT 1 FROM app.artifacts report WHERE report.id=e.conclusion_artifact_id
      AND report.project_id=e.project_id AND report.producer_run_id=e.run_id
      AND report.access_class='RESEARCH' AND report.kind='REPORT'
  ))) AS research_visible
  FROM app.experiments e LEFT JOIN app.experiment_authorship a ON a.experiment_id=e.id";

fn view(row: &PgRow) -> Result<ExperimentView, StoreError> {
    let pending: bool = row.try_get("pending")?;
    let disclosed: bool = row.try_get("research_visible")?;
    let outcome = (pending || disclosed)
        .then(|| db::enum_value(row, "outcome"))
        .transpose()?;
    let reason: Option<String> = row.try_get("outcome_reason")?;
    // Outcomes may come from historical imports. Never reflect their free text
    // as an error message or reveal evaluator prose through this metadata API.
    let reason = reason.filter(|text| {
        disclosed
            && (1..=120).contains(&text.len())
            && text
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    });
    Ok(ExperimentView {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        cycle_id: db::id(row.try_get("cycle_id")?)?,
        family_id: db::id(row.try_get("family_id")?)?,
        parent_experiment_id: db::optional_id(row, "parent_experiment_id")?,
        ordinal: u32::try_from(row.try_get::<i32, _>("ordinal")?)
            .map_err(|_| StoreError::Integrity)?,
        hypothesis: row.try_get("hypothesis")?,
        expected_failure_modes: row.try_get("expected_failure_modes")?,
        proposal_artifact_id: db::id(row.try_get("proposal_artifact_id")?)?,
        parameter_artifact_id: db::optional_id(row, "parameter_artifact_id")?,
        code_artifact_id: db::optional_id(row, "code_artifact_id")?,
        trial_source: db::enum_value(row, "trial_source")?,
        run_id: db::optional_id(row, "run_id")?,
        author_run_id: db::optional_id(row, "author_run_id")?,
        author_attempt_id: db::optional_id(row, "author_attempt_id")?,
        result_visibility: if pending {
            ExperimentResultVisibility::Pending
        } else if disclosed {
            ExperimentResultVisibility::Research
        } else {
            ExperimentResultVisibility::Restricted
        },
        outcome,
        outcome_reason: reason,
        conclusion_artifact_id: if disclosed {
            db::optional_id(row, "conclusion_artifact_id")?
        } else {
            None
        },
        revision: db::revision(row.try_get("revision")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

struct Author {
    scope: String,
    kind: &'static str,
    source: &'static str,
    credential: Option<Id>,
    run: Option<Id>,
    attempt: Option<Id>,
}

async fn authorize(
    tx: &mut Tx<'_>,
    actor: &Actor,
    project: Id,
    cycle: Id,
) -> Result<Author, StoreError> {
    match actor {
        Actor::Browser { .. } => {
            authority::browser(tx, actor, true, true).await?;
            sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
                .bind(project.as_uuid())
                .fetch_optional(&mut **tx)
                .await?
                .ok_or(StoreError::NotFound)?;
            Ok(Author {
                scope: "OPERATOR".into(),
                kind: "OPERATOR",
                source: "OPERATOR",
                credential: None,
                run: None,
                attempt: None,
            })
        }
        Actor::Machine { .. } => {
            // machine() already owns the parent project write lock, so all
            // proposal/Run writers serialize before acquiring subordinate locks.
            let machine = authority::machine(tx, actor, true).await?;
            machine.requires(MachineScope::ExperimentSubmit)?;
            machine.project(project)?;
            let (kind, source, run, attempt) = match machine.kind {
                PrincipalKind::Cli if machine.run_id.is_none() => ("CLI", "OPERATOR", None, None),
                PrincipalKind::Mission => {
                    let run = machine.run_id.ok_or(StoreError::InvalidCredentials)?;
                    let row = sqlx::query("SELECT r.active_attempt_id FROM app.runs r JOIN app.run_attempts a ON a.id=r.active_attempt_id AND a.run_id=r.id WHERE r.id=$1 AND r.project_id=$2 AND r.cycle_id=$3 AND r.kind='AGENT_RESEARCH' AND r.state IN ('DISPATCHING','RUNNING') AND r.deadline_at>clock_timestamp() AND a.lease_expires_at>clock_timestamp()")
                        .bind(run.as_uuid()).bind(project.as_uuid()).bind(cycle.as_uuid())
                        .fetch_optional(&mut **tx).await?.ok_or(StoreError::Forbidden)?;
                    let attempt = db::optional_id(&row, "active_attempt_id")?
                        .ok_or(StoreError::InvalidCredentials)?;
                    ("MISSION", "CODEX", Some(run), Some(attempt))
                }
                _ => return Err(StoreError::Forbidden),
            };
            Ok(Author {
                scope: format!("CREDENTIAL:{}", machine.credential_id),
                kind,
                source,
                credential: Some(machine.credential_id),
                run,
                attempt,
            })
        }
    }
}

async fn proposal_artifacts(
    tx: &mut Tx<'_>,
    project: Id,
    request: &ExperimentProposalV1,
) -> Result<(), StoreError> {
    let mut refs = vec![
        (
            request.proposal_artifact_id,
            ResearchArtifactKind::Report,
            "proposal_artifact_id",
        ),
        (
            request.parameter_artifact_id,
            ResearchArtifactKind::Parameters,
            "parameter_artifact_id",
        ),
    ];
    if let Some(code) = request.code_artifact_id {
        refs.push((code, ResearchArtifactKind::Code, "code_artifact_id"));
    }
    refs.sort_by_key(|(id, _, _)| *id);
    for (id, kind, field) in refs {
        let row = sqlx::query("SELECT kind,media_type,schema_name,schema_version,access_class,byte_count FROM app.artifacts WHERE id=$1 AND project_id=$2 FOR SHARE")
            .bind(id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?
            .ok_or_else(|| invalid(field, "REFERENCE_UNAVAILABLE"))?;
        if row.try_get::<String, _>("kind")? != kind.code()
            || row.try_get::<String, _>("media_type")? != kind.media_type()
            || row.try_get::<String, _>("schema_name")? != kind.schema_name()
            || row.try_get::<String, _>("schema_version")? != "1"
            || row.try_get::<String, _>("access_class")? != "RESEARCH"
            || row.try_get::<i64, _>("byte_count")? <= 0
        {
            return Err(invalid(field, "RESEARCH_ARTIFACT_CONTRACT").into());
        }
    }
    Ok(())
}

async fn read_one(tx: &mut Tx<'_>, project: Id, id: Id) -> Result<ExperimentView, StoreError> {
    let row = sqlx::query(&format!(
        "{SELECT_VIEW} WHERE e.id=$1 AND e.project_id=$2 FOR SHARE OF e"
    ))
    .bind(id.as_uuid())
    .bind(project.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::NotFound)?;
    view(&row)
}

impl Store {
    pub async fn propose_experiment(
        &self,
        actor: &Actor,
        key: &str,
        request: &ExperimentProposalV1,
    ) -> Result<CommandResult<ExperimentView>, StoreError> {
        commands::key(key)?;
        domain::experiments::proposal(request)?;
        let mut tx = self.pool.begin().await?;
        // This immutable lookup only selects the authority scope. It does not
        // expose the cycle or authorize a mutation before authenticate/lock.
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.research_cycles WHERE id=$1")
                .bind(request.cycle_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let project = db::id(project)?;
        let author = authorize(&mut tx, actor, project, request.cycle_id).await?;
        let prepared =
            commands::experiment_propose(&mut tx, author.scope.clone(), key, db::json(request)?)
                .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let cycle = sqlx::query("SELECT state,brief_id,budget_snapshot FROM app.research_cycles WHERE id=$1 AND project_id=$2 FOR UPDATE")
            .bind(request.cycle_id.as_uuid()).bind(project.as_uuid()).fetch_one(&mut *tx).await?;
        let active: bool =
            sqlx::query_scalar("SELECT state='ACTIVE' FROM app.projects WHERE id=$1")
                .bind(project.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !active || cycle.try_get::<String, _>("state")? != "RUNNING" {
            return Err(DomainError::AdmissionClosed.into());
        }
        let brief = sqlx::query("SELECT b.state,b.budget,b.stop_rule,p.family_id FROM app.research_briefs b JOIN app.evaluation_policies p ON p.id=b.evaluation_policy_id AND p.project_id=b.project_id WHERE b.id=$1 AND b.project_id=$2 FOR SHARE OF b")
            .bind(cycle.try_get::<uuid::Uuid, _>("brief_id")?).bind(project.as_uuid()).fetch_one(&mut *tx).await?;
        if brief.try_get::<String, _>("state")? != "FROZEN" {
            return Err(DomainError::AdmissionClosed.into());
        }
        if brief.try_get::<uuid::Uuid, _>("family_id")? != request.family_id.as_uuid() {
            return Err(invalid("family_id", "FROZEN_FAMILY_MISMATCH").into());
        }
        let budget_json: serde_json::Value = brief.try_get("budget")?;
        if budget_json != cycle.try_get::<serde_json::Value, _>("budget_snapshot")? {
            return Err(StoreError::Integrity);
        }
        let budget: BudgetV1 =
            serde_json::from_value(budget_json).map_err(|_| StoreError::Integrity)?;
        let stop: StopRuleV1 = serde_json::from_value(brief.try_get("stop_rule")?)
            .map_err(|_| StoreError::Integrity)?;
        domain::admission::validate_budget(&budget, &stop)?;
        if let Some(parent) = request.parent_experiment_id {
            let same: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.experiments WHERE id=$1 AND project_id=$2 AND family_id=$3)")
                .bind(parent.as_uuid()).bind(project.as_uuid()).bind(request.family_id.as_uuid()).fetch_one(&mut *tx).await?;
            if !same {
                return Err(invalid("parent_experiment_id", "FAMILY_REFERENCE_UNAVAILABLE").into());
            }
        }
        proposal_artifacts(&mut tx, project, request).await?;
        let (used, ordinal): (i64, i32) = sqlx::query_as("SELECT count(*),COALESCE(max(ordinal),0)::integer FROM app.experiments WHERE cycle_id=$1")
            .bind(request.cycle_id.as_uuid()).fetch_one(&mut *tx).await?;
        if used >= i64::from(budget.max_experiments) {
            return Err(DomainError::BudgetExhausted("experiments").into());
        }
        let ordinal = ordinal
            .checked_add(1)
            .ok_or(DomainError::BudgetExhausted("experiments"))?;
        let id = prepared.target;
        sqlx::query("INSERT INTO app.experiments(id,project_id,cycle_id,family_id,parent_experiment_id,ordinal,hypothesis,expected_failure_modes,proposal_artifact_id,code_artifact_id,parameter_artifact_id,trial_source,outcome) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'PENDING')")
            .bind(id.as_uuid()).bind(project.as_uuid()).bind(request.cycle_id.as_uuid()).bind(request.family_id.as_uuid())
            .bind(request.parent_experiment_id.map(Id::as_uuid)).bind(ordinal).bind(&request.hypothesis).bind(&request.expected_failure_modes)
            .bind(request.proposal_artifact_id.as_uuid()).bind(request.code_artifact_id.map(Id::as_uuid)).bind(request.parameter_artifact_id.as_uuid())
            .bind(author.source).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO app.experiment_authorship(experiment_id,project_id,actor_kind,credential_id,author_run_id,author_attempt_id) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(id.as_uuid()).bind(project.as_uuid()).bind(author.kind).bind(author.credential.map(Id::as_uuid))
            .bind(author.run.map(Id::as_uuid)).bind(author.attempt.map(Id::as_uuid)).execute(&mut *tx).await?;
        let current = authorize(&mut tx, actor, project, request.cycle_id).await?;
        if current.scope != author.scope
            || current.run != author.run
            || current.attempt != author.attempt
        {
            return Err(StoreError::InvalidCredentials);
        }
        let resource = read_one(&mut tx, project, id).await?;
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn experiments(
        &self,
        actor: &Actor,
        query: &ResearchListQuery,
    ) -> Result<Page<ExperimentView>, StoreError> {
        if !(1..=100).contains(&query.limit) {
            return Err(StoreError::Invalid("limit"));
        }
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, query.project_id, MachineScope::ResearchRead)
            .await?;
        let exists = sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR SHARE")
            .bind(query.project_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            return Err(StoreError::NotFound);
        }
        let rows = sqlx::query(&format!("{SELECT_VIEW} WHERE e.project_id=$1 AND ($2::uuid IS NULL OR e.id<$2) ORDER BY e.id DESC LIMIT $3 FOR SHARE OF e"))
            .bind(query.project_id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit) + 1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(view).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |e| e.id,
        );
        tx.commit().await?;
        Ok(result)
    }

    pub async fn experiment(&self, actor: &Actor, id: Id) -> Result<ExperimentView, StoreError> {
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.experiments WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let project = db::id(project)?;
        authority::read_project(&mut tx, actor, project, MachineScope::ResearchRead).await?;
        let result = read_one(&mut tx, project, id).await?;
        tx.commit().await?;
        Ok(result)
    }
}
