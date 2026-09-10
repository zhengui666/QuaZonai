//! Explicitly synthetic metadata for contract, authorization and transport tests.
//! This is not a real source, native catalog observation, PIT attestation or qualification.
#![allow(dead_code)]
use chrono::{DateTime, Utc};
use contracts::{
    catalogs::*,
    execution::{NativeDataQualityReportV1, NativeDatasetQualityV1},
    research::{DataOrigin, DataPartition, PitStatus},
    runtime::RuntimeDataKind,
    science::NativeBarSelectionV1,
    DbCounter, Id, SchemaV1,
};

pub fn instant(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap()
}
pub fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
pub fn metadata() -> RuntimeCatalogMetadataV1 {
    let instrument = "EUR/USD.SIM".to_owned();
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec!["EUR/USD.SIM-1-MINUTE-LAST-EXTERNAL".into()],
        event_start_ns: count(60_000_000_000),
        event_end_ns: count(240_000_000_000),
        decision_cutoff_ns: count(300_000_000_000),
        maximum_rows: 3,
    };
    RuntimeCatalogMetadataV1 {
        schema_version: SchemaV1,
        registered_ref: "controlled-catalog".into(),
        native_snapshot_ref: "controlled-snapshot".into(),
        storage_version: "fixture-v1".into(),
        provider_kind: "NAUTILUS_CATALOG".into(),
        data_kind: RuntimeDataKind::Bar,
        partition: DataPartition::Discovery,
        event_start: instant(60),
        event_end: instant(240),
        available_through: instant(300),
        row_count: count(3),
        origin: DataOrigin::Fixture,
        pit_status: PitStatus::Unverified,
        revision_policy: DataRevisionPolicy::AsKnownThen,
        provenance_reference: "Controlled regression fixture; not market evidence".into(),
        availability_provenance: "Synthetic clock values only; no historical PIT claim".into(),
        universe: NativeUniverseV1 {
            name: "Controlled universe".into(),
            calendar_ref: "fixture-calendar".into(),
            calendar_version: "1".into(),
            selection_asof: instant(0),
            has_historical_membership: false,
            coverage_start: instant(0),
            coverage_end: instant(600),
            membership: vec![NativeUniverseMemberV1 {
                instrument_id: instrument.clone(),
                valid_from: instant(0),
                valid_until: None,
                available_at: instant(0),
            }],
            instrument_definitions: vec![serde_json::json!({
                "type": "CurrencyPair", "id": instrument, "fixture_only": true
            })],
        },
        quality: NativeDataQualityReportV1 {
            schema_version: SchemaV1,
            native_version: "nautilus-persistence/0.63.0".into(),
            checked_at: instant(301),
            datasets: vec![NativeDatasetQualityV1 {
                dataset_revision_id: Id::new(),
                selection,
                row_count: count(3),
                instrument_ids: vec![instrument],
                first_event_ns: count(60_000_000_000),
                last_event_ns: count(180_000_000_000),
                available_through_ns: count(220_000_000_000),
            }],
        },
    }
}
