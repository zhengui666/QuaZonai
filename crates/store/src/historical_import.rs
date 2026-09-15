//! Immutable historical projections, never active qualifications or executable jobs.
use crate::{
    authority::{self, Actor},
    commands, db,
    historical_rows::{self, StagedProjection},
    historical_source, Store, StoreError,
};
use contracts::{
    control::{CommandResult, ListQuery, OperatorOperation, Page},
    imports::*,
    DbCounter, Id, SchemaV1,
};
use sqlx::{Connection, Postgres, Row, Transaction};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek},
};

mod artifacts;
mod semantics;
pub use artifacts::{HistoricalArtifactPublication, HistoricalImportSource};

fn invalid() -> StoreError {
    StoreError::Invalid("historical_import_source")
}
fn count(n: u64) -> Result<DbCounter, StoreError> {
    DbCounter::new(n).map_err(|_| invalid())
}
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

impl Store {
    /// Both callbacks resolve deployment-registered input by export_ref. They are
    /// deliberately invoked only after authorization and the original replay check.
    pub async fn import_historical_rows<L, F, R, P, Published>(
        &self,
        actor: &Actor,
        key: &str,
        request: &HistoricalImportRequestV1,
        load: L,
        mut read: F,
        mut publish: P,
    ) -> Result<CommandResult<HistoricalImportReportV1>, StoreError>
    where
        L: FnOnce(Id) -> Result<HistoricalImportSource, StoreError>,
        F: FnMut(Id, Id) -> Result<R, StoreError>,
        R: Read + Seek,
        P: FnMut(HistoricalArtifactPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut connection = self.pool.acquire().await?;
        connection.close_on_drop();
        let mut tx = connection.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::MigrationImport,
            key,
            None,
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let source = load(request.export_ref)?;
        let artifact_source = source.artifacts;
        let source = source.rows;
        if source.inspection.source_schema_version != "0029_portfolio_candidate_exposure"
            || source.tables.len() > 256
            || source.inspection.tables.len() != source.tables.len()
            || source.inspection.foreign_keys.len() > 1024
        {
            return Err(invalid());
        }
        let rules = historical_source::rules()?;
        let inspected: BTreeMap<_, _> = source
            .inspection
            .tables
            .iter()
            .map(|t| (&t.table, t))
            .collect();
        if inspected.len() != source.tables.len() {
            return Err(invalid());
        }
        let missing = rules
            .keys()
            .filter(|name| !inspected.contains_key(name))
            .cloned()
            .collect::<Vec<_>>();
        if missing != source.missing_tables {
            return Err(invalid());
        }
        let mut manual = !missing.is_empty();
        let mut seen = BTreeSet::new();
        let mut objects = BTreeSet::new();
        let mut staged = BTreeMap::<String, StagedProjection>::new();
        let mut total_bytes = 0u64;
        for projection in &source.tables {
            if !seen.insert(&projection.table) {
                return Err(invalid());
            }
            let inspection = inspected.get(&projection.table).ok_or_else(invalid)?;
            let rule = rules.get(&projection.table);
            let supported = rule.is_some_and(|r| r.supports(inspection));
            let mut columns = Vec::new();
            let mut exclusions = Vec::new();
            for column in &inspection.columns {
                let exclusion = if supported {
                    rule.and_then(|r| r.columns.get(&column.name))
                        .ok_or_else(invalid)?
                        .2
                } else {
                    Some(HistoricalExclusionReasonV1::UnsupportedSchema)
                };
                if let Some(reason) = exclusion {
                    manual |= matches!(
                        reason,
                        HistoricalExclusionReasonV1::UnreviewedFields
                            | HistoricalExclusionReasonV1::UnsupportedSchema
                            | HistoricalExclusionReasonV1::SealedEvidence
                    );
                    exclusions.push(HistoricalColumnExclusionV1 {
                        column: column.name.clone(),
                        reason,
                    });
                } else {
                    columns.push(column.name.clone());
                }
            }
            if projection.unsupported_schema == supported
                || projection.source_rows != inspection.rows
                || projection.columns != columns
                || projection.excluded_columns != exclusions
            {
                return Err(invalid());
            }
            if columns.is_empty() {
                if projection.object_ref.is_some()
                    || projection.byte_count.is_some()
                    || projection.projected_rows != DbCounter::ZERO
                {
                    return Err(invalid());
                }
                continue;
            }
            let object = projection.object_ref.ok_or_else(invalid)?;
            if !objects.insert(object) {
                return Err(invalid());
            }
            total_bytes = total_bytes
                .checked_add(projection.byte_count.ok_or_else(invalid)?.get())
                .ok_or_else(invalid)?;
            if total_bytes > 8 * 1024 * 1024 * 1024 {
                return Err(invalid());
            }
            let mut input = read(request.export_ref, object)?;
            let table = historical_rows::stage(&mut tx, inspection, projection, &mut input).await?;
            staged.insert(format!("public.{}", projection.table), table);
        }
        let mut checked = 0u64;
        let mut unverified = Vec::new();
        let expected: Vec<HistoricalForeignKeyCheckV1> =
            serde_json::from_str(include_str!("historical_relations_0029.json"))
                .map_err(|_| StoreError::Integrity)?;
        let mut relations = source.inspection.foreign_keys.clone();
        for relation in expected {
            if !inspected.contains_key(
                &relation
                    .source_table
                    .trim_start_matches("public.")
                    .to_owned(),
            ) {
                continue;
            }
            if !relations
                .iter()
                .any(|actual| same_relation(actual, &relation))
            {
                unverified.push(format!("MISSING_DECLARED:{}", relation.constraint));
                relations.push(relation);
            }
        }
        // ponytail: at most 1024 reported plus 0029's fixed baseline relations;
        // native SQL checks each semantic relation once even if constraints repeat.
        let mut checked_relations = Vec::new();
        for relation in &relations {
            if relation.orphan_rows != DbCounter::ZERO
                || relation.source_columns.is_empty()
                || relation.source_columns.len() != relation.target_columns.len()
            {
                return Err(invalid());
            }
            if checked_relations
                .iter()
                .any(|old| same_relation(old, relation))
            {
                continue;
            }
            checked_relations.push(relation.clone());
            let pair = staged
                .get(&relation.source_table)
                .zip(staged.get(&relation.target_table));
            let Some((from, to)) = pair.filter(|(from, to)| {
                relation
                    .source_columns
                    .iter()
                    .all(|c| from.columns.contains(c))
                    && relation
                        .target_columns
                        .iter()
                        .all(|c| to.columns.contains(c))
            }) else {
                unverified.push(format!("{}:{}", relation.source_table, relation.constraint));
                continue;
            };
            let nonnull = relation
                .source_columns
                .iter()
                .map(|c| format!("s.{} IS NOT NULL", quote(c)))
                .collect::<Vec<_>>()
                .join(" AND ");
            let equal = relation
                .source_columns
                .iter()
                .zip(&relation.target_columns)
                .map(|(a, b)| format!("s.{}=t.{}", quote(a), quote(b)))
                .collect::<Vec<_>>()
                .join(" AND ");
            let missing = format!(
                "({nonnull}) AND NOT EXISTS(SELECT 1 FROM pg_temp.{} t WHERE {equal})",
                to.table
            );
            let predicate = match relation.match_type {
                HistoricalForeignKeyMatchV1::Simple => missing,
                HistoricalForeignKeyMatchV1::Full => format!(
                    "({missing}) OR (({}) AND ({}))",
                    relation
                        .source_columns
                        .iter()
                        .map(|c| format!("s.{} IS NULL", quote(c)))
                        .collect::<Vec<_>>()
                        .join(" OR "),
                    relation
                        .source_columns
                        .iter()
                        .map(|c| format!("s.{} IS NOT NULL", quote(c)))
                        .collect::<Vec<_>>()
                        .join(" OR ")
                ),
            };
            let broken: bool = sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM pg_temp.{} s WHERE {predicate})",
                from.table
            ))
            .fetch_one(&mut *tx)
            .await?;
            if broken {
                return Err(StoreError::Invalid("historical_import_relationship"));
            }
            checked += 1;
        }
        checked += semantics::check(&mut tx, &staged, &mut unverified).await?;
        manual |= !unverified.is_empty();
        let mut projected = 0u64;
        let mut inserted = 0u64;
        let mut existing = 0u64;
        for (qualified, table) in &staged {
            let source_table = qualified.strip_prefix("public.").ok_or_else(invalid)?;
            projected += table.rows.get();
            let changed:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM ({}) incoming JOIN app.historical_records old ON old.source_installation_id=$1 AND old.source_table=$2 AND old.original_key=incoming.original_key WHERE old.fields<>incoming.fields)",table.select))
                .bind(source.source_installation_id.as_uuid()).bind(source_table).fetch_one(&mut *tx).await?;
            if changed {
                return Err(StoreError::Conflict);
            }
            let prior:i64=sqlx::query_scalar(&format!("SELECT count(*) FROM ({}) incoming JOIN app.historical_records old ON old.source_installation_id=$1 AND old.source_table=$2 AND old.original_key=incoming.original_key",table.select))
                .bind(source.source_installation_id.as_uuid()).bind(source_table).fetch_one(&mut *tx).await?;
            existing += u64::try_from(prior).map_err(|_| invalid())?;
            if !request.dry_run {
                let disposition = if matches!(
                    source_table,
                    "alpha_models" | "alpha_model_versions" | "alpha_qualifications"
                ) {
                    "LEGACY_REVALIDATION_REQUIRED"
                } else {
                    "READ_ONLY_HISTORY"
                };
                inserted+=sqlx::query(&format!("INSERT INTO app.historical_records(source_installation_id,source_table,original_key,fields,first_import_id,disposition) SELECT $1,$2,incoming.original_key,incoming.fields,$3,$4 FROM ({}) incoming ON CONFLICT(source_installation_id,source_table,original_key) DO NOTHING",table.select))
                    .bind(source.source_installation_id.as_uuid()).bind(source_table).bind(prepared.target.as_uuid()).bind(disposition).execute(&mut *tx).await?.rows_affected();
                sqlx::query(&format!("INSERT INTO app.historical_import_members(report_id,record_id) SELECT $3,old.id FROM ({}) incoming JOIN app.historical_records old ON old.source_installation_id=$1 AND old.source_table=$2 AND old.original_key=incoming.original_key",table.select))
                    .bind(source.source_installation_id.as_uuid()).bind(source_table).bind(prepared.target.as_uuid()).execute(&mut *tx).await?;
            }
        }
        if !request.dry_run && inserted.checked_add(existing) != Some(projected) {
            return Err(StoreError::Conflict);
        }
        manual |= artifacts::publish(
            &mut tx,
            &staged,
            &source,
            artifact_source.as_ref(),
            prepared.target,
            request,
            &mut publish,
        )
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let report = HistoricalImportReportV1 {
            schema_version: SchemaV1,
            id: prepared.target,
            export_ref: request.export_ref,
            source_installation_id: source.source_installation_id,
            dry_run: request.dry_run,
            projected_rows: count(projected)?,
            new_rows: count(inserted)?,
            existing_rows: count(existing)?,
            checked_relationships: count(checked)?,
            unverified_relationships: unverified,
            manual_review_required: manual,
        };
        sqlx::query("INSERT INTO app.historical_import_reports(id,export_ref,source_installation_id,dry_run,source_report,result) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(report.id.as_uuid()).bind(request.export_ref.as_uuid()).bind(source.source_installation_id.as_uuid()).bind(request.dry_run).bind(db::json(&source)?).bind(db::json(&report)?).execute(&mut *tx).await?;
        let result = commands::finish(&mut tx, prepared, report, 202).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn historical_import_report(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<HistoricalImportReportV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let value = readable_report(&mut tx, actor, id).await?;
        let report = serde_json::from_value(value).map_err(|_| StoreError::Integrity)?;
        tx.commit().await?;
        Ok(report)
    }

    pub async fn historical_import_reports(
        &self,
        actor: &Actor,
        query: &ListQuery,
    ) -> Result<Page<HistoricalImportReportV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let scope = read_scope(&mut tx, actor).await?;
        let rows: Vec<serde_json::Value> = sqlx::query_scalar("SELECT r.result FROM app.historical_import_reports r WHERE ($1::text IS NULL OR EXISTS(SELECT 1 FROM app.command_receipts c WHERE c.principal_scope=$1 AND c.operation='MIGRATION_IMPORT' AND c.resource_id=r.id)) AND ($2::uuid IS NULL OR r.id<$2) ORDER BY r.id DESC LIMIT $3")
            .bind(scope).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(|_| StoreError::Integrity))
            .collect::<Result<Vec<HistoricalImportReportV1>, _>>()?;
        let result = crate::control::page(items, query.limit, |r| r.id);
        tx.commit().await?;
        Ok(result)
    }

    pub async fn historical_import_source(
        &self,
        actor: &Actor,
        id: Id,
    ) -> Result<HistoricalRowExportV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        readable_report(&mut tx, actor, id).await?;
        let value: serde_json::Value = sqlx::query_scalar(
            "SELECT source_report FROM app.historical_import_reports WHERE id=$1",
        )
        .bind(id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let result = serde_json::from_value(value).map_err(|_| StoreError::Integrity)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn historical_import_mappings(
        &self,
        actor: &Actor,
        id: Id,
        query: &ListQuery,
    ) -> Result<Page<HistoricalMappingViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        readable_report(&mut tx, actor, id).await?;
        let rows = sqlx::query("SELECT r.id,r.source_installation_id,r.source_table,r.original_key,r.first_import_id,r.disposition,r.created_at FROM app.historical_import_members m JOIN app.historical_records r ON r.id=m.record_id WHERE m.report_id=$1 AND ($2::uuid IS NULL OR r.id<$2) ORDER BY r.id DESC LIMIT $3")
            .bind(id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(|row| {
                Ok(HistoricalMappingViewV1 {
                    id: db::id(row.try_get("id")?)?,
                    key: HistoricalOriginalKeyV1 {
                        source_installation_id: db::id(row.try_get("source_installation_id")?)?,
                        source_table: row.try_get("source_table")?,
                        values: serde_json::from_value(row.try_get("original_key")?)
                            .map_err(|_| StoreError::Integrity)?,
                    },
                    first_import_id: db::id(row.try_get("first_import_id")?)?,
                    disposition: db::enum_value(row, "disposition")?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        let result = crate::control::page(items, query.limit, |r| r.id);
        tx.commit().await?;
        Ok(result)
    }

    pub async fn historical_record_fields(
        &self,
        actor: &Actor,
        report: Id,
        record: Id,
    ) -> Result<HistoricalRecordFieldsV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        readable_record(&mut tx, actor, report, record).await?;
        let rows = sqlx::query("SELECT v.key AS name,char_length(v.value)::bigint AS characters FROM app.historical_records r CROSS JOIN LATERAL jsonb_each_text(r.fields) v WHERE r.id=$1 ORDER BY v.key")
            .bind(record.as_uuid()).fetch_all(&mut *tx).await?;
        let fields = rows
            .iter()
            .map(|row| {
                Ok(HistoricalFieldSummaryV1 {
                    name: row.try_get("name")?,
                    character_count: row
                        .try_get::<Option<i64>, _>("characters")?
                        .map(|v| count(v.try_into().map_err(|_| StoreError::Integrity)?))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await?;
        Ok(HistoricalRecordFieldsV1 {
            schema_version: SchemaV1,
            report_id: report,
            record_id: record,
            fields,
        })
    }

    pub async fn historical_record_field(
        &self,
        actor: &Actor,
        report: Id,
        record: Id,
        query: &HistoricalFieldQueryV1,
    ) -> Result<HistoricalFieldContentV1, StoreError> {
        if query.name.is_empty() || query.name.len() > 63 {
            return Err(StoreError::Invalid("historical_field_name"));
        }
        let mut tx = self.pool.begin().await?;
        readable_record(&mut tx, actor, report, record).await?;
        let length: Option<i64> = sqlx::query_scalar("SELECT char_length(fields->>$2)::bigint FROM app.historical_records WHERE id=$1 AND fields ? $2")
            .bind(record.as_uuid()).bind(&query.name).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let total = length
            .map(|v| count(v.try_into().map_err(|_| StoreError::Integrity)?))
            .transpose()?;
        if query.offset.get() > total.unwrap_or(DbCounter::ZERO).get() {
            return Err(StoreError::Invalid("historical_field_offset"));
        }
        let start = i32::try_from(query.offset.get() + 1)
            .map_err(|_| StoreError::Invalid("historical_field_offset"))?;
        // Native Unicode substring keeps the application response bounded. PostgreSQL
        // may detoast a large field per page; no copy of the entire value crosses SQLx.
        let text: Option<String> = sqlx::query_scalar(
            "SELECT substring(fields->>$2 FROM $3 FOR $4) FROM app.historical_records WHERE id=$1",
        )
        .bind(record.as_uuid())
        .bind(&query.name)
        .bind(start)
        .bind(HISTORICAL_FIELD_CHARS as i32)
        .fetch_one(&mut *tx)
        .await?;
        let end = query.offset.get()
            + text
                .as_deref()
                .map_or(0, |value| value.chars().count() as u64);
        let next = total
            .filter(|v| end < v.get())
            .map(|_| count(end))
            .transpose()?;
        tx.commit().await?;
        Ok(HistoricalFieldContentV1 {
            schema_version: SchemaV1,
            report_id: report,
            record_id: record,
            name: query.name.clone(),
            offset: query.offset,
            total_characters: total,
            text,
            next_offset: next,
        })
    }
}

async fn read_scope(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
) -> Result<Option<String>, StoreError> {
    match actor {
        Actor::Browser { .. } => {
            authority::browser(tx, actor, false, false).await?;
            Ok(None)
        }
        Actor::Machine { .. } => {
            let machine = authority::machine(tx, actor, false).await?;
            if machine.kind != contracts::control::PrincipalKind::Cli {
                return Err(StoreError::Forbidden);
            }
            Ok(Some(format!("CREDENTIAL:{}", machine.credential_id)))
        }
    }
}
pub(super) async fn readable_report(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Id,
) -> Result<serde_json::Value, StoreError> {
    let scope = read_scope(tx, actor).await?;
    sqlx::query_scalar("SELECT r.result FROM app.historical_import_reports r WHERE r.id=$1 AND ($2::text IS NULL OR EXISTS(SELECT 1 FROM app.command_receipts c WHERE c.principal_scope=$2 AND c.operation='MIGRATION_IMPORT' AND c.resource_id=r.id))")
        .bind(id.as_uuid()).bind(scope).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)
}

async fn readable_record(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    report: Id,
    record: Id,
) -> Result<(), StoreError> {
    readable_report(tx, actor, report).await?;
    let member: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.historical_import_members WHERE report_id=$1 AND record_id=$2)")
        .bind(report.as_uuid()).bind(record.as_uuid()).fetch_one(&mut **tx).await?;
    if !member {
        return Err(StoreError::NotFound);
    }
    sqlx::query("SET LOCAL statement_timeout='30s'")
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn same_relation(a: &HistoricalForeignKeyCheckV1, b: &HistoricalForeignKeyCheckV1) -> bool {
    a.source_table == b.source_table
        && a.target_table == b.target_table
        && a.source_columns == b.source_columns
        && a.target_columns == b.target_columns
        && a.match_type == b.match_type
}
