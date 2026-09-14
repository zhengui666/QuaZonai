//! Immutable historical projections, never active qualifications or executable jobs.
use crate::{
    authority::{self, Actor},
    commands, db,
    historical_rows::{self, StagedProjection},
    historical_source, Store, StoreError,
};
use contracts::{
    control::{CommandResult, OperatorOperation},
    imports::*,
    DbCounter, Id, SchemaV1,
};
use sqlx::Connection;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek},
};

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
    pub async fn import_historical_rows<L, F, R>(
        &self,
        actor: &Actor,
        key: &str,
        request: &HistoricalImportRequestV1,
        load: L,
        mut read: F,
    ) -> Result<CommandResult<HistoricalImportReportV1>, StoreError>
    where
        L: FnOnce(Id) -> Result<HistoricalRowExportV1, StoreError>,
        F: FnMut(Id, Id) -> Result<R, StoreError>,
        R: Read + Seek,
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
        for relation in &source.inspection.foreign_keys {
            if relation.orphan_rows != DbCounter::ZERO
                || relation.source_columns.is_empty()
                || relation.source_columns.len() != relation.target_columns.len()
            {
                return Err(invalid());
            }
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
        match actor {
            Actor::Browser { .. } => authority::browser(&mut tx, actor, false, false).await?,
            Actor::Machine { .. } => {
                let machine = authority::machine(&mut tx, actor, false).await?;
                if machine.kind != contracts::control::PrincipalKind::Cli {
                    return Err(StoreError::Forbidden);
                }
                let owns: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.command_receipts WHERE principal_scope=$1 AND operation='MIGRATION_IMPORT' AND resource_id=$2)")
                    .bind(format!("CREDENTIAL:{}", machine.credential_id)).bind(id.as_uuid()).fetch_one(&mut *tx).await?;
                if !owns {
                    return Err(StoreError::NotFound);
                }
            }
        }
        let value: serde_json::Value =
            sqlx::query_scalar("SELECT result FROM app.historical_import_reports WHERE id=$1")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(StoreError::NotFound)?;
        let report = serde_json::from_value(value).map_err(|_| StoreError::Integrity)?;
        tx.commit().await?;
        Ok(report)
    }
}
