//! Deployment-frozen exports and the shared Operator import transaction.
use crate::{
    access::{idempotency_key, Authority},
    auth::json,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    Json,
};
use contracts::{
    control::{CommandResult, ListQuery, Page},
    imports::*,
    Id,
};
use integrations::mission_files::{FrozenFile, MissionFiles};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};
use store::StoreError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRegistration {
    pub export_ref: Id,
    pub directory: PathBuf,
    pub artifact_directory: Option<PathBuf>,
}
struct RegisteredExport {
    report: HistoricalRowExportV1,
    artifacts: Option<HistoricalArtifactExportV1>,
    objects: BTreeMap<Id, FrozenFile>,
}
#[derive(Default)]
pub struct HistoricalExports {
    entries: BTreeMap<Id, RegisteredExport>,
}
impl HistoricalExports {
    /// Only trusted deployment configuration calls this, never an HTTP request.
    /// Linux native sealed snapshots keep request-time I/O independent of paths.
    pub fn load(registrations: Vec<ExportRegistration>) -> Result<Self, &'static str> {
        let invalid = || "historical export registration is invalid or unavailable";
        if registrations.len() > 32 {
            return Err(invalid());
        }
        let mut result = Self::default();
        let mut total = 0u64;
        for registration in registrations {
            if result.entries.contains_key(&registration.export_ref) {
                return Err(invalid());
            }
            let files = MissionFiles::open(&registration.directory).map_err(|_| invalid())?;
            let bytes = files
                .read_bytes("report.json", 4 * 1024 * 1024)
                .map_err(|_| invalid())?;
            let report: HistoricalRowExportV1 =
                serde_json::from_slice(&bytes).map_err(|_| invalid())?;
            if report.tables.len() > 256 || report.inspection.tables.len() != report.tables.len() {
                return Err(invalid());
            }
            let mut objects = BTreeMap::new();
            for table in &report.tables {
                if let Some(reference) = table.object_ref {
                    let size = table.byte_count.ok_or_else(invalid)?.get();
                    total = total.checked_add(size).ok_or_else(invalid)?;
                    // ponytail: startup snapshots use at most 8 GiB of native backing;
                    // larger migrations need explicitly partitioned export batches.
                    if total > 8 * 1024 * 1024 * 1024 || objects.contains_key(&reference) {
                        return Err(invalid());
                    }
                    let mut snapshot = files
                        .snapshot(&format!("{reference}.csv"), size)
                        .map_err(|_| invalid())?;
                    if snapshot.seek(SeekFrom::End(0)).map_err(|_| invalid())? != size {
                        return Err(invalid());
                    }
                    snapshot.rewind().map_err(|_| invalid())?;
                    objects.insert(reference, snapshot);
                } else if table.byte_count.is_some() {
                    return Err(invalid());
                }
            }
            let artifacts = if let Some(directory) = registration.artifact_directory {
                let files = MissionFiles::open(&directory).map_err(|_| invalid())?;
                let bytes = files
                    .read_bytes("report.json", 4 * 1024 * 1024)
                    .map_err(|_| invalid())?;
                let artifacts: HistoricalArtifactExportV1 =
                    serde_json::from_slice(&bytes).map_err(|_| invalid())?;
                if artifacts.source_installation_id != report.source_installation_id
                    || artifacts.artifacts.len() > 10_000
                {
                    return Err(invalid());
                }
                let mut identities = BTreeSet::new();
                for item in &artifacts.artifacts {
                    if item.identity.kind != HistoricalKindV1::Artifact
                        || !matches!(
                            item.identity.source_table.as_str(),
                            "mission_artifacts" | "alpha_signal_artifacts"
                        )
                        || item.identity.source_id.is_nil()
                        || item.identity.source_id.is_max()
                        || !identities.insert(item.identity.clone())
                    {
                        return Err(invalid());
                    }
                    if item.outcome != HistoricalArtifactOutcomeV1::Copied {
                        if item.object_ref.is_some() || item.byte_count.is_some() {
                            return Err(invalid());
                        }
                        continue;
                    }
                    let reference = item.object_ref.ok_or_else(invalid)?;
                    let size = item.byte_count.ok_or_else(invalid)?.get();
                    total = total.checked_add(size).ok_or_else(invalid)?;
                    if size == 0
                        || size > 64 * 1024 * 1024
                        || total > 8 * 1024 * 1024 * 1024
                        || objects.contains_key(&reference)
                    {
                        return Err(invalid());
                    }
                    let mut snapshot = files
                        .snapshot(&format!("objects/{reference}"), size)
                        .map_err(|_| invalid())?;
                    if snapshot.seek(SeekFrom::End(0)).map_err(|_| invalid())? != size {
                        return Err(invalid());
                    }
                    snapshot.rewind().map_err(|_| invalid())?;
                    objects.insert(reference, snapshot);
                }
                Some(artifacts)
            } else {
                None
            };
            result.entries.insert(
                registration.export_ref,
                RegisteredExport {
                    report,
                    artifacts,
                    objects,
                },
            );
        }
        Ok(result)
    }
    fn report(&self, reference: Id) -> Result<store::HistoricalImportSource, StoreError> {
        let entry = self.entries.get(&reference).ok_or(StoreError::NotFound)?;
        Ok(store::HistoricalImportSource {
            rows: entry.report.clone(),
            artifacts: entry.artifacts.clone(),
        })
    }
    fn object(&self, reference: Id, object: Id) -> Result<FrozenFile, StoreError> {
        self.entries
            .get(&reference)
            .and_then(|e| e.objects.get(&object))
            .cloned()
            .ok_or(StoreError::NotFound)
    }
}

