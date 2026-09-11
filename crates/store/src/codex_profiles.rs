//! Operator profile commands and two short transactions around native inspection.
//! No plaintext key, OAuth token, native history or subprocess enters the Store.
use crate::{authority::Actor, commands, control::page, db, settings::read_authority, Store, StoreError};
use chrono::{DateTime, Duration, Utc};
use contracts::{codex::*, control::{CommandResult, ListQuery, OperatorOperation, Page}, Id, SchemaV1};
use domain::codex::settings as rules;
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use std::future::Future;

type Tx<'a> = Transaction<'a, Postgres>;

/// Only a trusted local deployment/vault adapter consumes this binding check.
pub struct CodexBindingCheck {
    pub home_binding: String,
    pub profile_origin: ProfileOrigin,
    pub credential_ref: Option<Id>,
}

/// A private service snapshot, not a caller-supplied serialized configuration.
pub struct CodexProfileSnapshot {
    pub profile: CodexProfileViewV1,
    pub credential_ref: Option<Id>,
}

pub enum CodexProbePreparation {
    Replay(Box<CommandResult<CodexProbeViewV1>>),
    Execute(Box<CodexProbeTicket>),
}

pub struct CodexProbeTicket {
    pub snapshot: CodexProfileSnapshot,
    pub started_at: DateTime<Utc>,
    actor: Actor,
    key: String,
    request: CodexProbeRequestV1,
}

