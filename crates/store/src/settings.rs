//! Explicit integration commands. Native credentials stay in the trusted file
//! adapter; PostgreSQL owns authority, immutable receipts and configuration CAS.
use crate::{
    authority::{self, Actor},
    commands,
    control::page,
    db, Store, StoreError,
};
use contracts::{
    control::{CommandResult, ListQuery, MachineScope, OperatorOperation, Page, PrincipalKind},
    settings::*,
    Id, SchemaV1,
};
use domain::research::invalid;
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
use std::future::Future;

type Tx<'a> = Transaction<'a, Postgres>;

/// Trusted adapter input, deliberately not deserializable from a remote request.
#[derive(Clone, Copy)]
pub struct NativeSecretBinding {
    pub id: Id,
    pub purpose: IntegrationSecretPurpose,
}

pub(crate) async fn read_authority(tx: &mut Tx<'_>, actor: &Actor) -> Result<(), StoreError> {
    match actor {
        Actor::Browser { .. } => authority::browser(tx, actor, false, false).await?,
        Actor::Machine { .. } => {
            let machine = authority::machine(tx, actor, false).await?;
            if !matches!(machine.kind, PrincipalKind::Cli | PrincipalKind::Automation)
                || machine.scopes != [MachineScope::DoctorRead]
            {
                return Err(StoreError::Forbidden);
            }
        }
    }
    Ok(())
}