#[utoipa::path(post,path="/api/v2/migrations/import",operation_id="import_historical_rows",tag="Historical migration",params(("Idempotency-Key"=String,Header)),request_body=HistoricalImportRequestV1,responses((status=202,body=CommandResult<HistoricalImportReportV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn import(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<HistoricalImportRequestV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<HistoricalImportReportV1>>), ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let slot = state
        .historical_import_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError::internal())?;
    // Keep the native I/O and DB transaction alive together if the HTTP waiter leaves.
    let result = tokio::spawn(async move {
        let _slot = slot;
        let exports = &state.historical_exports;
        let mut attempted = Vec::new();
        let result = state
            .store
            .import_historical_rows(
                &actor,
                &key,
                &request,
                |reference| exports.report(reference),
                |reference, object| exports.object(reference, object),
                |publication| {
                    if !publication.existing && !publication.dry_run {
                        if let Some(id) = publication.target {
                            attempted.push(id);
                        }
                    }
                    let exports = state.historical_exports.clone();
                    let objects = state.historical_artifact_store.clone();
                    async move {
                        tokio::task::spawn_blocking(move || {
                            publish_copy(&exports, objects.as_deref(), publication)
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                    }
                },
            )
            .await;
        if result.is_err() {
            if let Some(objects) = &state.historical_artifact_store {
                for id in attempted {
                    let objects = objects.clone();
                    // Failed or uncertain cleanup retains the object for reconciliation.
                    let _ = state
                        .store
                        .discard_unpublished_historical_artifact(id, |id| async move {
                            tokio::task::spawn_blocking(move || {
                                objects
                                    .discard_unpublished(id)
                                    .map_err(|_| StoreError::Integrity)
                            })
                            .await
                            .map_err(|_| StoreError::Integrity)?
                        })
                        .await;
                }
            }
        }
        result
    })
    .await
    .map_err(|_| ApiError::internal())??;
    Ok((StatusCode::ACCEPTED, Json(result)))
}
#[utoipa::path(get,path="/api/v2/migrations/reports/{id}",operation_id="get_historical_import_report",tag="Historical migration",params(("id"=Id,Path)),responses((status=200,body=HistoricalImportReportV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn report(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<HistoricalImportReportV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.historical_import_report(&actor, id).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/migrations/reports",operation_id="list_historical_import_reports",tag="Historical migration",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<HistoricalImportReportV1>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem)))]
pub async fn reports(
    State(state): State<AppState>,
    Authority(actor): Authority,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<HistoricalImportReportV1>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .historical_import_reports(&actor, &query)
            .await?,
    ))
}
#[utoipa::path(get,path="/api/v2/migrations/reports/{id}/source",operation_id="get_historical_import_source",tag="Historical migration",params(("id"=Id,Path)),responses((status=200,body=HistoricalRowExportV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn source(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<HistoricalRowExportV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.historical_import_source(&actor, id).await?,
    ))
}
#[utoipa::path(get,path="/api/v2/migrations/reports/{id}/mappings",operation_id="list_historical_import_mappings",tag="Historical migration",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<HistoricalMappingViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn mappings(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<HistoricalMappingViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .historical_import_mappings(&actor, id, &query)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/migrations/reports/{id}/records/{record}/fields",operation_id="list_historical_record_fields",tag="Historical migration",params(("id"=Id,Path),("record"=Id,Path)),responses((status=200,body=HistoricalRecordFieldsV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn fields(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<(Id, Id)>, PathRejection>,
) -> Result<Json<HistoricalRecordFieldsV1>, ApiError> {
    let Path((id, record)) = path.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .historical_record_fields(&actor, id, record)
            .await?,
    ))
}
#[utoipa::path(get,path="/api/v2/migrations/reports/{id}/records/{record}/field",operation_id="get_historical_record_field",tag="Historical migration",params(("id"=Id,Path),("record"=Id,Path),("name"=String,Query,min_length=1,max_length=63),("offset"=Option<contracts::DbCounter>,Query)),responses((status=200,body=HistoricalFieldContentV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn field(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<(Id, Id)>, PathRejection>,
    query: Result<Query<HistoricalFieldQueryV1>, QueryRejection>,
) -> Result<Json<HistoricalFieldContentV1>, ApiError> {
    let Path((id, record)) = path.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .historical_record_field(&actor, id, record, &query)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/migrations/reports/{id}/artifacts/summary",operation_id="get_historical_artifact_summary",tag="Historical migration",params(("id"=Id,Path)),responses((status=200,body=HistoricalArtifactSummaryV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn artifact_summary(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<HistoricalArtifactSummaryV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.historical_artifact_summary(&actor, id).await?,
    ))
}
#[utoipa::path(get,path="/api/v2/migrations/reports/{id}/artifacts",operation_id="list_historical_artifact_results",tag="Historical migration",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<HistoricalArtifactResultV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn artifact_results(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<HistoricalArtifactResultV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .historical_artifact_results(&actor, id, &query)
            .await?,
    ))
}
#[utoipa::path(get,path="/api/v2/migrations/reports/{id}/artifacts/{record}",operation_id="get_historical_artifact",tag="Historical migration",params(("id"=Id,Path),("record"=Id,Path)),responses((status=200,body=HistoricalArtifactResultV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn artifact(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<(Id, Id)>, PathRejection>,
) -> Result<Json<HistoricalArtifactResultV1>, ApiError> {
    let Path((id, record)) = path.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.historical_artifact(&actor, id, record).await?,
    ))
}
#[utoipa::path(get,path="/api/v2/migrations/reports/{id}/artifacts/{record}/content",operation_id="get_historical_artifact_content",tag="Historical migration",params(("id"=Id,Path),("record"=Id,Path)),responses((status=200,body=inline(crate::artifacts::ArtifactBytes),content_type="application/octet-stream"),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn artifact_content(
    State(state): State<AppState>,
    Authority(actor): Authority,
    capacity: crate::artifacts::ArtifactCapacity,
    path: Result<Path<(Id, Id)>, PathRejection>,
) -> Result<axum::response::Response, ApiError> {
    let Path((id, record)) = path.map_err(|_| ApiError::validation())?;
    let bytes = state
        .store
        .historical_artifact_content(&actor, id, record)
        .await?;
    let objects = state
        .historical_artifact_store
        .ok_or_else(ApiError::internal)?;
    crate::artifacts::native_content(objects, record, bytes, capacity).await
}

