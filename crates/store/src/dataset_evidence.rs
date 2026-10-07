//! Owner audit access to Dataset-bound immutable registration evidence only.
//! This route grants no native snapshot access or new research eligibility.
use crate::{Store, StoreError, authority, authority::Actor, data, db};
use contracts::{
    DbCounter, Id, SchemaV1,
    catalogs::RuntimeCatalogMetadataV1,
    data::{DataProviderKind, DatasetEvidenceViewV1, DatasetQualitySummaryV1, DatasetView},
    execution::NativeDataQualityReportV1,
    research::{DataOrigin, DataPartition},
};
use serde::de::DeserializeOwned;
use sqlx::{Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;

async fn document<T, R, Read>(
    tx: &mut Tx<'_>,
    id: Id,
    schema: &str,
    kind: &str,
    maximum: i64,
    origin: DataOrigin,
    reader: &mut R,
) -> Result<T, StoreError>
where
    T: DeserializeOwned,
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    // IDs originate solely from the registered Dataset and its evidence row.
    // No arbitrary artifact IDs, locations, paths, or raw bytes reach the caller.
    let row = sqlx::query("SELECT byte_count FROM app.artifacts WHERE id=$1 AND schema_name=$2 AND kind=$3 AND schema_version='1' AND media_type='application/json' AND project_id IS NULL AND producer_run_id IS NULL AND producer_attempt_id IS NULL AND access_class='OPERATOR' AND created_by='RUNTIME' AND origin=$4 AND storage_backend='LOCAL' AND storage_object_ref=id::text AND storage_version='1' AND byte_count BETWEEN 1 AND $5")
        .bind(id.as_uuid()).bind(schema).bind(kind).bind(db::code(&origin)?).bind(maximum)
        .fetch_optional(&mut **tx).await?.ok_or(StoreError::Integrity)?;
    let size = DbCounter::new(row.try_get::<i64, _>("byte_count")? as u64)
        .map_err(|_| StoreError::Integrity)?;
    let bytes = reader(id, size).await?;
    if bytes.len() as u64 != size.get() {
        return Err(StoreError::Integrity);
    }
    serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)
}

fn matching_dataset(
    native: &RuntimeCatalogMetadataV1,
    dataset: &DatasetView,
) -> Result<(), StoreError> {
    domain::catalogs::metadata(
        native,
        dataset
            .registration_observed_at
            .ok_or(StoreError::Integrity)?,
    )
    .map_err(|_| StoreError::Integrity)?;
    if native.native_snapshot_ref != dataset.native_snapshot_ref
        || native.storage_version != dataset.storage_version
        || native.data_kind != dataset.data_kind
        || native.partition != dataset.partition
        || native.origin != dataset.origin
        || native.pit_status != dataset.pit_status
        || native.revision_policy != dataset.revision_policy
        || native.event_start != dataset.event_start
        || native.event_end != dataset.event_end
        || native.available_through != dataset.available_through
        || native.row_count != dataset.row_count
        || dataset.schema_version != "1"
        || dataset.timezone != "UTC"
    {
        return Err(StoreError::Integrity);
    }
    Ok(())
}

fn quality_summary(
    native: &NativeDataQualityReportV1,
    quality: &NativeDataQualityReportV1,
) -> Result<DatasetQualitySummaryV1, StoreError> {
    // The source-local quality ID predates Dataset registration. Do not
    // rewrite it or pretend it is the newly assigned Dataset revision ID.
    if serde_json::to_value(&quality).map_err(|_| StoreError::Integrity)?
        != serde_json::to_value(native).map_err(|_| StoreError::Integrity)?
    {
        return Err(StoreError::Integrity);
    }
    let measured = quality.datasets.first().ok_or(StoreError::Integrity)?;
    Ok(DatasetQualitySummaryV1 {
        native_version: quality.native_version.clone(),
        checked_at: quality.checked_at,
        row_count: measured.row_count,
        instrument_count: u16::try_from(measured.instrument_ids.len())
            .map_err(|_| StoreError::Integrity)?,
        first_event_ns: measured.first_event_ns,
        last_event_ns: measured.last_event_ns,
        available_through_ns: measured.available_through_ns,
    })
}

