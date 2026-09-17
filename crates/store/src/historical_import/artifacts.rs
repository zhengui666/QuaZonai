use super::*;

/// Deployment-native inputs only; never deserialized from an HTTP import body.
pub struct HistoricalImportSource {
    pub rows: HistoricalRowExportV1,
    pub artifacts: Option<HistoricalArtifactExportV1>,
}
pub struct HistoricalArtifactPublication {
    pub export_ref: Id,
    pub source_object: Id,
    pub target: Option<Id>,
    pub byte_count: DbCounter,
    pub existing: bool,
    pub dry_run: bool,
}
fn artifact_table(name: &str) -> bool {
    matches!(name, "mission_artifacts" | "alpha_signal_artifacts")
}

pub(super) async fn publish<P, Published>(
    tx: &mut Transaction<'_, Postgres>,
    staged: &BTreeMap<String, StagedProjection>,
    source: &HistoricalRowExportV1,
    artifacts: Option<&HistoricalArtifactExportV1>,
    report: Id,
    request: &HistoricalImportRequestV1,
    publish: &mut P,
) -> Result<bool, StoreError>
where
    P: FnMut(HistoricalArtifactPublication) -> Published,
    Published: std::future::Future<Output = Result<(), StoreError>>,
{
    let source_records = source
        .inspection
        .tables
        .iter()
        .filter(|t| artifact_table(&t.table))
        .map(|t| t.rows.get())
        .sum::<u64>();
    sqlx::query("CREATE TEMP TABLE historical_artifact_stage(source_table text,source_id uuid,record_id uuid,existing_bytes bigint,source_outcome text,verified_readable boolean NOT NULL DEFAULT false,stored boolean NOT NULL DEFAULT false,byte_count bigint,PRIMARY KEY(source_table,source_id)) ON COMMIT DROP").execute(&mut **tx).await?;
    let mut projected = 0;
    for (name, table) in staged {
        let table_name = name.strip_prefix("public.").ok_or_else(invalid)?;
        if !artifact_table(table_name) {
            continue;
        }
        projected += sqlx::query(sqlx::AssertSqlSafe(format!("INSERT INTO pg_temp.historical_artifact_stage(source_table,source_id,record_id,existing_bytes) SELECT $2,(incoming.original_key->>'id')::uuid,r.id,c.byte_count FROM ({}) incoming LEFT JOIN app.historical_records r ON r.source_installation_id=$1 AND r.source_table=$2 AND r.original_key=incoming.original_key LEFT JOIN app.historical_artifact_copies c ON c.record_id=r.id",table.select)))
            .bind(source.source_installation_id.as_uuid()).bind(table_name).execute(&mut **tx).await?.rows_affected();
    }
    let mut selected = 0;
    let mut readable = 0;
    let mut stored = 0;
    if let Some(artifacts) = artifacts {
        if artifacts.source_installation_id != source.source_installation_id
            || artifacts.artifacts.len() > 10_000
        {
            return Err(invalid());
        }
        let mut identities = BTreeSet::new();
        let mut objects = BTreeSet::new();
        for artifact in &artifacts.artifacts {
            let identity = &artifact.identity;
            if identity.kind != HistoricalKindV1::Artifact
                || !artifact_table(&identity.source_table)
                || identity.source_id.is_nil()
                || identity.source_id.is_max()
                || !identities.insert(identity.clone())
            {
                return Err(invalid());
            }
            let row = sqlx::query("SELECT record_id,existing_bytes FROM pg_temp.historical_artifact_stage WHERE source_table=$1 AND source_id=$2")
                .bind(&identity.source_table).bind(identity.source_id).fetch_optional(&mut **tx).await?.ok_or(StoreError::Invalid("historical_artifact_identity"))?;
            let target = db::optional_id(&row, "record_id")?;
            let existing: Option<i64> = row.try_get("existing_bytes")?;
            let copied = artifact.outcome == HistoricalArtifactOutcomeV1::Copied;
            if copied {
                let object = artifact.object_ref.ok_or_else(invalid)?;
                let bytes = artifact.byte_count.ok_or_else(invalid)?;
                if bytes.get() == 0 || bytes.get() > 64 * 1024 * 1024 || !objects.insert(object) {
                    return Err(invalid());
                }
                if existing.is_some_and(|n| n as u64 != bytes.get()) {
                    return Err(StoreError::Conflict);
                }
                if !request.dry_run && target.is_none() {
                    return Err(StoreError::Integrity);
                }
                publish(HistoricalArtifactPublication {
                    export_ref: request.export_ref,
                    source_object: object,
                    target,
                    byte_count: bytes,
                    existing: existing.is_some(),
                    dry_run: request.dry_run,
                })
                .await?;
                readable += 1;
                if !request.dry_run {
                    sqlx::query("INSERT INTO app.historical_artifact_copies(record_id,byte_count,first_import_id) VALUES($1,$2,$3) ON CONFLICT(record_id) DO NOTHING")
                        .bind(target.ok_or(StoreError::Integrity)?.as_uuid()).bind(bytes.get() as i64).bind(report.as_uuid()).execute(&mut **tx).await?;
                    stored += 1;
                }
            } else if artifact.object_ref.is_some() || artifact.byte_count.is_some() {
                return Err(invalid());
            }
            sqlx::query("UPDATE pg_temp.historical_artifact_stage SET source_outcome=$3,verified_readable=$4,stored=$5,byte_count=$6 WHERE source_table=$1 AND source_id=$2")
                .bind(&identity.source_table).bind(identity.source_id).bind(db::code(&artifact.outcome)?).bind(copied).bind(copied && !request.dry_run).bind(artifact.byte_count.map(|n|n.get() as i64)).execute(&mut **tx).await?;
            selected += 1;
        }
    }
    sqlx::query("INSERT INTO app.historical_artifact_results(report_id,source_table,source_id,record_id,source_outcome,verified_readable,stored,byte_count) SELECT $1,source_table,source_id,CASE WHEN $2 THEN NULL ELSE record_id END,source_outcome,verified_readable,stored,byte_count FROM pg_temp.historical_artifact_stage ORDER BY source_table,source_id")
        .bind(report.as_uuid()).bind(request.dry_run).execute(&mut **tx).await?;
    let summary = HistoricalArtifactSummaryV1 {
        schema_version: SchemaV1,
        report_id: report,
        source_records: count(source_records)?,
        projected_records: count(projected)?,
        selected_records: count(selected)?,
        readable_records: count(readable)?,
        stored_records: count(stored)?,
    };
    sqlx::query("INSERT INTO app.historical_artifact_reports(report_id,summary,export_report) VALUES($1,$2,$3)")
        .bind(report.as_uuid()).bind(db::json(&summary)?).bind(artifacts.map(db::json).transpose()?).execute(&mut **tx).await?;
    Ok(source_records != readable)
}