fn publish_copy(
    exports: &HistoricalExports,
    objects: Option<&integrations::artifacts::ArtifactStore>,
    publication: store::HistoricalArtifactPublication,
) -> Result<(), StoreError> {
    let mut source = exports.object(publication.export_ref, publication.source_object)?;
    let mut bytes = Vec::new();
    source
        .by_ref()
        .take(publication.byte_count.get() + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StoreError::Integrity)?;
    if bytes.len() as u64 != publication.byte_count.get() {
        return Err(StoreError::Integrity);
    }
    if publication.dry_run && !publication.existing {
        return Ok(());
    }
    let objects = objects.ok_or(StoreError::Integrity)?;
    let target = publication.target.ok_or(StoreError::Integrity)?;
    if !publication.dry_run && !publication.existing {
        match objects.put(target, &bytes) {
            Ok(()) => (),
            Err(integrations::artifacts::ArtifactError::Io(e))
                if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(crate::error::artifact_storage(error)),
        }
    }
    let original = objects
        .read(target, publication.byte_count)
        .map_err(|_| StoreError::Integrity)?;
    if original != bytes {
        return Err(StoreError::Conflict);
    }
    Ok(())
}

#[cfg(test)]
mod artifact_tests {
    use super::*;
    use contracts::DbCounter;
    use integrations::artifacts::ArtifactStore;
    use std::fs;