fn runtime_view(row: &PgRow) -> Result<RuntimeView, StoreError> {
    if row.try_get::<String, _>("protocol_version")? != "1" {
        return Err(StoreError::Integrity);
    }
    let capabilities: Vec<String> = row.try_get("allowed_capabilities")?;
    Ok(RuntimeView {
        id: db::id(row.try_get("id")?)?,
        configuration: RuntimeConfigurationV1 {
            name: row.try_get("name")?,
            endpoint: row.try_get("endpoint")?,
            tls_policy: db::enum_value(row, "tls_policy")?,
            allowed_capabilities: capabilities
                .into_iter()
                .map(|value| serde_json::from_value(serde_json::Value::String(value)))
                .collect::<Result<_, _>>()
                .map_err(|_| StoreError::Integrity)?,
            enabled: row.try_get("enabled")?,
            development_http: row.try_get("development_http")?,
        },
        protocol_version: SchemaV1,
        credential_configured: !row.try_get::<String, _>("credential_ref")?.is_empty(),
        ca_configured: row
            .try_get::<Option<String>, _>("ca_certificate_ref")?
            .is_some(),
        last_capability_snapshot_artifact_id: db::optional_id(
            row,
            "last_capability_snapshot_artifact_id",
        )?,
        revision: db::revision(row.try_get("revision")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn downstream_view(row: &PgRow) -> Result<DownstreamView, StoreError> {
    let versions: Vec<String> = row.try_get("accepted_package_versions")?;
    Ok(DownstreamView {
        id: db::id(row.try_get("id")?)?,
        configuration: DownstreamConfigurationV1 {
            name: row.try_get("name")?,
            endpoint: row.try_get("endpoint")?,
            accepted_package_versions: versions
                .into_iter()
                .map(|value| serde_json::from_value(serde_json::Value::String(value)))
                .collect::<Result<_, _>>()
                .map_err(|_| StoreError::Integrity)?,
            environments: db::enum_value(row, "environments")?,
            enabled: row.try_get("enabled")?,
            development_http: row.try_get("development_http")?,
        },
        credential_configured: !row.try_get::<String, _>("credential_ref")?.is_empty(),
        revision: db::revision(row.try_get("revision")?)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn binding(
    value: String,
    purpose: IntegrationSecretPurpose,
) -> Result<NativeSecretBinding, StoreError> {
    Ok(NativeSecretBinding {
        id: value
            .try_into()
            .map_err(|_| invalid("credential_ref", "NATIVE_REFERENCE_UNAVAILABLE"))?,
        purpose,
    })
}

impl Store {
    /// The callback publishes or compares exact native encrypted bytes. Plaintext
    /// never enters Store, serde receipts, SQL parameters or application logs.
    pub async fn register_integration_secret<F, Fut>(
        &self,
        actor: &Actor,
        key: &str,
        request: &IntegrationSecretIntent,
        native: F,
    ) -> Result<CommandResult<IntegrationSecretView>, StoreError>
    where
        F: FnOnce(Id, IntegrationSecretPurpose, bool) -> Fut,
        Fut: Future<Output = Result<(), StoreError>>,
    {
        domain::settings::secret_intent(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::IntegrationSecretRegister,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay::<IntegrationSecretView>()? {
            if result.resource.id != prepared.target || result.resource.purpose != request.purpose {
                return Err(StoreError::Integrity);
            }
            native(result.resource.id, request.purpose, true).await?;
            commands::recheck_authority(&mut tx, actor, &prepared).await?;
            tx.commit().await?;
            return Ok(result);
        }
        native(prepared.target, request.purpose, false).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let created_at = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let resource = IntegrationSecretView {
            id: prepared.target,
            purpose: request.purpose,
            label: request.label.clone(),
            created_at,
        };
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        // Unknown commit outcomes preserve the native object. No speculative cleanup.
        tx.commit().await?;
        Ok(result)
    }

    pub async fn runtimes(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<RuntimeView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let rows = sqlx::query("SELECT * FROM app.runtime_integrations WHERE ($1::uuid IS NULL OR id<$1) ORDER BY id DESC LIMIT $2")
            .bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1)
            .fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter()
                .map(runtime_view)
                .collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |r| r.id,
        );
        tx.commit().await?;
        Ok(result)
    }

    pub async fn runtime(&self, actor: &Actor, id: Id) -> Result<RuntimeView, StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let row = sqlx::query("SELECT * FROM app.runtime_integrations WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let result = runtime_view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn create_runtime<F, Fut>(
        &self,
        actor: &Actor,
        key: &str,
        request: &RuntimeCreate,
        verify: F,
    ) -> Result<CommandResult<RuntimeView>, StoreError>
    where
        F: FnOnce(Vec<NativeSecretBinding>) -> Fut,
        Fut: Future<Output = Result<(), StoreError>>,
    {
        domain::settings::runtime_create(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::RuntimeCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let mut refs = vec![NativeSecretBinding {
            id: request.credential_ref,
            purpose: IntegrationSecretPurpose::Runtime,
        }];
        if let Some(id) = request.ca_certificate_ref {
            refs.push(NativeSecretBinding {
                id,
                purpose: IntegrationSecretPurpose::TlsCa,
            });
        }
        verify(refs).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let c = &request.configuration;
        let capabilities = c
            .allowed_capabilities
            .iter()
            .map(db::code)
            .collect::<Result<Vec<_>, _>>()?;
        let row = sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled,development_http,ca_certificate_ref) VALUES($1,$2,$3,$4,$5,$6,'1',$7,$8,$9) RETURNING *")
            .bind(prepared.target.as_uuid()).bind(&c.name).bind(&c.endpoint).bind(db::code(&c.tls_policy)?)
            .bind(request.credential_ref.to_string()).bind(capabilities).bind(c.enabled).bind(c.development_http)
            .bind(request.ca_certificate_ref.map(|id| id.to_string())).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, runtime_view(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn update_runtime<F, Fut>(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &RuntimeUpdate,
        verify: F,
    ) -> Result<CommandResult<RuntimeView>, StoreError>
    where
        F: FnOnce(Vec<NativeSecretBinding>) -> Fut,
        Fut: Future<Output = Result<(), StoreError>>,
    {
        domain::settings::runtime_update(request)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::RuntimeUpdate,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let old = sqlx::query("SELECT * FROM app.runtime_integrations WHERE id=$1 FOR UPDATE")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let current = db::revision(old.try_get("revision")?)?;
        if current != request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        let credential = match request.credential_ref {
            Some(id) => NativeSecretBinding {
                id,
                purpose: IntegrationSecretPurpose::Runtime,
            },
            None => binding(
                old.try_get("credential_ref")?,
                IntegrationSecretPurpose::Runtime,
            )?,
        };
        let ca = if request.configuration.tls_policy == TlsPolicy::PinnedCa {
            match request.ca_certificate_ref {
                Some(id) => Some(id),
                None => old
                    .try_get::<Option<String>, _>("ca_certificate_ref")?
                    .map(|value| value.try_into().map_err(|_| StoreError::Integrity))
                    .transpose()?,
            }
            .ok_or_else(|| invalid("ca_certificate_ref", "CA_POLICY_MISMATCH"))?
            .into()
        } else {
            None
        };
        let mut refs = vec![credential];
        if let Some(id) = ca {
            refs.push(NativeSecretBinding {
                id,
                purpose: IntegrationSecretPurpose::TlsCa,
            });
        }
        verify(refs).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let c = &request.configuration;
        let capabilities = c
            .allowed_capabilities
            .iter()
            .map(db::code)
            .collect::<Result<Vec<_>, _>>()?;
        let row = sqlx::query("UPDATE app.runtime_integrations SET name=$2,endpoint=$3,tls_policy=$4,credential_ref=$5,allowed_capabilities=$6,enabled=$7,development_http=$8,ca_certificate_ref=$9,last_capability_snapshot_artifact_id=NULL WHERE id=$1 RETURNING *")
            .bind(id.as_uuid()).bind(&c.name).bind(&c.endpoint).bind(db::code(&c.tls_policy)?)
            .bind(credential.id.to_string()).bind(capabilities).bind(c.enabled).bind(c.development_http)
            .bind(ca.map(|id: Id| id.to_string())).fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, runtime_view(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn downstreams(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<DownstreamView>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let rows = sqlx::query("SELECT * FROM app.downstream_integrations WHERE ($1::uuid IS NULL OR id<$1) ORDER BY id DESC LIMIT $2")
            .bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1)
            .fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter()
                .map(downstream_view)
                .collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |r| r.id,
        );
        tx.commit().await?;
        Ok(result)
    }

    pub async fn downstream(&self, actor: &Actor, id: Id) -> Result<DownstreamView, StoreError> {
        let mut tx = self.pool.begin().await?;
        read_authority(&mut tx, actor).await?;
        let row = sqlx::query("SELECT * FROM app.downstream_integrations WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let result = downstream_view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn create_downstream<F, Fut>(
        &self,
        actor: &Actor,
        key: &str,
        request: &DownstreamCreate,
        verify: F,
    ) -> Result<CommandResult<DownstreamView>, StoreError>
    where
        F: FnOnce(Vec<NativeSecretBinding>) -> Fut,
        Fut: Future<Output = Result<(), StoreError>>,
    {
        domain::settings::downstream_configuration(&request.configuration)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DownstreamCreate,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        verify(vec![NativeSecretBinding {
            id: request.credential_ref,
            purpose: IntegrationSecretPurpose::Downstream,
        }])
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let c = &request.configuration;
        let versions = c
            .accepted_package_versions
            .iter()
            .map(db::code)
            .collect::<Result<Vec<_>, _>>()?;
        let row = sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled,development_http) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING *")
            .bind(prepared.target.as_uuid()).bind(&c.name).bind(&c.endpoint).bind(request.credential_ref.to_string())
            .bind(versions).bind(db::code(&c.environments)?).bind(c.enabled).bind(c.development_http)
            .fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, downstream_view(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn update_downstream<F, Fut>(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &DownstreamUpdate,
        verify: F,
    ) -> Result<CommandResult<DownstreamView>, StoreError>
    where
        F: FnOnce(Vec<NativeSecretBinding>) -> Fut,
        Fut: Future<Output = Result<(), StoreError>>,
    {
        domain::settings::downstream_configuration(&request.configuration)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DownstreamUpdate,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let old = sqlx::query("SELECT * FROM app.downstream_integrations WHERE id=$1 FOR UPDATE")
            .bind(id.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let current = db::revision(old.try_get("revision")?)?;
        if current != request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        let credential = match request.credential_ref {
            Some(id) => NativeSecretBinding {
                id,
                purpose: IntegrationSecretPurpose::Downstream,
            },
            None => binding(
                old.try_get("credential_ref")?,
                IntegrationSecretPurpose::Downstream,
            )?,
        };
        verify(vec![credential]).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let c = &request.configuration;
        let versions = c
            .accepted_package_versions
            .iter()
            .map(db::code)
            .collect::<Result<Vec<_>, _>>()?;
        let row = sqlx::query("UPDATE app.downstream_integrations SET name=$2,endpoint=$3,credential_ref=$4,accepted_package_versions=$5,environments=$6,enabled=$7,development_http=$8 WHERE id=$1 RETURNING *")
            .bind(id.as_uuid()).bind(&c.name).bind(&c.endpoint).bind(credential.id.to_string())
            .bind(versions).bind(db::code(&c.environments)?).bind(c.enabled).bind(c.development_http)
            .fetch_one(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, downstream_view(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }
}