impl Store {
    pub async fn historical_artifact_summary(
        &self,
        actor: &Actor,
        report: Id,
    ) -> Result<HistoricalArtifactSummaryV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        readable_report(&mut tx, actor, report).await?;
        let value: serde_json::Value = sqlx::query_scalar(
            "SELECT summary FROM app.historical_artifact_reports WHERE report_id=$1",
        )
        .bind(report.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let summary = serde_json::from_value(value).map_err(|_| StoreError::Integrity)?;
        tx.commit().await?;
        Ok(summary)
    }
    pub async fn historical_artifact_results(
        &self,
        actor: &Actor,
        report: Id,
        query: &ListQuery,
    ) -> Result<Page<HistoricalArtifactResultV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        readable_report(&mut tx, actor, report).await?;
        let rows = sqlx::query("SELECT * FROM app.historical_artifact_results WHERE report_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(report.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(|r| artifact_result(r, report))
            .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |r| r.id))
    }
    pub async fn historical_artifact(
        &self,
        actor: &Actor,
        report: Id,
        record: Id,
    ) -> Result<HistoricalArtifactResultV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        readable_report(&mut tx, actor, report).await?;
        let row = sqlx::query(
            "SELECT * FROM app.historical_artifact_results WHERE report_id=$1 AND record_id=$2",
        )
        .bind(report.as_uuid())
        .bind(record.as_uuid())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        let result = artifact_result(&row, report)?;
        tx.commit().await?;
        Ok(result)
    }
    /// Only references from this actual import permit access, not dry-run selections.
    pub async fn historical_artifact_content(
        &self,
        actor: &Actor,
        report: Id,
        record: Id,
    ) -> Result<DbCounter, StoreError> {
        let mut tx = self.pool.begin().await?;
        readable_report(&mut tx, actor, report).await?;
        let bytes: i64 = sqlx::query_scalar("SELECT c.byte_count FROM app.historical_artifact_results r JOIN app.historical_artifact_copies c ON c.record_id=r.record_id WHERE r.report_id=$1 AND r.record_id=$2 AND r.stored")
            .bind(report.as_uuid()).bind(record.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        tx.commit().await?;
        count(bytes as u64)
    }
    /// Serialize exact-object recovery with the producer's Operator lock, including
    /// uncertain COMMIT outcomes. Never scan or delete another namespace.
    pub async fn discard_unpublished_historical_artifact<F, Fut>(
        &self,
        record: Id,
        discard: F,
    ) -> Result<bool, StoreError>
    where
        F: FnOnce(Id) -> Fut,
        Fut: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL lock_timeout='5s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "SELECT id FROM app.operator_auth_state WHERE singleton AND initialized FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        let referenced: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM app.historical_artifact_copies WHERE record_id=$1)",
        )
        .bind(record.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if !referenced {
            discard(record).await?;
        }
        tx.commit().await?;
        Ok(!referenced)
    }
}

fn artifact_result(
    r: &sqlx::postgres::PgRow,
    report: Id,
) -> Result<HistoricalArtifactResultV1, StoreError> {
    Ok(HistoricalArtifactResultV1 {
        id: db::id(r.try_get("id")?)?,
        report_id: report,
        identity: HistoricalIdentityV1 {
            kind: HistoricalKindV1::Artifact,
            source_table: r.try_get("source_table")?,
            source_id: r.try_get("source_id")?,
        },
        record_id: db::optional_id(r, "record_id")?,
        source_outcome: r
            .try_get::<Option<String>, _>("source_outcome")?
            .map(|s| {
                serde_json::from_value(serde_json::Value::String(s))
                    .map_err(|_| StoreError::Integrity)
            })
            .transpose()?,
        verified_readable: r.try_get("verified_readable")?,
        stored: r.try_get("stored")?,
        byte_count: r
            .try_get::<Option<i64>, _>("byte_count")?
            .map(|n| count(n as u64))
            .transpose()?,
    })
}