impl Store {
    pub async fn get_dataset_evidence<R, Read>(
        &self,
        actor: &Actor,
        id: Id,
        mut reader: R,
    ) -> Result<DatasetEvidenceViewV1, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        // Diagnostic CLI and project-scoped science identities cannot gain
        // evidence access merely because they can read registration labels.
        authority::browser(&mut tx, actor, false).await?;
        let dataset = data::dataset_in_tx(&mut tx, id).await?;
        if dataset.partition == DataPartition::Sealed {
            return Err(StoreError::Invalid("dataset_evidence_sealed"));
        }
        let metadata_id = dataset
            .native_metadata_artifact_id
            .ok_or(StoreError::Invalid("native_dataset_legacy_registration"))?;
        let native: RuntimeCatalogMetadataV1 = document(
            &mut tx,
            metadata_id,
            "qz.native_catalog_metadata",
            "REPORT",
            1048576,
            dataset.origin,
            &mut reader,
        )
        .await?;
        matching_dataset(&native, &dataset)?;
        let source = sqlx::query(
            "SELECT native_catalog_ref,provider_kind FROM app.data_sources WHERE id=$1",
        )
        .bind(dataset.source_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if native.registered_ref != source.try_get::<String, _>("native_catalog_ref")?
            || native.provider_kind != source.try_get::<String, _>("provider_kind")?
            || native.provider_kind != DataProviderKind::NautilusCatalog.code()
        {
            return Err(StoreError::Integrity);
        }
        let universe = data::universe_in_tx(&mut tx, dataset.universe_version_id).await?;
        if native.universe.name != universe.name
            || native.universe.calendar_ref != universe.calendar_ref
            || native.universe.calendar_version != universe.calendar_version
            || native.universe.selection_asof != universe.selection_asof
            || native.universe.has_historical_membership != universe.has_historical_membership
            || native.universe.coverage_start != universe.coverage_start
            || native.universe.coverage_end != universe.coverage_end
        {
            return Err(StoreError::Integrity);
        }
        let quality: NativeDataQualityReportV1 = document(
            &mut tx,
            dataset.quality_artifact_id,
            "qz.data_quality",
            "DATA_QUALITY",
            1049600,
            dataset.origin,
            &mut reader,
        )
        .await?;
        let summary = quality_summary(&native.quality, &quality)?;
        // Native I/O can outlive wall-clock credential expiry. Recheck before
        // returning, and refresh the current license observation without turning
        // a historical audit read into a license extension or new-use approval.
        authority::browser(&mut tx, actor, false).await?;
        let dataset = data::dataset_in_tx(&mut tx, id).await?;
        tx.commit().await?;
        Ok(DatasetEvidenceViewV1 {
            schema_version: SchemaV1,
            dataset_revision_id: dataset.id,
            source_id: dataset.source_id,
            data_use_grant_id: dataset.data_use_grant_id,
            native_metadata_artifact_id: metadata_id,
            quality_artifact_id: dataset.quality_artifact_id,
            registration_observed_at: dataset
                .registration_observed_at
                .ok_or(StoreError::Integrity)?,
            provider_kind: DataProviderKind::NautilusCatalog,
            partition: dataset.partition,
            origin: dataset.origin,
            pit_status: dataset.pit_status,
            revision_policy: dataset.revision_policy,
            source_enabled: dataset.source_enabled,
            runtime_enabled: dataset.runtime_enabled,
            license_state: dataset.license_state,
            checked_at: dataset.checked_at,
            quality: summary,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::catalog_fixture as catalog;
    use super::*;

    fn dataset(native: &RuntimeCatalogMetadataV1) -> DatasetView {
        serde_json::from_value(serde_json::json!({
            "id":Id::new(),"source_id":Id::new(),"data_use_grant_id":Id::new(),
            "native_snapshot_ref":native.native_snapshot_ref,"storage_version":native.storage_version,
            "universe_version_id":Id::new(),"schema_version":"1","data_kind":native.data_kind,
            "partition":native.partition,"event_start":native.event_start,"event_end":native.event_end,
            "available_through":native.available_through,"row_count":native.row_count,"timezone":"UTC",
            "quality_artifact_id":Id::new(),"pit_status":native.pit_status,"revision_policy":native.revision_policy,
            "origin":native.origin,"created_at":catalog::instant(600),"native_metadata_artifact_id":Id::new(),
            "registration_observed_at":catalog::instant(600),"source_enabled":true,"runtime_enabled":true,
            "license_state":"ACTIVE","checked_at":catalog::instant(600)
        })).unwrap()
    }

    #[test]
    fn registered_identity_labels_counts_and_source_document_must_match() {
        let native = catalog::metadata();
        let original = dataset(&native);
        matching_dataset(&native, &original).unwrap();
        for field in [
            "native_snapshot_ref",
            "storage_version",
            "row_count",
            "origin",
            "pit_status",
            "partition",
        ] {
            let mut changed = serde_json::to_value(&original).unwrap();
            changed[field] = match field {
                "row_count" => serde_json::json!("4"),
                "origin" => serde_json::json!("REAL"),
                "pit_status" => serde_json::json!("VERIFIED"),
                "partition" => serde_json::json!("VALIDATION"),
                _ => serde_json::json!("other"),
            };
            assert!(
                matching_dataset(&native, &serde_json::from_value(changed).unwrap()).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn summary_preserves_original_quality_clock_and_excludes_free_text_and_values() {
        let native = catalog::metadata();
        let summary = quality_summary(&native.quality, &native.quality).unwrap();
        assert_eq!(summary.checked_at, native.quality.checked_at);
        assert_eq!(summary.row_count, native.row_count);
        assert_eq!(summary.instrument_count, 1);
        let object = serde_json::to_value(summary).unwrap();
        assert_eq!(object.as_object().unwrap().len(), 7);
        for field in [
            "dataset_revision_id",
            "selection",
            "instrument_ids",
            "settlements",
            "last_bar_notionals",
            "provenance_reference",
        ] {
            assert!(object.get(field).is_none());
        }
    }

    #[test]
    fn foreign_or_changed_quality_cannot_be_summarized_as_registered_evidence() {
        let native = catalog::metadata();
        let mut quality = native.quality.clone();
        quality.datasets[0].dataset_revision_id = Id::new();
        assert!(quality_summary(&native.quality, &quality).is_err());
        let mut quality = native.quality.clone();
        quality.datasets[0].row_count = catalog::count(4);
        assert!(quality_summary(&native.quality, &quality).is_err());
        let mut quality = native.quality.clone();
        quality.datasets.clear();
        assert!(quality_summary(&native.quality, &quality).is_err());
    }
}

#[cfg(test)]
#[path = "../../../tests/support/catalog_metadata.rs"]
mod catalog_fixture;