fn view(row: &PgRow) -> Result<CodexProfileViewV1, StoreError> {
    let binding: String = row.try_get("codex_home_ref")?;
    let endpoint: Option<String> = row.try_get("custom_base_url")?;
    Ok(CodexProfileViewV1 {
        id: db::id(row.try_get("id")?)?,
        name: row.try_get("name")?,
        home_binding: rules::home_binding(&binding).is_ok().then_some(binding),
        profile_origin: db::enum_value(row, "profile_origin")?,
        connection_mode: db::enum_value(row, "connection_mode")?,
        custom_base_url: endpoint.filter(|value| rules::provider_url(value).is_ok()),
        credential_configured: row.try_get::<Option<String>, _>("custom_api_key_ref")?.is_some(),
        model_settings: SavedModelSettingsV1 {
            schema_version: SchemaV1,
            use_default_model_settings: row.try_get("use_default_model_settings")?,
            saved_model: row.try_get("saved_model")?,
            saved_reasoning_effort: row.try_get("saved_reasoning_effort")?,
            saved_fast_mode: row.try_get("saved_fast_mode")?,
        },
        revision: db::revision(row.try_get("revision")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn observation(row: &PgRow) -> Result<CodexProbeViewV1, StoreError> {
    let document: Value = row.try_get("outcome")?;
    let outcome = serde_json::from_value(document.get("result").cloned().ok_or(StoreError::Integrity)?)
        .map_err(|_| StoreError::Integrity)?;
    Ok(CodexProbeViewV1 {
        schema_version: SchemaV1,
        id: db::id(row.try_get("id")?)?,
        profile_id: db::id(row.try_get("profile_id")?)?,
        profile_revision: db::revision(row.try_get("profile_revision")?)?,
        observed_at: row.try_get("observed_at")?,
        valid_until: row.try_get("valid_until")?,
        outcome,
    })
}

async fn row(tx: &mut Tx<'_>, id: Id, write: bool) -> Result<PgRow, StoreError> {
    let query = if write {
        "SELECT * FROM app.codex_profiles WHERE id=$1 FOR UPDATE"
    } else {
        "SELECT * FROM app.codex_profiles WHERE id=$1 FOR SHARE"
    };
    sqlx::query(query).bind(id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)
}

impl Store {
    pub async fn authorize_codex_settings_read(&self, actor: &Actor) -> Result<(), StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn codex_profiles(&self, actor: &Actor, query: &ListQuery) -> Result<Page<CodexProfileViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let rows = sqlx::query("SELECT * FROM app.codex_profiles WHERE ($1::uuid IS NULL OR id<$1) ORDER BY id DESC LIMIT $2")
            .bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(rows.iter().map(view).collect::<Result<Vec<_>, _>>()?, query.limit, |p| p.id);
        tx.commit().await?;
        Ok(result)
    }

    pub async fn codex_profile(&self, actor: &Actor, id: Id) -> Result<CodexProfileViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let result = view(&row(&mut tx, id, false).await?)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn create_codex_profile<F, Fut>(
        &self, actor: &Actor, key: &str, request: &CodexProfileCreateV1, verify: F,
    ) -> Result<CommandResult<CodexProfileViewV1>, StoreError>
    where F: FnOnce(CodexBindingCheck) -> Fut, Fut: Future<Output = Result<(), StoreError>> {
        rules::profile_create(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(&mut tx, actor, OperatorOperation::CodexProfileCreate, key, None, db::json(request)?).await?;
        if let Some(result) = prepared.replay()? { tx.commit().await?; return Ok(result); }
        let occupied: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.codex_profiles WHERE codex_home_ref=$1)")
            .bind(&request.home_binding).fetch_one(&mut *tx).await?;
        if occupied { return Err(StoreError::Conflict); }
        let (mode, endpoint, credential) = match &request.connection {
            CodexConnectionCreateV1::System => (ConnectionMode::System, None, None),
            CodexConnectionCreateV1::CustomProvider { base_url, credential_ref } =>
                (ConnectionMode::CustomProvider, Some(base_url.as_str()), Some(*credential_ref)),
        };
        verify(CodexBindingCheck { home_binding: request.home_binding.clone(), profile_origin: request.profile_origin, credential_ref: credential }).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let settings = &request.model_settings;
        let row = sqlx::query("INSERT INTO app.codex_profiles(id,name,connection_mode,profile_origin,codex_home_ref,custom_base_url,custom_api_key_ref,custom_provider_options,use_default_model_settings,saved_model,saved_reasoning_effort,saved_fast_mode) VALUES($1,$2,$3,$4,$5,$6,$7,NULL,$8,$9,$10,$11) RETURNING *")
            .bind(prepared.target.as_uuid()).bind(&request.name).bind(db::code(&mode)?)
            .bind(db::code(&request.profile_origin)?).bind(&request.home_binding)
            .bind(endpoint).bind(credential.map(|id| id.to_string()))
            .bind(settings.use_default_model_settings).bind(&settings.saved_model)
            .bind(&settings.saved_reasoning_effort).bind(settings.saved_fast_mode).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, view(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn update_codex_profile<F, Fut>(
        &self, actor: &Actor, key: &str, id: Id, request: &CodexProfileUpdateV1, verify: F,
    ) -> Result<CommandResult<CodexProfileViewV1>, StoreError>
    where F: FnOnce(CodexBindingCheck) -> Fut, Fut: Future<Output = Result<(), StoreError>> {
        rules::profile_update(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(&mut tx, actor, OperatorOperation::CodexProfileUpdate, key, Some(id), db::json(request)?).await?;
        if let Some(result) = prepared.replay()? { tx.commit().await?; return Ok(result); }
        let old = row(&mut tx, id, true).await?;
        let current = db::revision(old.try_get("revision")?)?;
        if current != request.expected_revision { return Err(StoreError::RevisionConflict { current }); }
        let (mode, endpoint, credential) = match &request.connection {
            CodexConnectionUpdateV1::System => (ConnectionMode::System, None, None),
            CodexConnectionUpdateV1::CustomProvider { base_url, credential_ref } => {
                let credential = match credential_ref {
                    Some(id) => *id,
                    None if old.try_get::<String, _>("connection_mode")? == "CUSTOM_PROVIDER" => old.try_get::<Option<String>, _>("custom_api_key_ref")?
                        .ok_or_else(|| domain::research::invalid("connection.credential_ref", "CUSTOM_CREDENTIAL_REQUIRED"))?
                        .try_into().map_err(|_| StoreError::Integrity)?,
                    None => return Err(domain::research::invalid("connection.credential_ref", "CUSTOM_CREDENTIAL_REQUIRED").into()),
                };
                (ConnectionMode::CustomProvider, Some(base_url.as_str()), Some(credential))
            }
        };
        verify(CodexBindingCheck { home_binding: old.try_get("codex_home_ref")?, profile_origin: db::enum_value(&old, "profile_origin")?, credential_ref: credential }).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let settings = &request.model_settings;
        let row = sqlx::query("UPDATE app.codex_profiles SET name=$2,connection_mode=$3,custom_base_url=$4,custom_api_key_ref=$5,custom_provider_options=NULL,use_default_model_settings=$6,saved_model=$7,saved_reasoning_effort=$8,saved_fast_mode=$9 WHERE id=$1 RETURNING *")
            .bind(id.as_uuid()).bind(&request.name).bind(db::code(&mode)?).bind(endpoint)
            .bind(credential.map(|id| id.to_string())).bind(settings.use_default_model_settings)
            .bind(&settings.saved_model).bind(&settings.saved_reasoning_effort).bind(settings.saved_fast_mode).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, view(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn prepare_codex_probe(&self, actor: &Actor, key: &str, request: &CodexProbeRequestV1) -> Result<CodexProbePreparation, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(&mut tx, actor, OperatorOperation::CodexProbe, key, Some(request.profile_id), db::json(request)?).await?;
        if let Some(result) = prepared.replay()? { tx.commit().await?; return Ok(CodexProbePreparation::Replay(Box::new(result))); }
        let row = row(&mut tx, request.profile_id, false).await?;
        let profile = view(&row)?;
        if profile.revision != request.expected_revision { return Err(StoreError::RevisionConflict { current: profile.revision }); }
        if row.try_get::<Option<Value>, _>("custom_provider_options")?.is_some() {
            return Err(domain::DomainError::CapabilityUnavailable("unsupported_native_provider_options").into());
        }
        let credential_ref = if profile.connection_mode == ConnectionMode::CustomProvider {
            Some(row.try_get::<Option<String>, _>("custom_api_key_ref")?.ok_or(StoreError::Integrity)?.try_into().map_err(|_| StoreError::Integrity)?)
        } else { None };
        let started_at = sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&mut *tx).await?;
        let ticket = CodexProbeTicket { snapshot: CodexProfileSnapshot { profile, credential_ref }, started_at, actor: actor.clone(), key: key.to_owned(), request: request.clone() };
        tx.commit().await?;
        Ok(CodexProbePreparation::Execute(Box::new(ticket)))
    }

    pub async fn complete_codex_probe(&self, ticket: CodexProbeTicket, outcome: CodexProbeOutcomeV1) -> Result<CommandResult<CodexProbeViewV1>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(&mut tx, &ticket.actor, OperatorOperation::CodexProbe, &ticket.key, Some(ticket.request.profile_id), db::json(&ticket.request)?).await?;
        if let Some(result) = prepared.replay()? { tx.commit().await?; return Ok(result); }
        let profile = view(&row(&mut tx, ticket.request.profile_id, false).await?)?;
        if profile.revision != ticket.request.expected_revision { return Err(StoreError::RevisionConflict { current: profile.revision }); }
        let observed: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&mut *tx).await?;
        if observed < ticket.started_at || observed > ticket.started_at + Duration::seconds(120) {
            return Err(domain::DomainError::CapabilityUnavailable("codex_probe_expired").into());
        }
        rules::probe_outcome(&outcome, &profile.model_settings, profile.connection_mode, profile.revision, ticket.started_at, observed)?;
        let document = json!({"schema_version":1,"result":outcome});
        if serde_json::to_vec(&document).map_err(|_| StoreError::Integrity)?.len() > 2 * 1024 * 1024 {
            return Err(StoreError::Invalid("codex_catalog_size"));
        }
        commands::recheck_authority(&mut tx, &ticket.actor, &prepared).await?;
        let row = sqlx::query("INSERT INTO app.codex_profile_observations(profile_id,profile_revision,observed_at,valid_until,outcome) VALUES($1,$2,$3,$4,$5) RETURNING *")
            .bind(profile.id.as_uuid()).bind(profile.revision.get() as i64).bind(observed)
            .bind(observed + Duration::seconds(60)).bind(document).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, observation(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn codex_observation(&self, actor: &Actor, id: Id) -> Result<CodexObservationV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let profile = view(&row(&mut tx, id, false).await?)?;
        let recent = sqlx::query("SELECT * FROM app.codex_profile_observations WHERE profile_id=$1 ORDER BY observed_at DESC,id DESC LIMIT 1")
            .bind(id.as_uuid()).fetch_optional(&mut *tx).await?.as_ref().map(observation).transpose()?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()").fetch_one(&mut *tx).await?;
        let state = match &recent {
            None => CodexObservationStateV1::NeverProbed,
            Some(value) if value.profile_revision != profile.revision || value.valid_until <= now => CodexObservationStateV1::Stale,
            Some(value) => match value.outcome {
                CodexProbeOutcomeV1::Available { .. } => CodexObservationStateV1::Available,
                CodexProbeOutcomeV1::Unavailable { .. } => CodexObservationStateV1::Unavailable,
            },
        };
        tx.commit().await?;
        Ok(CodexObservationV1 { schema_version: SchemaV1, profile_id: id, profile_revision: profile.revision, state, observation: recent })
    }
}
