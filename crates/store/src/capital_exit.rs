//! Durable intent/reservation and immutable owner evidence. No order or cash ledger.
use crate::{
    Store, StoreError,
    authority::{self, Actor},
    commands, db,
    lifecycle::native::NativeObjectPublication,
};
use chrono::{DateTime, Utc};
use contracts::{
    DbCounter, DecimalValue, Id, Revision, SchemaV1,
    account_observation::*,
    capital_exit::*,
    control::{CommandResult, ListQuery, MachineScope, Page, PrincipalKind},
    forward::ForwardEnvironmentV1,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;

/// Only constructed by the trusted configured owner adapter, never HTTP JSON.
/// A configured physical account identity must survive project/session aliases.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapitalExitOwnerRegistration {
    pub managed_account_key: String,
    pub owner_binding_ref: String,
    pub downstream_id: Id,
    pub environment: ForwardEnvironmentV1,
    pub venue: String,
    pub native_account_id: String,
    pub native_trader_id: String,
    pub native_client_id: String,
    pub collateral_currency: String,
    pub instrument_id: String,
    pub controlled_strategy_ids: Vec<String>,
}

// app.document requires an explicit schema-one object. Keep configuration and
// native amounts in typed envelopes instead of weakening that shared domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnerRegistrationDocumentV1 {
    schema_version: SchemaV1,
    registration: CapitalExitOwnerRegistration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paper_source: Option<PaperSourceDocumentV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PaperSourceDocumentV1 {
    schema_version: SchemaV1,
    source_id: Id,
    binding: NativeAccountBindingV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedManagedCapitalDocumentV1 {
    schema_version: SchemaV1,
    money: AccountMoneyV1,
    source_observation_id: Id,
}
#[derive(Serialize)]
struct ProjectCommandDocumentV1<'a, T> {
    schema_version: SchemaV1,
    project_id: Id,
    request: &'a T,
}
#[derive(Serialize)]
struct IntentCommandDocumentV1<'a, T> {
    schema_version: SchemaV1,
    id: Id,
    request: &'a T,
}
fn owner_document(registration: &CapitalExitOwnerRegistration) -> OwnerRegistrationDocumentV1 {
    OwnerRegistrationDocumentV1 {
        schema_version: SchemaV1,
        registration: registration.clone(),
        paper_source: None,
    }
}
fn project_command<T: Serialize>(
    project_id: Id,
    request: &T,
) -> Result<serde_json::Value, StoreError> {
    db::json(&ProjectCommandDocumentV1 {
        schema_version: SchemaV1,
        project_id,
        request,
    })
}
fn intent_command<T: Serialize>(id: Id, request: &T) -> Result<serde_json::Value, StoreError> {
    db::json(&IntentCommandDocumentV1 {
        schema_version: SchemaV1,
        id,
        request,
    })
}

