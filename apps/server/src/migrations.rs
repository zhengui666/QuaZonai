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
    collections::BTreeMap,
    io::{Seek, SeekFrom},
    path::PathBuf,
};
use store::StoreError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportRegistration {
    pub export_ref: Id,
    pub directory: PathBuf,
}
struct RegisteredExport {
    report: HistoricalRowExportV1,
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
            result.entries.insert(
                registration.export_ref,
                RegisteredExport { report, objects },
            );
        }
        Ok(result)
    }
    fn report(&self, reference: Id) -> Result<HistoricalRowExportV1, StoreError> {
        Ok(self
            .entries
            .get(&reference)
            .ok_or(StoreError::NotFound)?
            .report
            .clone())
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
    let key = idempotency_key(&headers)?;
    let _slot = state
        .historical_import_slots
        .try_acquire()
        .map_err(|_| ApiError::internal())?;
    let exports = &state.historical_exports;
    let result = state
        .store
        .import_historical_rows(
            &actor,
            key,
            &request,
            |reference| exports.report(reference),
            |reference, object| exports.object(reference, object),
        )
        .await?;
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
