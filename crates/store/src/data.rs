//! Operator data administration over the existing source/grant/dataset tables.
//! Licenses and dataset provenance remain distinct; no request can mint REAL/PASS.
use crate::{
    authority::{self, Actor},
    commands, db, Store, StoreError,
};
use chrono::{DateTime, Utc};
use contracts::{control::*, data::*, Id, SchemaV1};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;

pub(crate) async fn read_authority(tx: &mut Tx<'_>, actor: &Actor) -> Result<(), StoreError> {
    match actor {
        Actor::Browser { .. } => authority::browser(tx, actor, false, false).await,
        Actor::Machine { .. } => {
            let machine = authority::machine(tx, actor, false).await?;
            if machine.kind != PrincipalKind::Cli
                || machine.scopes.len() != 1
                || !machine.scopes.contains(&MachineScope::DoctorRead)
            {
                return Err(StoreError::Forbidden);
            }
            Ok(())
        }
    }
}

pub(crate) fn source(row: &PgRow) -> Result<DataSourceView, StoreError> {
    Ok(DataSourceView {
        id: db::id(row.try_get("id")?)?,
        name: row.try_get("name")?,
        runtime_id: db::id(row.try_get("runtime_id")?)?,
        native_catalog_ref: row.try_get("native_catalog_ref")?,
        provider_kind: row.try_get("provider_kind")?,
        enabled: row.try_get("enabled")?,
        revision: db::revision(row.try_get("revision")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn grant(row: &PgRow, checked: DateTime<Utc>) -> Result<DataGrantView, StoreError> {
    let valid_from = row.try_get("valid_from")?;
    let valid_until = row.try_get("valid_until")?;
    Ok(DataGrantView {
        id: db::id(row.try_get("id")?)?,
        source_id: db::id(row.try_get("source_id")?)?,
        version: db::revision(i64::from(row.try_get::<i32, _>("version")?))?,
        license_reference: row.try_get("license_reference")?,
        evidence_artifact_id: db::id(row.try_get("evidence_artifact_id")?)?,
        allowed_uses: db::enum_value(row, "allowed_uses")?,
        valid_from,
        valid_until,
        created_at: row.try_get("created_at")?,
        license_state: domain::data::license_state(
            valid_from,
            valid_until,
            row.try_get("revoked")?,
            checked,
        ),
        checked_at: checked,
    })
}

fn revocation(row: &PgRow) -> Result<DataGrantRevocationView, StoreError> {
    Ok(DataGrantRevocationView {
        id: db::id(row.try_get("id")?)?,
        grant_id: db::id(row.try_get("grant_id")?)?,
        effective_at: row.try_get("effective_at")?,
        reason_code: row.try_get("reason_code")?,
        reason: row.try_get("reason")?,
        created_at: row.try_get("created_at")?,
    })
}

const UNIVERSE_FIELDS: &str = "u.*,EXISTS(SELECT 1 FROM app.dataset_revisions d JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id WHERE d.universe_version_id=u.id) AS native_registered";

fn universe(row: &PgRow) -> Result<UniverseView, StoreError> {
    Ok(UniverseView {
        id: db::id(row.try_get("id")?)?,
        name: row.try_get("name")?,
        registration_state: if row.try_get("native_registered")? {
            UniverseRegistrationState::NativeMetadata
        } else {
            UniverseRegistrationState::LegacyUnverified
        },
        membership_artifact_id: db::id(row.try_get("membership_artifact_id")?)?,
        instrument_definitions_artifact_id: db::id(
            row.try_get("instrument_definition_artifact_id")?,
        )?,
        calendar_ref: row.try_get("calendar_ref")?,
        calendar_version: row.try_get("calendar_version")?,
        selection_asof: row.try_get("selection_asof")?,
        has_historical_membership: row.try_get("has_historical_membership")?,
        coverage_start: row.try_get("coverage_start")?,
        coverage_end: row.try_get("coverage_end")?,
        created_at: row.try_get("created_at")?,
    })
}

pub(crate) async fn universe_in_tx(tx: &mut Tx<'_>, id: Id) -> Result<UniverseView, StoreError> {
    let row = sqlx::query(&format!(
        "SELECT {UNIVERSE_FIELDS} FROM app.universe_versions u WHERE u.id=$1"
    ))
    .bind(id.as_uuid())
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(StoreError::NotFound)?;
    universe(&row)
}

pub(crate) fn dataset(row: &PgRow, checked: DateTime<Utc>) -> Result<DatasetView, StoreError> {
    Ok(DatasetView {
        id: db::id(row.try_get("id")?)?,
        source_id: db::id(row.try_get("source_id")?)?,
        data_use_grant_id: db::id(row.try_get("data_use_grant_id")?)?,
        native_snapshot_ref: row.try_get("native_snapshot_ref")?,
        storage_version: row.try_get("native_storage_version")?,
        universe_version_id: db::id(row.try_get("universe_version_id")?)?,
        schema_version: row.try_get("schema_version")?,
        data_kind: db::enum_value(row, "data_kind")?,
        partition: db::enum_value(row, "partition_role")?,
        event_start: row.try_get("event_start")?,
        event_end: row.try_get("event_end")?,
        available_through: row.try_get("available_through")?,
        row_count: row
            .try_get::<i64, _>("row_count")?
            .to_string()
            .try_into()
            .map_err(|_| StoreError::Integrity)?,
        timezone: row.try_get("timezone")?,
        quality_artifact_id: db::id(row.try_get("quality_artifact_id")?)?,
        pit_status: db::enum_value(row, "pit_status")?,
        revision_policy: db::enum_value(row, "revision_policy")?,
        origin: db::enum_value(row, "origin")?,
        created_at: row.try_get("created_at")?,
        native_metadata_artifact_id: row
            .try_get::<Option<uuid::Uuid>, _>("native_metadata_artifact_id")?
            .map(db::id)
            .transpose()?,
        registration_observed_at: row.try_get("registration_observed_at")?,
        source_enabled: row.try_get("source_enabled")?,
        runtime_enabled: row.try_get("runtime_enabled")?,
        license_state: domain::data::license_state(
            row.try_get("valid_from")?,
            row.try_get("valid_until")?,
            row.try_get("revoked")?,
            checked,
        ),
        checked_at: checked,
    })
}

pub(crate) async fn dataset_in_tx(tx: &mut Tx<'_>, id: Id) -> Result<DatasetView, StoreError> {
    let checked: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let row = sqlx::query("SELECT d.*,s.enabled AS source_enabled,r.enabled AS runtime_enabled,g.valid_from,g.valid_until,e.native_metadata_artifact_id,e.observed_at AS registration_observed_at,EXISTS(SELECT 1 FROM app.data_use_revocations v WHERE v.grant_id=g.id AND v.effective_at<=$2) AS revoked FROM app.dataset_revisions d JOIN app.data_sources s ON s.id=d.source_id JOIN app.runtime_integrations r ON r.id=s.runtime_id JOIN app.data_use_grants g ON g.id=d.data_use_grant_id LEFT JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id WHERE d.id=$1")
        .bind(id.as_uuid()).bind(checked).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    dataset(&row, checked)
}

impl Store {
    pub async fn create_data_source(
        &self,
        actor: &Actor,
        key: &str,
        request: &DataSourceCreate,
    ) -> Result<CommandResult<DataSourceView>, StoreError> {
        domain::data::source_create(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DataSourceCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        sqlx::query("SELECT id FROM app.runtime_integrations WHERE id=$1 FOR SHARE")
            .bind(request.runtime_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.data_sources WHERE runtime_id=$1 AND native_catalog_ref=$2)")
            .bind(request.runtime_id.as_uuid()).bind(&request.native_catalog_ref).fetch_one(&mut *tx).await?;
        if duplicate {
            return Err(StoreError::NativeIdentityConflict);
        }
        let row = sqlx::query("INSERT INTO app.data_sources(id,name,runtime_id,native_catalog_ref,provider_kind,enabled) VALUES($1,$2,$3,$4,$5,$6) RETURNING *")
            .bind(prepared.target.as_uuid()).bind(&request.name).bind(request.runtime_id.as_uuid())
            .bind(&request.native_catalog_ref).bind(request.provider_kind.code()).bind(request.enabled)
            .fetch_one(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, source(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn update_data_source(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &DataSourceUpdate,
    ) -> Result<CommandResult<DataSourceView>, StoreError> {
        domain::data::source_update(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DataSourceUpdate,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let current: i64 =
            sqlx::query_scalar("SELECT revision FROM app.data_sources WHERE id=$1 FOR UPDATE")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let current = db::revision(current)?;
        if current != request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        let row =
            sqlx::query("UPDATE app.data_sources SET name=$2,enabled=$3 WHERE id=$1 RETURNING *")
                .bind(id.as_uuid())
                .bind(&request.name)
                .bind(request.enabled)
                .fetch_one(&mut *tx)
                .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, source(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn get_data_source(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<DataSourceView, StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let row = sqlx::query("SELECT * FROM app.data_sources WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let result = source(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn list_data_sources(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<DataSourceView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let rows = sqlx::query("SELECT * FROM app.data_sources WHERE ($1::uuid IS NULL OR id<$1) ORDER BY id DESC LIMIT $2")
            .bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit) + 1).fetch_all(&mut *tx).await?;
        let mut items = rows.iter().map(source).collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() > query.limit as usize {
            items.truncate(query.limit as usize);
            items.last().map(|item| item.id)
        } else {
            None
        };
        tx.commit().await?;
        Ok(Page {
            schema_version: SchemaV1,
            items,
            next_cursor,
        })
    }

    pub async fn create_data_grant(
        &self,
        actor: &Actor,
        key: &str,
        request: &DataGrantCreate,
    ) -> Result<CommandResult<DataGrantView>, StoreError> {
        domain::data::grant_create(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DataGrantCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let enabled: bool =
            sqlx::query_scalar("SELECT enabled FROM app.data_sources WHERE id=$1 FOR UPDATE")
                .bind(request.source_id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        if !enabled {
            return Err(StoreError::Invalid("data_source_disabled"));
        }
        let proof: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 AND kind='REPORT' AND byte_count>0 AND created_by='OPERATOR' AND access_class IN ('OPERATOR','RESEARCH') AND storage_backend='LOCAL')")
            .bind(request.evidence_artifact_id.as_uuid()).fetch_one(&mut *tx).await?;
        if !proof {
            return Err(StoreError::Invalid("license_evidence_artifact"));
        }
        let version: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(version)::bigint,0)+1 FROM app.data_use_grants WHERE source_id=$1",
        )
        .bind(request.source_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let version = i32::try_from(version).map_err(|_| StoreError::Integrity)?;
        let row = sqlx::query("INSERT INTO app.data_use_grants(id,source_id,version,license_reference,evidence_artifact_id,allowed_uses,valid_from,valid_until,authorized_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'OPERATOR') RETURNING *,false AS revoked")
            .bind(prepared.target.as_uuid()).bind(request.source_id.as_uuid()).bind(version)
            .bind(&request.license_reference).bind(request.evidence_artifact_id.as_uuid())
            .bind(request.allowed_uses.code()).bind(request.valid_from).bind(request.valid_until)
            .fetch_one(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let checked = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let result = commands::finish(&mut tx, prepared, grant(&row, checked)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn revoke_data_grant(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &DataGrantRevoke,
    ) -> Result<CommandResult<DataGrantRevocationView>, StoreError> {
        domain::data::grant_revoke(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DataGrantRevoke,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        // Same source -> grant ordering as native input admission and registration.
        let source_id: uuid::Uuid =
            sqlx::query_scalar("SELECT source_id FROM app.data_use_grants WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        sqlx::query("SELECT id FROM app.data_sources WHERE id=$1 FOR SHARE")
            .bind(source_id)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM app.data_use_grants WHERE id=$1 FOR UPDATE")
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let effective = request.effective_at.unwrap_or(now);
        if effective < now {
            return Err(StoreError::Invalid("revocation_effective_at"));
        }
        let row = sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,$2,$3,$4) RETURNING *")
            .bind(id.as_uuid()).bind(effective).bind(&request.reason_code).bind(&request.reason).fetch_one(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, revocation(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn list_data_grants(
        &self,
        actor: &Actor,
        source_id: Id,
        query: &ListQuery,
    ) -> Result<Page<DataGrantView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        sqlx::query("SELECT id FROM app.data_sources WHERE id=$1")
            .bind(source_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let checked = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let rows = sqlx::query("SELECT g.*,EXISTS(SELECT 1 FROM app.data_use_revocations r WHERE r.grant_id=g.id AND r.effective_at<=$4) AS revoked FROM app.data_use_grants g WHERE g.source_id=$1 AND ($2::uuid IS NULL OR g.id<$2) ORDER BY g.id DESC LIMIT $3")
            .bind(source_id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit) + 1).bind(checked)
            .fetch_all(&mut *tx).await?;
        let mut items = rows
            .iter()
            .map(|row| grant(row, checked))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() > query.limit as usize {
            items.truncate(query.limit as usize);
            items.last().map(|item| item.id)
        } else {
            None
        };
        tx.commit().await?;
        Ok(Page {
            schema_version: SchemaV1,
            items,
            next_cursor,
        })
    }

    pub async fn list_data_revocations(
        &self,
        actor: &Actor,
        grant_id: Id,
        query: &ListQuery,
    ) -> Result<Page<DataGrantRevocationView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        sqlx::query("SELECT id FROM app.data_use_grants WHERE id=$1")
            .bind(grant_id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query("SELECT * FROM app.data_use_revocations WHERE grant_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(grant_id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit) + 1).fetch_all(&mut *tx).await?;
        let mut items = rows.iter().map(revocation).collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() > query.limit as usize {
            items.truncate(query.limit as usize);
            items.last().map(|item| item.id)
        } else {
            None
        };
        tx.commit().await?;
        Ok(Page {
            schema_version: SchemaV1,
            items,
            next_cursor,
        })
    }

    pub async fn get_dataset_revision(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<DatasetView, StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let result = dataset_in_tx(&mut tx, id).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn list_dataset_revisions(
        &self,
        actor: &Actor,
        query: &DataListQuery,
    ) -> Result<Page<DatasetView>, StoreError> {
        domain::data::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let checked: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let partition = query.partition.map(|partition| partition.code());
        let rows = sqlx::query("SELECT d.*,s.enabled AS source_enabled,r.enabled AS runtime_enabled,g.valid_from,g.valid_until,e.native_metadata_artifact_id,e.observed_at AS registration_observed_at,EXISTS(SELECT 1 FROM app.data_use_revocations v WHERE v.grant_id=g.id AND v.effective_at<=$5) AS revoked FROM app.dataset_revisions d JOIN app.data_sources s ON s.id=d.source_id JOIN app.runtime_integrations r ON r.id=s.runtime_id JOIN app.data_use_grants g ON g.id=d.data_use_grant_id LEFT JOIN app.dataset_registration_evidence e ON e.dataset_revision_id=d.id WHERE ($1::uuid IS NULL OR d.source_id=$1) AND ($2::text IS NULL OR d.partition_role=$2) AND ($3::uuid IS NULL OR d.id<$3) ORDER BY d.id DESC LIMIT $4")
            .bind(query.source_id.map(Id::as_uuid)).bind(partition).bind(query.cursor.map(Id::as_uuid))
            .bind(i64::from(query.limit) + 1).bind(checked).fetch_all(&mut *tx).await?;
        let mut items = rows
            .iter()
            .map(|row| dataset(row, checked))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() > query.limit as usize {
            items.truncate(query.limit as usize);
            items.last().map(|item| item.id)
        } else {
            None
        };
        tx.commit().await?;
        Ok(Page {
            schema_version: SchemaV1,
            items,
            next_cursor,
        })
    }

    pub async fn get_universe_version(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<UniverseView, StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let result = universe_in_tx(&mut tx, id).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn list_universe_versions(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<UniverseView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let rows = sqlx::query(&format!("SELECT {UNIVERSE_FIELDS} FROM app.universe_versions u WHERE ($1::uuid IS NULL OR u.id<$1) ORDER BY u.id DESC LIMIT $2"))
            .bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit) + 1).fetch_all(&mut *tx).await?;
        let mut items = rows.iter().map(universe).collect::<Result<Vec<_>, _>>()?;
        let next_cursor = if items.len() > query.limit as usize {
            items.truncate(query.limit as usize);
            items.last().map(|item| item.id)
        } else {
            None
        };
        tx.commit().await?;
        Ok(Page {
            schema_version: SchemaV1,
            items,
            next_cursor,
        })
    }
}