struct Reservation {
    registration: CapitalExitOwnerRegistration,
    revision: Revision,
    epoch: DbCounter,
    reserved: DecimalValue,
    active_intent: Option<Id>,
    assessment: Option<CapitalExitOwnerAssessmentV1>,
    assessment_artifact: Option<Id>,
}
fn reservation(row: &sqlx::postgres::PgRow) -> Result<Reservation, StoreError> {
    Ok(Reservation {
        registration: serde_json::from_value::<OwnerRegistrationDocumentV1>(
            row.try_get("owner_registration")?,
        )
        .map_err(|_| StoreError::Integrity)?
        .registration,
        revision: db::revision(row.try_get("revision")?)?,
        epoch: DbCounter::new(
            u64::try_from(row.try_get::<i64, _>("account_control_epoch")?)
                .map_err(|_| StoreError::Integrity)?,
        )
        .map_err(|_| StoreError::Integrity)?,
        reserved: row
            .try_get::<String, _>("reserved_amount")?
            .parse()
            .map_err(|_| StoreError::Integrity)?,
        active_intent: db::optional_id(row, "active_intent_id")?,
        assessment: row
            .try_get::<Option<serde_json::Value>, _>("latest_assessment")?
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| StoreError::Integrity)?,
        assessment_artifact: db::optional_id(row, "latest_assessment_artifact_id")?,
    })
}
async fn now(tx: &mut Tx<'_>) -> Result<DateTime<Utc>, StoreError> {
    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?)
}
async fn project(tx: &mut Tx<'_>, project: Id) -> Result<(), StoreError> {
    sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR SHARE")
        .bind(project.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    Ok(())
}
async fn source(
    tx: &mut Tx<'_>,
    project: Id,
    id: Id,
) -> Result<(Id, NativeAccountBindingV1, Option<String>, Id), StoreError> {
    let row = sqlx::query("SELECT s.downstream_id,s.binding,s.native_client_id,COALESCE(c.latest_snapshot_id,c.last_observation_id) AS latest_snapshot_id FROM app.native_account_sources s JOIN app.native_account_cursors c ON c.source_id=s.id WHERE s.id=$1 AND s.project_id=$2 FOR SHARE OF c")
        .bind(id.as_uuid()).bind(project.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    Ok((
        db::id(row.try_get("downstream_id")?)?,
        serde_json::from_value(row.try_get("binding")?).map_err(|_| StoreError::Integrity)?,
        row.try_get("native_client_id")?,
        db::optional_id(&row, "latest_snapshot_id")?.ok_or(StoreError::Invalid(
            "capital_exit_account_observation_unavailable",
        ))?,
    ))
}
async fn resolve_reservation(
    tx: &mut Tx<'_>,
    source_id: Id,
    binding: &NativeAccountBindingV1,
    downstream: Id,
    client: Option<&str>,
) -> Result<Option<Reservation>, StoreError> {
    let rows = sqlx::query("SELECT r.*,r.reserved_amount::text AS reserved_amount FROM app.managed_capital_reservations r JOIN app.downstream_integrations d ON d.id=r.downstream_id WHERE r.downstream_id=$1 AND r.environment=$2 AND r.native_account_id=$3 AND r.native_trader_id=$4 AND r.native_client_id=$5 AND (r.environment='LIVE' OR r.paper_account_source_id=$6) AND d.enabled AND (d.environments='BOTH' OR d.environments=r.environment) FOR UPDATE OF r")
        .bind(downstream.as_uuid()).bind(db::code(&binding.environment)?).bind(&binding.native_account_id).bind(&binding.native_trader_id).bind(client).bind(source_id.as_uuid()).fetch_all(&mut **tx).await?;
    if rows.len() > 1 {
        return Err(StoreError::Invalid("account_owner_binding_ambiguous"));
    }
    rows.first().map(reservation).transpose()
}
async fn read_intent(tx: &mut Tx<'_>, id: Id, lock: bool) -> Result<CapitalExitViewV1, StoreError> {
    let query = if lock {
        "SELECT content FROM app.capital_exit_intents WHERE id=$1 FOR UPDATE"
    } else {
        "SELECT content FROM app.capital_exit_intents WHERE id=$1"
    };
    let value: serde_json::Value = sqlx::query_scalar(query)
        .bind(id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    let view: CapitalExitViewV1 =
        serde_json::from_value(value).map_err(|_| StoreError::Integrity)?;
    if view.owner_command.command_id != view.command_id
        || view.owner_command.account_control_epoch != view.account_control_epoch
    {
        return Err(StoreError::Integrity);
    }
    Ok(view)
}
async fn save_intent(tx: &mut Tx<'_>, value: &CapitalExitViewV1) -> Result<(), StoreError> {
    sqlx::query("UPDATE app.capital_exit_intents SET revision=$2,state=$3,command_id=$4,content=$5,updated_at=$6 WHERE id=$1")
        .bind(value.id.as_uuid()).bind(value.revision.get() as i64).bind(db::code(&value.state)?).bind(value.command_id.as_uuid()).bind(db::json(value)?).bind(value.updated_at).execute(&mut **tx).await?;
    Ok(())
}
async fn bound_owner(
    tx: &mut Tx<'_>,
    actor: &Actor,
    view: &CapitalExitViewV1,
) -> Result<Id, StoreError> {
    let downstream =
        crate::forward::source_authority(tx, actor, view.project_id, view.environment).await?;
    let bound: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.managed_capital_reservations WHERE managed_account_key=$1 AND downstream_id=$2 AND owner_binding_ref=$3)")
        .bind(&view.managed_account_key).bind(downstream.as_uuid()).bind(&view.owner_binding_ref).fetch_one(&mut **tx).await?;
    if !bound {
        return Err(StoreError::Forbidden);
    }
    Ok(downstream)
}
async fn publish<P, F, T>(
    tx: &mut Tx<'_>,
    id: Id,
    project: Id,
    environment: ForwardEnvironmentV1,
    schema: &str,
    content: &T,
    publisher: P,
) -> Result<Id, StoreError>
where
    P: FnOnce(NativeObjectPublication) -> F,
    F: std::future::Future<Output = Result<(), StoreError>>,
    T: Serialize,
{
    let bytes = serde_json::to_vec(content).map_err(|_| StoreError::Integrity)?;
    let length = i64::try_from(bytes.len()).map_err(|_| StoreError::Integrity)?;
    publisher(NativeObjectPublication { id, bytes }).await?;
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json',$3,'1','LOCAL',$4,'1',$5,'RESEARCH',$6,'IMPORT','REFERENCED')")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(schema).bind(id.to_string()).bind(length).bind(if environment == ForwardEnvironmentV1::Paper { "SYNTHETIC" } else { "REAL" }).execute(&mut **tx).await?;
    Ok(id)
}
async fn preview_receipt(tx: &mut Tx<'_>, id: Id) -> Result<CapitalExitPreviewV1, StoreError> {
    let value: serde_json::Value = sqlx::query_scalar("SELECT response_nonsecret_body FROM app.command_receipts WHERE operation='CAPITAL_EXIT_PREVIEW' AND resource_id=$1")
        .bind(id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    let value: CommandResult<CapitalExitPreviewV1> =
        serde_json::from_value(value).map_err(|_| StoreError::Integrity)?;
    Ok(value.resource)
}
async fn enqueue(tx: &mut Tx<'_>, view: &CapitalExitViewV1) -> Result<(), StoreError> {
    // Native owner consumes the durable scoped intent; PGMQ is wake-up only.
    sqlx::query("SELECT pgmq.send('capital_exits',$1::jsonb)").bind(serde_json::json!({"intent_id":view.id,"command_id":view.command_id,"account_control_epoch":view.account_control_epoch})).execute(&mut **tx).await?;
    Ok(())
}

impl Store {
    /// Configured trusted adapter only. No route accepts a user-selected key or
    /// grants itself an owner. Re-registration must match the original binding.
    pub async fn register_capital_exit_owner(
        &self,
        binding: &CapitalExitOwnerRegistration,
    ) -> Result<(), StoreError> {
        if binding.environment == ForwardEnvironmentV1::Paper {
            return Err(StoreError::Invalid(
                "paper_capital_exit_source_binding_required",
            ));
        }
        for value in [
            &binding.managed_account_key,
            &binding.owner_binding_ref,
            &binding.venue,
            &binding.native_account_id,
            &binding.native_trader_id,
            &binding.native_client_id,
            &binding.instrument_id,
        ] {
            domain::control::text(value, 1, 200, false)?;
        }
        if binding.controlled_strategy_ids.is_empty()
            || binding
                .controlled_strategy_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != binding.controlled_strategy_ids.len()
            || !contracts::research_currency::supported(&binding.collateral_currency)
        {
            return Err(StoreError::Invalid("capital_exit_owner_registration"));
        }
        let mut tx = self.pool.begin().await?;
        let existing: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.native_account_sources s JOIN app.downstream_integrations d ON d.id=s.downstream_id WHERE s.downstream_id=$1 AND s.environment=$2 AND s.native_account_id=$3 AND s.native_trader_id=$4 AND s.native_client_id=$5 AND d.enabled)")
            .bind(binding.downstream_id.as_uuid()).bind(db::code(&binding.environment)?).bind(&binding.native_account_id).bind(&binding.native_trader_id).bind(&binding.native_client_id).fetch_one(&mut *tx).await?;
        if !existing {
            return Err(StoreError::Invalid("account_owner_binding_unavailable"));
        }
        sqlx::query("INSERT INTO app.managed_capital_reservations(managed_account_key,owner_binding_ref,downstream_id,environment,venue,native_account_id,native_trader_id,native_client_id,collateral_currency,owner_registration) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT DO NOTHING")
            .bind(&binding.managed_account_key).bind(&binding.owner_binding_ref).bind(binding.downstream_id.as_uuid()).bind(db::code(&binding.environment)?).bind(&binding.venue).bind(&binding.native_account_id).bind(&binding.native_trader_id).bind(&binding.native_client_id).bind(&binding.collateral_currency).bind(db::json(&owner_document(binding))?).execute(&mut *tx).await?;
        let row = sqlx::query("SELECT owner_registration FROM app.managed_capital_reservations WHERE managed_account_key=$1 FOR UPDATE").bind(&binding.managed_account_key).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("account_owner_binding_conflict"))?;
        if row.try_get::<serde_json::Value, _>("owner_registration")?
            != db::json(&owner_document(binding))?
        {
            return Err(StoreError::NativeIdentityConflict);
        }
        tx.commit().await?;
        Ok(())
    }

    /// Called only by a deployment-approved native Paper intake hook after the
    /// original observation is durably authenticated. It grants no credentials.
    pub async fn register_paper_capital_exit_owner(
        &self,
        actor: &Actor,
        source_id: Id,
        original_binding: &NativeAccountBindingV1,
        configured: &CapitalExitOwnerRegistration,
    ) -> Result<CapitalExitOwnerRegistration, StoreError> {
        if original_binding.environment != ForwardEnvironmentV1::Paper
            || configured.environment != ForwardEnvironmentV1::Paper
        {
            return Err(StoreError::Invalid(
                "paper_capital_exit_source_binding_required",
            ));
        }
        domain::account_observation::binding(original_binding)?;
        for label in [
            &configured.venue,
            &configured.native_client_id,
            &configured.instrument_id,
        ] {
            domain::control::text(label, 1, 200, false)?;
        }
        if configured.controlled_strategy_ids.is_empty()
            || configured
                .controlled_strategy_ids
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != configured.controlled_strategy_ids.len()
            || !contracts::research_currency::supported(&configured.collateral_currency)
        {
            return Err(StoreError::Invalid("capital_exit_owner_registration"));
        }
        for label in &configured.controlled_strategy_ids {
            domain::control::text(label, 1, 200, false)?;
        }
        let mut tx = self.pool.begin().await?;
        let downstream = crate::forward::source_authority(
            &mut tx,
            actor,
            original_binding.project_id,
            ForwardEnvironmentV1::Paper,
        )
        .await?;
        let source = sqlx::query("SELECT binding,downstream_id,native_client_id FROM app.native_account_sources WHERE id=$1 AND project_id=$2").bind(source_id.as_uuid()).bind(original_binding.project_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        if source.try_get::<serde_json::Value, _>("binding")? != db::json(original_binding)?
            || db::id(source.try_get("downstream_id")?)? != downstream
            || configured.downstream_id != downstream
            || source
                .try_get::<Option<String>, _>("native_client_id")?
                .as_deref()
                != Some(configured.native_client_id.as_str())
            || configured.native_account_id != original_binding.native_account_id
            || configured.native_trader_id != original_binding.native_trader_id
        {
            return Err(StoreError::Invalid("paper_capital_exit_source_mismatch"));
        }
        let legacy: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.capital_exit_intents i JOIN app.managed_capital_reservations r ON r.managed_account_key=i.managed_account_key JOIN app.native_account_sources s ON s.id=i.account_source_id WHERE r.environment='PAPER' AND r.paper_account_source_id IS NULL AND s.environment='PAPER' AND s.native_session_id=$1 AND s.native_account_id=$2)").bind(&original_binding.native_session_id).bind(&original_binding.native_account_id).fetch_one(&mut *tx).await?;
        if legacy {
            return Err(StoreError::Invalid(
                "paper_capital_exit_legacy_binding_unresolved",
            ));
        }
        let mut binding = configured.clone();
        binding.managed_account_key = format!("paper-native:{source_id}");
        binding.owner_binding_ref = format!("nautilus-paper:{source_id}");
        let document = OwnerRegistrationDocumentV1 {
            schema_version: SchemaV1,
            registration: binding.clone(),
            paper_source: Some(PaperSourceDocumentV1 {
                schema_version: SchemaV1,
                source_id,
                binding: original_binding.clone(),
            }),
        };
        sqlx::query("INSERT INTO app.managed_capital_reservations(managed_account_key,owner_binding_ref,downstream_id,environment,venue,native_account_id,native_trader_id,native_client_id,collateral_currency,owner_registration,paper_account_source_id,paper_native_session_id) VALUES($1,$2,$3,'PAPER',$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT DO NOTHING")
            .bind(&binding.managed_account_key).bind(&binding.owner_binding_ref).bind(downstream.as_uuid()).bind(&binding.venue).bind(&binding.native_account_id).bind(&binding.native_trader_id).bind(&binding.native_client_id).bind(&binding.collateral_currency).bind(db::json(&document)?).bind(source_id.as_uuid()).bind(&original_binding.native_session_id).execute(&mut *tx).await?;
        let stored = sqlx::query("SELECT owner_registration FROM app.managed_capital_reservations WHERE paper_account_source_id=$1 FOR UPDATE").bind(source_id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("paper_capital_exit_engine_alias_conflict"))?;
        if stored.try_get::<serde_json::Value, _>("owner_registration")? != db::json(&document)? {
            return Err(StoreError::NativeIdentityConflict);
        }
        crate::forward::source_authority(
            &mut tx,
            actor,
            original_binding.project_id,
            ForwardEnvironmentV1::Paper,
        )
        .await?;
        tx.commit().await?;
        Ok(binding)
    }

    pub async fn submit_capital_exit_assessment<P, F>(
        &self,
        actor: &Actor,
        idempotency_key: &str,
        request: &CapitalExitOwnerAssessmentV1,
        publisher: P,
    ) -> Result<CommandResult<CapitalExitAssessmentReceiptV1>, StoreError>
    where
        P: FnOnce(NativeObjectPublication) -> F,
        F: std::future::Future<Output = Result<(), StoreError>>,
    {
        if idempotency_key != request.external_message_id
            || request.account_source_id != request.request.account_source_id
        {
            return Err(StoreError::Invalid("capital_exit_assessment_identity"));
        }
        let mut tx = self.pool.begin().await?;
        let project_id: uuid::Uuid =
            sqlx::query_scalar("SELECT project_id FROM app.native_account_sources WHERE id=$1")
                .bind(request.account_source_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let project_id = db::id(project_id)?;
        let raw_binding: serde_json::Value =
            sqlx::query_scalar("SELECT binding FROM app.native_account_sources WHERE id=$1")
                .bind(request.account_source_id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let immutable_binding: NativeAccountBindingV1 =
            serde_json::from_value(raw_binding).map_err(|_| StoreError::Integrity)?;
        let downstream = crate::forward::source_authority(
            &mut tx,
            actor,
            project_id,
            immutable_binding.environment,
        )
        .await?;
        let (source_downstream, binding, client, current) =
            source(&mut tx, project_id, request.account_source_id).await?;
        if source_downstream != downstream {
            return Err(StoreError::Forbidden);
        }
        let prepared = commands::capital_exit(
            &mut tx,
            format!("DOWNSTREAM:{downstream}:CAPITAL_EXIT"),
            "CAPITAL_EXIT_ASSESSMENT",
            idempotency_key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let reserve = resolve_reservation(
            &mut tx,
            request.account_source_id,
            &binding,
            downstream,
            client.as_deref(),
        )
        .await?
        .ok_or(StoreError::Invalid("account_owner_binding_unavailable"))?;
        if reserve.registration.owner_binding_ref != request.owner_binding_ref
            || reserve.revision != request.expected_account_control_revision
        {
            return Err(StoreError::RevisionConflict {
                current: reserve.revision,
            });
        }
        if current != request.request.expected_source_observation_id {
            return Err(StoreError::Invalid("capital_exit_preview_stale"));
        }
        let checked = now(&mut tx).await?;
        domain::capital_exit::preview_request(&request.request, checked)?;
        if request.asof > checked
            || request.valid_until <= checked
            || request.asof >= request.valid_until
            || request.sequence.get() == 0
            || request.native_report_ref.trim().is_empty()
            || request
                .native_evidence
                .as_object()
                .is_none_or(|v| v.is_empty())
            || request.funds.requested.currency != reserve.registration.collateral_currency
            || request
                .reduction_legs
                .iter()
                .any(|leg| leg.instrument_id != reserve.registration.instrument_id)
        {
            return Err(StoreError::Invalid("capital_exit_assessment_evidence"));
        }
        if let Some(active) = reserve.active_intent {
            let active = read_intent(&mut tx, active, false).await?;
            if active.account_source_id != request.account_source_id {
                return Err(StoreError::Invalid("capital_exit_owner_recovery_required"));
            }
        }
        verify_observation(&mut tx, request.account_source_id, current, checked).await?;
        let prior_sequence: Option<i64> = sqlx::query_scalar("SELECT max(sequence) FROM app.capital_exit_evidence WHERE owner_binding_ref=$1 AND account_source_id=$2 AND record_kind='ASSESSMENT'").bind(&request.owner_binding_ref).bind(request.account_source_id.as_uuid()).fetch_one(&mut *tx).await?;
        if prior_sequence.is_some_and(|v| request.sequence.get() <= v as u64) {
            return Err(StoreError::Invalid("capital_exit_assessment_sequence"));
        }
        for reference in request
            .evidence_refs
            .iter()
            .copied()
            .chain(request.remaining_risk_evidence_id)
        {
            let retained: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 AND project_id=$2) OR EXISTS(SELECT 1 FROM app.native_account_observations WHERE id=$1 AND source_id=$3)")
                .bind(reference.as_uuid()).bind(project_id.as_uuid()).bind(request.account_source_id.as_uuid()).fetch_one(&mut *tx).await?;
            if !retained {
                return Err(StoreError::Invalid(
                    "capital_exit_original_evidence_unavailable",
                ));
            }
        }
        let artifact = Id::new();
        publish(
            &mut tx,
            artifact,
            project_id,
            binding.environment,
            "qz.capital_exit_owner_assessment",
            request,
            publisher,
        )
        .await?;
        let received = now(&mut tx).await?;
        if request.valid_until <= received {
            return Err(StoreError::Invalid("capital_exit_assessment_expired"));
        }
        sqlx::query("INSERT INTO app.capital_exit_evidence(id,intent_id,managed_account_key,owner_binding_ref,account_source_id,record_kind,external_message_id,sequence,artifact_id,content,received_at) VALUES($1,NULL,$2,$3,$4,'ASSESSMENT',$5,$6,$7,$8,$9)")
            .bind(prepared.target.as_uuid()).bind(&reserve.registration.managed_account_key).bind(&request.owner_binding_ref).bind(request.account_source_id.as_uuid()).bind(&request.external_message_id).bind(request.sequence.get() as i64).bind(artifact.as_uuid()).bind(db::json(request)?).bind(received).execute(&mut *tx).await?;
        sqlx::query("UPDATE app.managed_capital_reservations SET latest_assessment=$2,latest_assessment_artifact_id=$3 WHERE managed_account_key=$1")
            .bind(&reserve.registration.managed_account_key).bind(db::json(request)?).bind(artifact.as_uuid()).execute(&mut *tx).await?;
        crate::forward::source_authority(&mut tx, actor, project_id, binding.environment).await?;
        let resource = CapitalExitAssessmentReceiptV1 {
            id: prepared.target,
            artifact_id: artifact,
            account_source_id: request.account_source_id,
            external_message_id: request.external_message_id.clone(),
            received_at: received,
        };
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn preview_capital_exit<P, F>(
        &self,
        actor: &Actor,
        project_id: Id,
        idempotency_key: &str,
        request: &CapitalExitPreviewRequestV1,
        publisher: P,
    ) -> Result<CommandResult<CapitalExitPreviewV1>, StoreError>
    where
        P: FnOnce(NativeObjectPublication) -> F,
        F: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        authority::browser(&mut tx, actor, true).await?;
        project(&mut tx, project_id).await?;
        let prepared = commands::capital_exit(
            &mut tx,
            "OPERATOR".into(),
            "CAPITAL_EXIT_PREVIEW",
            idempotency_key,
            None,
            project_command(project_id, request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let checked = now(&mut tx).await?;
        domain::capital_exit::preview_request(request, checked)?;
        let (downstream, binding, client, current) =
            source(&mut tx, project_id, request.account_source_id).await?;
        let reserve = resolve_reservation(
            &mut tx,
            request.account_source_id,
            &binding,
            downstream,
            client.as_deref(),
        )
        .await?;
        let original_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.native_account_observations WHERE id=$1 AND source_id=$2)").bind(request.expected_source_observation_id.as_uuid()).bind(request.account_source_id.as_uuid()).fetch_one(&mut *tx).await?;
        if !original_exists {
            return Err(StoreError::Invalid(
                "capital_exit_original_observation_mismatch",
            ));
        }
        let requested = request.scope.money();
        let artifact = Id::new();
        let mut view = CapitalExitPreviewV1 {
            schema_version: SchemaV1,
            id: prepared.target,
            project_id,
            account_source_id: request.account_source_id,
            original_observation_id: request.expected_source_observation_id,
            environment: binding.environment,
            managed_account_key: reserve
                .as_ref()
                .map(|r| r.registration.managed_account_key.clone()),
            owner_binding_ref: reserve
                .as_ref()
                .map(|r| r.registration.owner_binding_ref.clone()),
            expected_account_control_revision: reserve.as_ref().map(|r| r.revision),
            scope: request.scope.clone(),
            policy: request.policy.clone(),
            plan_artifact_id: artifact,
            evidence_refs: vec![request.expected_source_observation_id],
            valid_until: checked,
            funds: CapitalExitPreviewFundsV1 {
                requested,
                native_total_cash: None,
                native_free_cash: None,
                native_locked_cash: None,
                verified_idle_cash: None,
                estimated_release: None,
                native_equity: None,
                managed_capital_before: None,
                remaining_managed_capital: None,
                estimated_execution_cost: None,
                existing_unrealized_pnl: None,
            },
            proposed_cancellations: vec![],
            retained_protective_orders: vec![],
            reduction_legs: vec![],
            remaining_risk_evidence_id: None,
            capability: CapitalExitCapabilityV1::Blocked,
            reason_codes: vec![],
        };
        if current != request.expected_source_observation_id {
            view.reason_codes.push("capital_exit_preview_stale".into());
        }
        if let Some(reserve) = &reserve {
            match (&reserve.assessment, reserve.assessment_artifact) {
                (Some(assessment), Some(evidence))
                    if assessment.request == *request
                        && assessment.expected_account_control_revision == reserve.revision
                        && assessment.valid_until > checked =>
                {
                    view.funds = assessment.funds.clone();
                    view.proposed_cancellations = assessment.proposed_cancellations.clone();
                    view.retained_protective_orders = assessment.retained_protective_orders.clone();
                    view.reduction_legs = assessment.reduction_legs.clone();
                    view.remaining_risk_evidence_id = assessment.remaining_risk_evidence_id;
                    view.evidence_refs.extend(assessment.evidence_refs.clone());
                    view.evidence_refs.push(evidence);
                    view.valid_until = assessment.valid_until;
                    view.capability = assessment.capability;
                    view.reason_codes.extend(assessment.reason_codes.clone());
                }
                _ => view
                    .reason_codes
                    .push("capital_exit_owner_assessment_unavailable".into()),
            }
        } else {
            view.reason_codes
                .push("account_owner_binding_unavailable".into());
        }
        if matches!(request.scope, CapitalExitScopeV1::PortfolioScope { .. }) {
            view.capability = CapitalExitCapabilityV1::Unsupported;
            view.reason_codes
                .push("capital_exit_portfolio_scope_unavailable".into());
        }
        if let Err(error) =
            verify_observation(&mut tx, request.account_source_id, current, checked).await
        {
            match error {
                StoreError::Invalid(code) => view.reason_codes.push(code.into()),
                error => return Err(error),
            }
        }
        if !view.reason_codes.is_empty() && view.capability != CapitalExitCapabilityV1::Unsupported
        {
            view.capability = CapitalExitCapabilityV1::Blocked;
        }
        if view.capability == CapitalExitCapabilityV1::Supported {
            domain::capital_exit::admit_preview(&view, checked)?;
        }
        publish(
            &mut tx,
            artifact,
            project_id,
            binding.environment,
            "qz.capital_exit_preview",
            &view,
            publisher,
        )
        .await?;
        authority::browser(&mut tx, actor, true).await?;
        let result = commands::finish(&mut tx, prepared, view, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn start_capital_exit(
        &self,
        actor: &Actor,
        project_id: Id,
        idempotency_key: &str,
        request: &CapitalExitStartV1,
    ) -> Result<CommandResult<CapitalExitViewV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        authority::browser(&mut tx, actor, true).await?;
        project(&mut tx, project_id).await?;
        let prepared = commands::capital_exit(
            &mut tx,
            "OPERATOR".into(),
            "CAPITAL_EXIT_START",
            idempotency_key,
            None,
            project_command(project_id, request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let preview = preview_receipt(&mut tx, request.preview_id).await?;
        let checked = now(&mut tx).await?;
        domain::capital_exit::admit_preview(&preview, checked)?;
        if preview.project_id != project_id
            || preview.plan_artifact_id != request.acknowledged_plan_artifact_id
            || preview.original_observation_id != request.expected_source_observation_id
            || preview.expected_account_control_revision
                != Some(request.expected_account_control_revision)
        {
            return Err(StoreError::Invalid("capital_exit_plan_acknowledgement"));
        }
        let (downstream, binding, client, current) =
            source(&mut tx, project_id, preview.account_source_id).await?;
        if current != preview.original_observation_id {
            return Err(StoreError::Invalid("capital_exit_preview_stale"));
        }
        verify_observation(&mut tx, preview.account_source_id, current, checked).await?;
        let reserve = resolve_reservation(
            &mut tx,
            preview.account_source_id,
            &binding,
            downstream,
            client.as_deref(),
        )
        .await?
        .ok_or(StoreError::Invalid("account_owner_binding_unavailable"))?;
        if reserve.revision != request.expected_account_control_revision {
            return Err(StoreError::RevisionConflict {
                current: reserve.revision,
            });
        }
        if reserve.active_intent.is_some() || reserve.reserved.is_positive() {
            return Err(StoreError::Invalid("capital_exit_account_already_reserved"));
        }
        if preview.managed_account_key.as_ref() != Some(&reserve.registration.managed_account_key)
            || preview.owner_binding_ref.as_ref() != Some(&reserve.registration.owner_binding_ref)
            || reserve.assessment.as_ref().is_none_or(|a| {
                a.request.scope != preview.scope
                    || a.request.policy != preview.policy
                    || a.request.expected_source_observation_id != current
                    || a.valid_until <= checked
                    || a.capability != CapitalExitCapabilityV1::Supported
                    || !a.reason_codes.is_empty()
            })
            || reserve
                .assessment_artifact
                .is_none_or(|id| !preview.evidence_refs.contains(&id))
        {
            return Err(StoreError::Invalid("capital_exit_preview_stale"));
        }
        let revision = reserve.revision.next().ok_or(StoreError::Integrity)?;
        let epoch = DbCounter::new(
            reserve
                .epoch
                .get()
                .checked_add(1)
                .ok_or(StoreError::Integrity)?,
        )
        .map_err(|_| StoreError::Integrity)?;
        let requested = preview.scope.money();
        let zero = AccountMoneyV1 {
            amount: DecimalValue::zero(),
            currency: requested.currency.clone(),
        };
        let command_id = Id::new();
        let view = CapitalExitViewV1 {
            schema_version: SchemaV1,
            id: prepared.target,
            project_id,
            account_source_id: preview.account_source_id,
            environment: preview.environment,
            managed_account_key: reserve.registration.managed_account_key.clone(),
            owner_binding_ref: reserve.registration.owner_binding_ref,
            account_control_epoch: epoch,
            account_control_revision: revision,
            revision: Revision::INITIAL,
            state: CapitalExitStateV1::Requested,
            last_phase: CapitalExitStateV1::Requested,
            reason_codes: vec!["capital_exit_owner_fence_pending".into()],
            preview_id: preview.id,
            plan_artifact_id: preview.plan_artifact_id,
            scope: preview.scope,
            policy: preview.policy,
            command_id,
            owner_command: CapitalExitOwnerCommandV1 {
                schema_version: SchemaV1,
                command_id,
                account_control_epoch: epoch,
                instruction: CapitalExitOwnerInstructionV1::Start {},
            },
            external_claim_id: None,
            remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
            funds: CapitalExitFundsV1 {
                requested_amount: requested.clone(),
                reserved_amount: requested,
                released_cash_amount: None,
                verified_withdrawable_amount: None,
                reconciled_withdrawal_amount: zero,
                unreleased_amount: None,
                evidence_asof: None,
                evidence_valid_until: None,
                withdrawability: CapitalExitWithdrawabilityV1::Unverified,
            },
            evidence_refs: preview.evidence_refs,
            created_at: checked,
            updated_at: checked,
        };
        sqlx::query("INSERT INTO app.capital_exit_intents(id,project_id,account_source_id,managed_account_key,original_plan_artifact_id,original_request,revision,state,command_id,content) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(view.id.as_uuid()).bind(project_id.as_uuid()).bind(view.account_source_id.as_uuid()).bind(&view.managed_account_key).bind(view.plan_artifact_id.as_uuid()).bind(db::json(request)?).bind(view.revision.get() as i64).bind(db::code(&view.state)?).bind(view.command_id.as_uuid()).bind(db::json(&view)?).execute(&mut *tx).await?;
        sqlx::query("UPDATE app.managed_capital_reservations SET revision=$2,account_control_epoch=$3,reserved_amount=$4::numeric,active_intent_id=$5,latest_assessment=NULL,latest_assessment_artifact_id=NULL WHERE managed_account_key=$1")
            .bind(&view.managed_account_key).bind(revision.get() as i64).bind(epoch.get() as i64).bind(view.funds.reserved_amount.amount.as_decimal().to_plain_string()).bind(view.id.as_uuid()).execute(&mut *tx).await?;
        enqueue(&mut tx, &view).await?;
        let result = finish_intent_command(&mut tx, prepared, view, 202).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn act_capital_exit(
        &self,
        actor: &Actor,
        id: Id,
        idempotency_key: &str,
        request: &CapitalExitActionV1,
    ) -> Result<CommandResult<CapitalExitViewV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        authority::browser(&mut tx, actor, true).await?;
        let prepared = commands::capital_exit(
            &mut tx,
            "OPERATOR".into(),
            "CAPITAL_EXIT_ACTION",
            idempotency_key,
            Some(id),
            intent_command(id, request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        // Acquire the account before the intent in every writer.
        let original = read_intent(&mut tx, id, false).await?;
        let row=sqlx::query("SELECT *,reserved_amount::text AS reserved_amount FROM app.managed_capital_reservations WHERE managed_account_key=$1 FOR UPDATE").bind(&original.managed_account_key).fetch_one(&mut *tx).await?;
        let reserve = reservation(&row)?;
        let mut view = read_intent(&mut tx, id, true).await?;
        require_bound_paper_source(&mut tx, &view).await?;
        if view.revision != request.expected_revision() {
            return Err(StoreError::RevisionConflict {
                current: view.revision,
            });
        }
        if reserve.active_intent != Some(id) {
            return Err(StoreError::Conflict);
        }
        let checked = now(&mut tx).await?;
        let state = domain::capital_exit::action_state(view.state, request)?;
        sqlx::query("UPDATE app.capital_exit_intents SET command_action=$2 WHERE id=$1")
            .bind(id.as_uuid())
            .bind(request.action())
            .execute(&mut *tx)
            .await?;
        if let CapitalExitActionV1::Resume { preview_id, .. } = request {
            let preview = preview_receipt(&mut tx, *preview_id).await?;
            domain::capital_exit::admit_preview(&preview, checked)?;
            let (_, _, _, current) =
                source(&mut tx, view.project_id, view.account_source_id).await?;
            if preview.project_id != view.project_id
                || preview.account_source_id != view.account_source_id
                || preview.managed_account_key.as_ref() != Some(&view.managed_account_key)
                || preview.owner_binding_ref.as_ref() != Some(&view.owner_binding_ref)
                || preview.original_observation_id != current
                || preview.expected_account_control_revision != Some(reserve.revision)
                || preview.scope.money() != view.funds.reserved_amount
                || reserve
                    .assessment_artifact
                    .is_none_or(|artifact| !preview.evidence_refs.contains(&artifact))
            {
                return Err(StoreError::Invalid("capital_exit_resume_plan_mismatch"));
            }
            verify_observation(&mut tx, view.account_source_id, current, checked).await?;
            // A new plan is only for the remaining reserve. Previously executed
            // quantities remain native facts; the owner rechecks cumulative bounds.
            view.preview_id = preview.id;
            view.plan_artifact_id = preview.plan_artifact_id;
            view.policy = preview.policy;
            view.evidence_refs.extend(preview.evidence_refs);
        }
        if let CapitalExitActionV1::ReconcileWithdrawal {
            user_reported_amount,
            currency,
            ..
        } = request
        {
            if currency != &view.funds.reserved_amount.currency
                || user_reported_amount > &view.funds.reserved_amount.amount
            {
                return Err(StoreError::Invalid("capital_exit_reported_withdrawal"));
            }
            sqlx::query("UPDATE app.capital_exit_intents SET withdrawal_report=$2 WHERE id=$1")
                .bind(id.as_uuid())
                .bind(db::json(request)?)
                .execute(&mut *tx)
                .await?;
        }
        view.last_phase = view.state;
        view.state = state;
        view.revision = view.revision.next().ok_or(StoreError::Integrity)?;
        view.account_control_revision = reserve.revision.next().ok_or(StoreError::Integrity)?;
        view.account_control_epoch = DbCounter::new(
            reserve
                .epoch
                .get()
                .checked_add(1)
                .ok_or(StoreError::Integrity)?,
        )
        .map_err(|_| StoreError::Integrity)?;
        view.command_id = Id::new();
        view.owner_command = CapitalExitOwnerCommandV1 {
            schema_version: SchemaV1,
            command_id: view.command_id,
            account_control_epoch: view.account_control_epoch,
            instruction: CapitalExitOwnerInstructionV1::from_action(request),
        };
        view.external_claim_id = None;
        view.updated_at = checked;
        view.reason_codes = vec!["capital_exit_owner_command_pending".into()];
        view.remaining_trading = CapitalExitRemainingTradingV1::FencedPendingTarget;
        view.funds.verified_withdrawable_amount = None;
        view.funds.withdrawability = CapitalExitWithdrawabilityV1::Unverified;
        sqlx::query("UPDATE app.managed_capital_reservations SET revision=$2,account_control_epoch=$3,latest_assessment=NULL,latest_assessment_artifact_id=NULL WHERE managed_account_key=$1")
            .bind(&view.managed_account_key).bind(view.account_control_revision.get() as i64).bind(view.account_control_epoch.get() as i64).execute(&mut *tx).await?;
        sqlx::query("UPDATE app.capital_exit_intents SET fence_evidence_id=NULL,availability_observation_id=NULL,claim_credential_id=NULL WHERE id=$1").bind(id.as_uuid()).execute(&mut *tx).await?;
        enqueue(&mut tx, &view).await?;
        let result = finish_intent_command(&mut tx, prepared, view, 202).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Pending read-side plans are the immutable preview receipts, not a second queue/ledger.
    pub async fn downstream_capital_exit_assessments(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<CapitalExitPreviewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let machine = authority::machine(&mut tx, actor, true).await?;
        if machine.kind != PrincipalKind::Downstream {
            return Err(StoreError::Forbidden);
        }
        machine.requires(MachineScope::ForwardSubmit)?;
        let downstream = machine.downstream_id.ok_or(StoreError::Forbidden)?;
        let project = machine.project_id.ok_or(StoreError::Forbidden)?;
        let rows = sqlx::query("SELECT c.response_nonsecret_body->'resource' AS content FROM app.command_receipts c JOIN app.native_account_sources s ON s.id::text=c.response_nonsecret_body->'resource'->>'account_source_id' JOIN app.managed_capital_reservations r ON r.downstream_id=s.downstream_id AND r.native_account_id=s.native_account_id AND r.native_trader_id=s.native_trader_id AND r.native_client_id=s.native_client_id AND r.environment=s.environment AND (r.environment='LIVE' OR r.paper_account_source_id=s.id) JOIN app.downstream_integrations d ON d.id=r.downstream_id WHERE c.operation='CAPITAL_EXIT_PREVIEW' AND s.project_id=$1 AND s.downstream_id=$2 AND d.enabled AND c.created_at>clock_timestamp()-interval '5 minutes' AND c.response_nonsecret_body->'resource'->'reason_codes' ? 'capital_exit_owner_assessment_unavailable' AND ($3::uuid IS NULL OR c.resource_id<$3) ORDER BY c.resource_id DESC LIMIT $4")
            .bind(project.as_uuid()).bind(downstream.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let mut items = Vec::new();
        for row in rows {
            let value: CapitalExitPreviewV1 = serde_json::from_value(row.try_get("content")?)
                .map_err(|_| StoreError::Integrity)?;
            crate::forward::source_authority(&mut tx, actor, project, value.environment).await?;
            items.push(value);
        }
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |v| v.id))
    }

    pub async fn capital_exit(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<CapitalExitViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        authority::browser(&mut tx, actor, false).await?;
        let mut view = read_intent(&mut tx, id, false).await?;
        project(&mut tx, view.project_id).await?;
        refresh_view(&mut tx, &mut view).await?;
        tx.commit().await?;
        Ok(view)
    }
    pub async fn capital_exits(
        &self,
        actor: &Actor,
        project_id: Id,
        query: &ListQuery,
    ) -> Result<Page<CapitalExitViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        authority::browser(&mut tx, actor, false).await?;
        project(&mut tx, project_id).await?;
        let rows=sqlx::query("SELECT content FROM app.capital_exit_intents WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3").bind(project_id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let mut views = Vec::new();
        for row in rows {
            let mut view = serde_json::from_value(row.try_get("content")?)
                .map_err(|_| StoreError::Integrity)?;
            refresh_view(&mut tx, &mut view).await?;
            views.push(view);
        }
        tx.commit().await?;
        Ok(crate::control::page(views, query.limit, |v| v.id))
    }
    pub async fn downstream_capital_exits(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<CapitalExitViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let machine = authority::machine(&mut tx, actor, true).await?;
        if machine.kind != PrincipalKind::Downstream {
            return Err(StoreError::Forbidden);
        }
        machine.requires(MachineScope::ForwardSubmit)?;
        let downstream = machine.downstream_id.ok_or(StoreError::Forbidden)?;
        let project_id = machine.project_id.ok_or(StoreError::Forbidden)?;
        let rows=sqlx::query("SELECT i.content FROM app.capital_exit_intents i JOIN app.managed_capital_reservations r ON r.active_intent_id=i.id WHERE i.project_id=$1 AND r.downstream_id=$2 AND (r.environment='LIVE' OR r.paper_account_source_id=i.account_source_id) AND i.state<>'COMPLETED' AND ($3::uuid IS NULL OR i.id<$3) ORDER BY i.id DESC LIMIT $4").bind(project_id.as_uuid()).bind(downstream.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let mut views = Vec::new();
        for row in rows {
            let mut view = serde_json::from_value(row.try_get("content")?)
                .map_err(|_| StoreError::Integrity)?;
            bound_owner(&mut tx, actor, &view).await?;
            refresh_view(&mut tx, &mut view).await?;
            views.push(view);
        }
        tx.commit().await?;
        Ok(crate::control::page(views, query.limit, |v| v.id))
    }
    pub async fn claim_capital_exit(
        &self,
        actor: &Actor,
        id: Id,
        idempotency_key: &str,
        request: &CapitalExitClaimV1,
    ) -> Result<CommandResult<CapitalExitViewV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let original = read_intent(&mut tx, id, false).await?;
        let downstream = bound_owner(&mut tx, actor, &original).await?;
        let prepared = commands::capital_exit(
            &mut tx,
            format!("DOWNSTREAM:{downstream}:CAPITAL_EXIT"),
            "CAPITAL_EXIT_CLAIM",
            idempotency_key,
            Some(id),
            intent_command(id, request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        sqlx::query("SELECT managed_account_key FROM app.managed_capital_reservations WHERE managed_account_key=$1 FOR UPDATE").bind(&original.managed_account_key).fetch_one(&mut *tx).await?;
        let mut view = read_intent(&mut tx, id, true).await?;
        require_bound_paper_source(&mut tx, &view).await?;
        domain::control::text(&request.external_claim_id, 1, 200, false)?;
        if view.revision != request.expected_revision {
            return Err(StoreError::RevisionConflict {
                current: view.revision,
            });
        }
        if request.command_id != view.command_id
            || request.account_control_epoch != view.account_control_epoch
            || request.account_source_id != view.account_source_id
            || request.owner_binding_ref != view.owner_binding_ref
            || view.external_claim_id.is_some()
            || view.state == CapitalExitStateV1::Completed
        {
            return Err(StoreError::Conflict);
        }
        let Actor::Machine { credential_id, .. } = actor else {
            return Err(StoreError::Forbidden);
        };
        sqlx::query("UPDATE app.capital_exit_intents SET claim_credential_id=$2 WHERE id=$1")
            .bind(id.as_uuid())
            .bind(credential_id.as_uuid())
            .execute(&mut *tx)
            .await?;
        view.external_claim_id = Some(request.external_claim_id.clone());
        if view.state == CapitalExitStateV1::Requested {
            view.last_phase = view.state;
            view.state = CapitalExitStateV1::Fencing;
        }
        view.revision = view.revision.next().ok_or(StoreError::Integrity)?;
        view.updated_at = now(&mut tx).await?;
        view.reason_codes = vec!["capital_exit_owner_fence_pending".into()];
        let result = finish_intent_command(&mut tx, prepared, view, 202).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn submit_capital_exit_evidence<P, F>(
        &self,
        actor: &Actor,
        id: Id,
        idempotency_key: &str,
        request: &CapitalExitOwnerEvidenceV1,
        publisher: P,
    ) -> Result<CommandResult<CapitalExitViewV1>, StoreError>
    where
        P: FnOnce(NativeObjectPublication) -> F,
        F: std::future::Future<Output = Result<(), StoreError>>,
    {
        if id != request.intent_id || idempotency_key != request.external_message_id {
            return Err(StoreError::Invalid("capital_exit_evidence_identity"));
        }
        let mut tx = self.pool.begin().await?;
        let original = read_intent(&mut tx, id, false).await?;
        let downstream = bound_owner(&mut tx, actor, &original).await?;
        let prepared = commands::capital_exit(
            &mut tx,
            format!("DOWNSTREAM:{downstream}:CAPITAL_EXIT"),
            "CAPITAL_EXIT_EVIDENCE",
            idempotency_key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let row=sqlx::query("SELECT *,reserved_amount::text AS reserved_amount FROM app.managed_capital_reservations WHERE managed_account_key=$1 FOR UPDATE").bind(&original.managed_account_key).fetch_one(&mut *tx).await?;
        let reserve = reservation(&row)?;
        let mut view = read_intent(&mut tx, id, true).await?;
        let checked = now(&mut tx).await?;
        if reserve.active_intent != Some(id) || reserve.epoch != view.account_control_epoch {
            return Err(StoreError::Conflict);
        }
        require_bound_paper_source(&mut tx, &view).await?;
        domain::capital_exit::evidence(request, checked)?;
        let Actor::Machine { credential_id, .. } = actor else {
            return Err(StoreError::Forbidden);
        };
        let claim_credential: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT claim_credential_id FROM app.capital_exit_intents WHERE id=$1",
        )
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if claim_credential != Some(credential_id.as_uuid()) {
            return Err(StoreError::Forbidden);
        }
        if request.command_id != view.command_id
            || request.account_control_epoch != view.account_control_epoch
            || request.account_source_id != view.account_source_id
            || request.owner_binding_ref != view.owner_binding_ref
            || view.external_claim_id.as_ref() != Some(&request.external_claim_id)
        {
            return Err(StoreError::NativeIdentityConflict);
        }
        let (_, binding, _, current) =
            source(&mut tx, view.project_id, view.account_source_id).await?;
        if request.native_session_id != binding.native_session_id
            || request.native_account_id != binding.native_account_id
            || request.source_observation_id != current
        {
            return Err(StoreError::Invalid("capital_exit_native_source_mismatch"));
        }
        verify_observation(&mut tx, view.account_source_id, current, checked).await?;
        let last:Option<i64>=sqlx::query_scalar("SELECT max(sequence) FROM app.capital_exit_evidence WHERE intent_id=$1 AND account_source_id=$2").bind(id.as_uuid()).bind(request.account_source_id.as_uuid()).fetch_one(&mut *tx).await?;
        if last.is_some_and(|value| request.sequence.get() <= value as u64) {
            return Err(StoreError::Invalid("capital_exit_evidence_sequence"));
        }
        let fence: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT fence_evidence_id FROM app.capital_exit_intents WHERE id=$1",
        )
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if !matches!(
            request.evidence,
            CapitalExitEvidenceKindV1::FenceApplied { .. }
        ) && fence.is_none()
        {
            return Err(StoreError::Invalid(
                "capital_exit_owner_fence_unacknowledged",
            ));
        }
        let command_action: String =
            sqlx::query_scalar("SELECT command_action FROM app.capital_exit_intents WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let evidence_id = Id::new();
        let artifact = Id::new();
        let mut movement: Option<&str> = None;
        match &request.evidence {
            CapitalExitEvidenceKindV1::FenceApplied {
                controlled_strategy_ids,
                remaining_trading,
                ..
            } => {
                let supplied: std::collections::BTreeSet<_> =
                    controlled_strategy_ids.iter().collect();
                let expected: std::collections::BTreeSet<_> = reserve
                    .registration
                    .controlled_strategy_ids
                    .iter()
                    .collect();
                if supplied != expected {
                    return Err(StoreError::Invalid("capital_exit_gate_coverage_mismatch"));
                }
                sqlx::query("UPDATE app.capital_exit_intents SET fence_evidence_id=$2 WHERE id=$1")
                    .bind(id.as_uuid())
                    .bind(evidence_id.as_uuid())
                    .execute(&mut *tx)
                    .await?;
                view.remaining_trading = *remaining_trading;
                view.reason_codes.clear();
                if matches!(
                    view.state,
                    CapitalExitStateV1::Requested | CapitalExitStateV1::Fencing
                ) {
                    view.last_phase = view.state;
                    view.state = CapitalExitStateV1::WaitingEvidence;
                }
            }
            CapitalExitEvidenceKindV1::NativeProgress {
                phase,
                released_cash_amount,
                reason_codes,
                ..
            } => {
                if *phase == CapitalExitStateV1::Reducing
                    && (!matches!(command_action.as_str(), "START" | "RESUME")
                        || matches!(view.policy, CapitalExitPolicyV1::CashOnly {})
                        || matches!(
                            view.state,
                            CapitalExitStateV1::Paused
                                | CapitalExitStateV1::CancellingExit
                                | CapitalExitStateV1::CancelledReserved
                                | CapitalExitStateV1::ReconcilingWithdrawal
                        )
                        || matches!(&view.policy,CapitalExitPolicyV1::BoundedLimit {deadline,..} if *deadline<=checked))
                {
                    return Err(StoreError::Invalid("capital_exit_reduction_not_authorized"));
                }
                if *phase == CapitalExitStateV1::CancelledReserved
                    && view.state != CapitalExitStateV1::CancellingExit
                {
                    return Err(StoreError::Invalid("capital_exit_cancel_not_requested"));
                }
                if let Some(released) = released_cash_amount {
                    if released.currency != view.funds.reserved_amount.currency
                        || !released.amount.is_nonnegative()
                        || released.amount > view.funds.requested_amount.amount
                    {
                        return Err(StoreError::Invalid("capital_exit_released_cash"));
                    }
                    view.funds.released_cash_amount = Some(released.clone());
                    view.funds.unreleased_amount = Some(AccountMoneyV1 {
                        amount: domain::capital_exit::subtract(
                            &view.funds.requested_amount.amount,
                            &released.amount,
                        )?,
                        currency: released.currency.clone(),
                    });
                }
                view.last_phase = view.state;
                view.state = match command_action.as_str() {
                    "PAUSE" => CapitalExitStateV1::Paused,
                    "CANCEL" if *phase != CapitalExitStateV1::CancelledReserved => {
                        CapitalExitStateV1::CancellingExit
                    }
                    "RECONCILE_WITHDRAWAL"
                        if view.state == CapitalExitStateV1::ReconcilingWithdrawal =>
                    {
                        CapitalExitStateV1::ReconcilingWithdrawal
                    }
                    _ => *phase,
                };
                view.reason_codes = reason_codes.clone();
                view.funds.verified_withdrawable_amount = None;
                view.funds.withdrawability = CapitalExitWithdrawabilityV1::Unverified;
            }
            CapitalExitEvidenceKindV1::WithdrawabilityObserved {
                available_cash,
                basis,
                venue,
                ..
            } => {
                if venue != &reserve.registration.venue {
                    return Err(StoreError::Invalid(
                        "capital_exit_availability_venue_mismatch",
                    ));
                }
                domain::capital_exit::apply_availability(
                    &mut view,
                    available_cash,
                    *basis,
                    request.asof,
                    request.valid_until,
                )?;
                sqlx::query("UPDATE app.capital_exit_intents SET availability_observation_id=$2 WHERE id=$1").bind(id.as_uuid()).bind(current.as_uuid()).execute(&mut *tx).await?;
            }
            CapitalExitEvidenceKindV1::WithdrawalReconciled {
                amount,
                native_cash_movement_ref,
                external_transfer_ref,
                before_observation_id,
                after_observation_id,
                observed_managed_capital_after,
            } => {
                // The public command carries the report; this retained slot also
                // records whether that exact report is still pending. Completion
                // consumes it, so the same instruction cannot reconcile twice.
                let pending_report: bool = sqlx::query_scalar("SELECT withdrawal_report IS NOT NULL FROM app.capital_exit_intents WHERE id=$1").bind(id.as_uuid()).fetch_one(&mut *tx).await?;
                if !pending_report {
                    return Err(StoreError::Invalid(
                        "capital_exit_withdrawal_report_consumed",
                    ));
                }
                let CapitalExitOwnerInstructionV1::ReconcileWithdrawal {
                    user_reported_amount,
                    currency,
                    external_transfer_ref: reported_ref,
                } = view.owner_command.instruction.clone()
                else {
                    return Err(StoreError::Invalid(
                        "capital_exit_withdrawal_report_required",
                    ));
                };
                if view.state != CapitalExitStateV1::ReconcilingWithdrawal
                    || amount.amount != user_reported_amount
                    || amount.currency != currency
                    || amount.currency != view.funds.reserved_amount.currency
                    || reported_ref
                        .as_ref()
                        .is_some_and(|r| r != external_transfer_ref)
                    || *after_observation_id != current
                    || observed_managed_capital_after.currency != currency
                    || !observed_managed_capital_after.amount.is_nonnegative()
                {
                    return Err(StoreError::Invalid(
                        "capital_exit_withdrawal_evidence_mismatch",
                    ));
                }
                let before:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.native_account_observations WHERE id=$1 AND source_id=$2 AND jsonb_typeof(content->'snapshot')='object')").bind(before_observation_id.as_uuid()).bind(view.account_source_id.as_uuid()).fetch_one(&mut *tx).await?;
                if !before {
                    return Err(StoreError::Invalid(
                        "capital_exit_cash_movement_observations",
                    ));
                }
                let post_observation: serde_json::Value = sqlx::query_scalar("SELECT content FROM app.native_account_observations WHERE id=$1 AND source_id=$2").bind(after_observation_id.as_uuid()).bind(view.account_source_id.as_uuid()).fetch_one(&mut *tx).await?;
                let post_observation: AccountObservationSubmitV1 =
                    serde_json::from_value(post_observation).map_err(|_| StoreError::Integrity)?;
                let native_post = post_observation.snapshot.as_ref().and_then(|snapshot| {
                    snapshot
                        .total_equity
                        .iter()
                        .find(|money| money.currency == currency)
                });
                if native_post != Some(observed_managed_capital_after) {
                    return Err(StoreError::Invalid(
                        "capital_exit_native_post_transfer_basis_mismatch",
                    ));
                }
                let repeated_movement: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.capital_exit_evidence WHERE managed_account_key=$1 AND cash_movement_ref=$2)").bind(&view.managed_account_key).bind(native_cash_movement_ref).fetch_one(&mut *tx).await?;
                if repeated_movement {
                    return Err(StoreError::NativeIdentityConflict);
                }
                view.funds.reserved_amount.amount = domain::capital_exit::subtract(
                    &view.funds.reserved_amount.amount,
                    &amount.amount,
                )?;
                view.funds.reconciled_withdrawal_amount.amount = view
                    .funds
                    .reconciled_withdrawal_amount
                    .amount
                    .checked_add(&amount.amount)
                    .ok_or(StoreError::Integrity)?;
                view.account_control_revision =
                    reserve.revision.next().ok_or(StoreError::Integrity)?;
                view.funds.verified_withdrawable_amount = None;
                view.funds.withdrawability = CapitalExitWithdrawabilityV1::Unverified;
                view.last_phase = view.state;
                view.state = if view.funds.reserved_amount.amount.is_positive() {
                    CapitalExitStateV1::WaitingEvidence
                } else {
                    CapitalExitStateV1::Completed
                };
                // Adopt native post-transfer basis once. Never subtract amount again.
                sqlx::query("UPDATE app.managed_capital_reservations SET reserved_amount=$2::numeric,observed_managed_capital_after=$3,revision=$4,active_intent_id=CASE WHEN $2::numeric=0 THEN NULL ELSE active_intent_id END,latest_assessment=NULL,latest_assessment_artifact_id=NULL WHERE managed_account_key=$1")
                    .bind(&view.managed_account_key).bind(view.funds.reserved_amount.amount.as_decimal().to_plain_string()).bind(db::json(&ObservedManagedCapitalDocumentV1 { schema_version: SchemaV1, money: observed_managed_capital_after.clone(), source_observation_id: *after_observation_id })?).bind(view.account_control_revision.get() as i64).execute(&mut *tx).await?;
                sqlx::query("UPDATE app.capital_exit_intents SET withdrawal_report=NULL,availability_observation_id=NULL WHERE id=$1").bind(id.as_uuid()).execute(&mut *tx).await?;
                movement = Some(native_cash_movement_ref);
            }
        }
        publish(
            &mut tx,
            artifact,
            view.project_id,
            view.environment,
            "qz.capital_exit_owner_evidence",
            request,
            publisher,
        )
        .await?;
        let received = now(&mut tx).await?;
        if request.valid_until <= received {
            return Err(StoreError::Invalid("capital_exit_native_evidence_expired"));
        }
        sqlx::query("INSERT INTO app.capital_exit_evidence(id,intent_id,managed_account_key,owner_binding_ref,account_source_id,record_kind,external_message_id,sequence,artifact_id,content,cash_movement_ref,received_at) VALUES($1,$2,$3,$4,$5,'INTENT',$6,$7,$8,$9,$10,$11)")
            .bind(evidence_id.as_uuid()).bind(id.as_uuid()).bind(&view.managed_account_key).bind(&view.owner_binding_ref).bind(view.account_source_id.as_uuid()).bind(&request.external_message_id).bind(request.sequence.get() as i64).bind(artifact.as_uuid()).bind(db::json(request)?).bind(movement).bind(received).execute(&mut *tx).await?;
        view.evidence_refs.push(artifact);
        view.revision = view.revision.next().ok_or(StoreError::Integrity)?;
        view.updated_at = received;
        crate::forward::source_authority(&mut tx, actor, view.project_id, view.environment).await?;
        let result = finish_intent_command(&mut tx, prepared, view, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}

async fn verify_observation(
    tx: &mut Tx<'_>,
    source_id: Id,
    observation: Id,
    checked: DateTime<Utc>,
) -> Result<(), StoreError> {
    let row=sqlx::query("SELECT o.content,c.has_gap,c.connection,c.last_observed_at_ns,c.latest_snapshot_id FROM app.native_account_observations o JOIN app.native_account_cursors c ON c.source_id=o.source_id WHERE o.id=$1 AND o.source_id=$2")
        .bind(observation.as_uuid()).bind(source_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("capital_exit_account_observation_unavailable"))?;
    let value: AccountObservationSubmitV1 =
        serde_json::from_value(row.try_get("content")?).map_err(|_| StoreError::Integrity)?;
    let snapshot = value.snapshot.ok_or(StoreError::Invalid(
        "capital_exit_account_observation_unavailable",
    ))?;
    if snapshot.account_type != NativeAccountTypeV1::Cash
        || !snapshot.margins.is_empty()
        || snapshot.balances.len() != 1
    {
        return Err(StoreError::Invalid("capital_exit_cash_account_required"));
    }
    let nanos = u64::try_from(checked.timestamp_nanos_opt().ok_or(StoreError::Integrity)?)
        .map_err(|_| StoreError::Integrity)?;
    let original = snapshot.ts_init.get();
    let connected = domain::account_observation::connection(
        db::enum_value(&row, "connection")?,
        u64::try_from(row.try_get::<i64, _>("last_observed_at_ns")?)
            .map_err(|_| StoreError::Integrity)?,
        nanos,
    );
    if row.try_get::<bool, _>("has_gap")?
        || snapshot.is_stale
        || !snapshot.unpriced_instruments.is_empty()
        || !snapshot.stale_instruments.is_empty()
        || !snapshot.stale_currencies.is_empty()
        || connected != AccountConnectionFreshnessV1::Connected
        || original > nanos
        || nanos - original > 120_000_000_000
        || db::optional_id(&row, "latest_snapshot_id")? != Some(observation)
    {
        return Err(StoreError::Invalid("capital_exit_native_observation_stale"));
    }
    Ok(())
}
async fn refresh_view(tx: &mut Tx<'_>, view: &mut CapitalExitViewV1) -> Result<(), StoreError> {
    if view.owner_command.command_id != view.command_id
        || view.owner_command.account_control_epoch != view.account_control_epoch
    {
        return Err(StoreError::Integrity);
    }
    let checked = now(tx).await?;
    let row=sqlx::query("SELECT i.availability_observation_id,c.latest_snapshot_id,r.owner_binding_ref,r.paper_account_source_id,d.enabled,d.environments FROM app.capital_exit_intents i JOIN app.native_account_cursors c ON c.source_id=i.account_source_id JOIN app.managed_capital_reservations r ON r.managed_account_key=i.managed_account_key JOIN app.downstream_integrations d ON d.id=r.downstream_id WHERE i.id=$1")
        .bind(view.id.as_uuid()).fetch_one(&mut **tx).await?;
    let available = db::optional_id(&row, "availability_observation_id")?;
    let current = db::optional_id(&row, "latest_snapshot_id")?;
    let observation_fresh = if let Some(current) = current {
        match verify_observation(tx, view.account_source_id, current, checked).await {
            Ok(()) => true,
            Err(StoreError::Invalid(_)) => false,
            Err(error) => return Err(error),
        }
    } else {
        false
    };
    let paper_source_bound = view.environment == ForwardEnvironmentV1::Live
        || db::optional_id(&row, "paper_account_source_id")? == Some(view.account_source_id);
    let owner_current = domain::forward::downstream_environment_enabled(
        row.try_get("enabled")?,
        &row.try_get::<String, _>("environments")?,
        view.environment,
    ) && row.try_get::<String, _>("owner_binding_ref")?
        == view.owner_binding_ref
        && paper_source_bound;
    domain::capital_exit::freshness(
        view,
        checked,
        owner_current && observation_fresh && available.is_some() && available == current,
    );
    if (!owner_current || !observation_fresh) && view.state != CapitalExitStateV1::Completed {
        view.last_phase = view.state;
        view.state = CapitalExitStateV1::Blocked;
        view.reason_codes = vec![
            if !paper_source_bound {
                "paper_capital_exit_legacy_binding_unresolved"
            } else if owner_current {
                "capital_exit_native_observation_stale"
            } else {
                "account_owner_binding_unavailable"
            }
            .into(),
        ];
    }
    if matches!(&view.policy,CapitalExitPolicyV1::BoundedLimit {deadline,..} if *deadline<=checked)
        && matches!(
            view.state,
            CapitalExitStateV1::Reducing | CapitalExitStateV1::CancellingOpeners
        )
    {
        view.reason_codes
            .push("capital_exit_deadline_pause_evidence_pending".into());
    }
    Ok(())
}

/// Immutable replays return their original receipt earlier. A new response has
/// current readiness, while read-side capability/freshness blocks never replace
/// the durable command lifecycle needed to accept later native terminal evidence.
async fn finish_intent_command(
    tx: &mut Tx<'_>,
    prepared: commands::Prepared,
    mut view: CapitalExitViewV1,
    status: i32,
) -> Result<CommandResult<CapitalExitViewV1>, StoreError> {
    let mut projection = view.clone();
    refresh_view(tx, &mut projection).await?;
    // Persist invalidation, never resurrect old availability after a reconnect.
    // Phase and reason projections remain read-side, so the original command can
    // finish when fresh evidence arrives without another user report or cancel.
    view.funds.verified_withdrawable_amount = projection.funds.verified_withdrawable_amount.clone();
    view.funds.withdrawability = projection.funds.withdrawability;
    save_intent(tx, &view).await?;
    commands::finish(tx, prepared, projection, status).await
}

#[cfg(test)]
mod document_tests {
    use super::*;
    fn registration() -> CapitalExitOwnerRegistration {
        CapitalExitOwnerRegistration {
            managed_account_key: "sandbox:SIM-001".into(),
            owner_binding_ref: "configured-owner".into(),
            downstream_id: Id::new(),
            environment: ForwardEnvironmentV1::Paper,
            venue: "SIM".into(),
            native_account_id: "SIM-001".into(),
            native_trader_id: "TRADER-001".into(),
            native_client_id: "SandboxExecutionClient-SIM".into(),
            collateral_currency: "USD".into(),
            instrument_id: "TEST.SIM".into(),
            controlled_strategy_ids: vec!["TEST-001".into()],
        }
    }
    fn assert_app_document(value: serde_json::Value) {
        assert!(value.is_object());
        assert_eq!(value["schema_version"], serde_json::json!(1));
    }
    #[test]
    fn capital_exit_every_persistent_wrapper_satisfies_existing_document_domain_shape() {
        let owner = owner_document(&registration());
        assert_app_document(db::json(&owner).unwrap());
        assert_eq!(
            serde_json::from_value::<OwnerRegistrationDocumentV1>(db::json(&owner).unwrap())
                .unwrap(),
            owner
        );
        let basis = ObservedManagedCapitalDocumentV1 {
            schema_version: SchemaV1,
            money: AccountMoneyV1 {
                amount: "970.25".parse().unwrap(),
                currency: "USD".into(),
            },
            source_observation_id: Id::new(),
        };
        assert_app_document(db::json(&basis).unwrap());
        assert_eq!(
            serde_json::from_value::<ObservedManagedCapitalDocumentV1>(db::json(&basis).unwrap())
                .unwrap(),
            basis
        );
        let start = CapitalExitStartV1 {
            schema_version: SchemaV1,
            preview_id: Id::new(),
            expected_account_control_revision: Revision::INITIAL,
            acknowledged_plan_artifact_id: Id::new(),
            expected_source_observation_id: Id::new(),
        };
        assert_app_document(project_command(Id::new(), &start).unwrap());
        let preview = CapitalExitPreviewRequestV1 {
            schema_version: SchemaV1,
            account_source_id: Id::new(),
            expected_source_observation_id: Id::new(),
            scope: CapitalExitScopeV1::Amount {
                amount: "100".parse().unwrap(),
                currency: "USD".into(),
            },
            policy: CapitalExitPolicyV1::CashOnly {},
        };
        assert_app_document(project_command(Id::new(), &preview).unwrap());
        let action = CapitalExitActionV1::ReconcileWithdrawal {
            schema_version: SchemaV1,
            expected_revision: Revision::INITIAL,
            user_reported_amount: "30".parse().unwrap(),
            currency: "USD".into(),
            external_transfer_ref: Some("transfer-reference".into()),
        };
        assert_app_document(intent_command(Id::new(), &action).unwrap());
        assert_app_document(db::json(&action).unwrap());
        let claim = CapitalExitClaimV1 {
            schema_version: SchemaV1,
            expected_revision: Revision::INITIAL,
            command_id: Id::new(),
            account_control_epoch: DbCounter::new(1).unwrap(),
            account_source_id: Id::new(),
            owner_binding_ref: "configured-owner".into(),
            external_claim_id: "original-claim".into(),
        };
        assert_app_document(intent_command(Id::new(), &claim).unwrap());
    }
}

async fn require_bound_paper_source(
    tx: &mut Tx<'_>,
    view: &CapitalExitViewV1,
) -> Result<(), StoreError> {
    if view.environment == ForwardEnvironmentV1::Paper {
        let bound: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.managed_capital_reservations WHERE managed_account_key=$1 AND paper_account_source_id=$2)").bind(&view.managed_account_key).bind(view.account_source_id.as_uuid()).fetch_one(&mut **tx).await?;
        if !bound {
            return Err(StoreError::Invalid(
                "paper_capital_exit_legacy_binding_unresolved",
            ));
        }
    }
    Ok(())
}