    #[test]
    fn frozen_binary_publication_compares_original_bytes_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        let bytes = b"\xff\0original";
        fs::write(source.join("one"), bytes).unwrap();
        fs::write(source.join("two"), b"\xfe\0original").unwrap();
        let files = MissionFiles::open(&source).unwrap();
        let reference = Id::new();
        let first = Id::new();
        let second = Id::new();
        // No row parsing here: the native SQL importer independently proves identity.
        let report: HistoricalRowExportV1 = serde_json::from_value(serde_json::json!({
            "schema_version":1,"source_installation_id":Id::new(),"missing_tables":[],
            "inspection":{"schema_version":1,"source_schema_version":"0029_portfolio_candidate_exposure","inspected_at":"2020-01-01T00:00:00Z","tables":[],"foreign_keys":[]},"tables":[]
        })).unwrap();
        let exports = HistoricalExports {
            entries: BTreeMap::from([(
                reference,
                RegisteredExport {
                    report,
                    artifacts: None,
                    objects: BTreeMap::from([
                        (first, files.snapshot("one", bytes.len() as u64).unwrap()),
                        (second, files.snapshot("two", bytes.len() as u64).unwrap()),
                    ]),
                },
            )]),
        };
        fs::write(source.join("one"), "replaced").unwrap();
        let target = Id::new();
        let count = DbCounter::new(bytes.len() as u64).unwrap();
        let publication = |source_object, existing, dry_run| store::HistoricalArtifactPublication {
            export_ref: reference,
            source_object,
            target: Some(target),
            byte_count: count,
            existing,
            dry_run,
        };
        assert!(publish_copy(&exports, None, publication(first, false, true)).is_ok());
        let native = ArtifactStore::open(&root.path().join("historical-artifacts")).unwrap();
        publish_copy(&exports, Some(&native), publication(first, false, false)).unwrap();
        publish_copy(&exports, Some(&native), publication(first, true, false)).unwrap();
        publish_copy(&exports, Some(&native), publication(first, false, false)).unwrap();
        assert!(matches!(
            publish_copy(&exports, Some(&native), publication(second, true, false)),
            Err(StoreError::Conflict)
        ));
        assert!(matches!(
            publish_copy(&exports, Some(&native), publication(second, false, false)),
            Err(StoreError::Conflict)
        ));
        assert_eq!(native.read(target, count).unwrap(), bytes);
        assert_eq!(
            fs::read_dir(root.path().join("historical-artifacts"))
                .unwrap()
                .count(),
            1
        );
    }
}
