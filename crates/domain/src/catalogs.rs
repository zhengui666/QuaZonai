//! Structural provenance checks; an ordered timestamp is not proof of historical availability.
use crate::{control::text, research::invalid, DomainError};
use chrono::{DateTime, Utc};
use contracts::{catalogs::*, research::PitStatus, runtime::RuntimeDataKind};
use std::collections::BTreeSet;

fn bad(field: &str) -> DomainError {
    invalid(field, "NATIVE_CATALOG_METADATA_INVALID")
}

pub fn metadata(
    value: &RuntimeCatalogMetadataV1,
    observed_at: DateTime<Utc>,
) -> Result<(), DomainError> {
    for (field, text_value, maximum) in [
        ("registered_ref", value.registered_ref.as_str(), 512),
        (
            "native_snapshot_ref",
            value.native_snapshot_ref.as_str(),
            512,
        ),
        ("provider_kind", value.provider_kind.as_str(), 120),
        (
            "provenance_reference",
            value.provenance_reference.as_str(),
            2000,
        ),
        (
            "availability_provenance",
            value.availability_provenance.as_str(),
            8000,
        ),
    ] {
        text(text_value, 1, maximum, false).map_err(|_| bad(field))?;
    }
    crate::runtime_jobs::storage_version(&value.storage_version)?;
    if value.event_start >= value.event_end
        || value.available_through < value.event_start
        || value.available_through > observed_at + chrono::Duration::seconds(5)
        || value.row_count.get() == 0
        || value.row_count.get() > 1_000_000
        || value.data_kind != RuntimeDataKind::Bar
        || value.quality.native_version != "nautilus-persistence/0.63.0"
        || value.quality.datasets.len() != 1
        || value.quality.checked_at > observed_at + chrono::Duration::seconds(5)
        || value.quality.checked_at < value.available_through
        || (value.pit_status == PitStatus::Verified
            && value.revision_policy != DataRevisionPolicy::AsKnownThen)
    {
        return Err(bad("snapshot"));
    }
    let quality = &value.quality.datasets[0];
    if quality.row_count != value.row_count
        || quality.first_event_ns > quality.last_event_ns
        || quality.last_event_ns > quality.available_through_ns
        || quality.instrument_ids.is_empty()
        || quality.instrument_ids.len() > 256
    {
        return Err(bad("quality"));
    }
    let event_start = value
        .event_start
        .timestamp_nanos_opt()
        .ok_or_else(|| bad("event_start"))?;
    let event_end = value
        .event_end
        .timestamp_nanos_opt()
        .ok_or_else(|| bad("event_end"))?;
    let available = value
        .available_through
        .timestamp_nanos_opt()
        .ok_or_else(|| bad("available_through"))?;
    if event_start < 0
        || quality.first_event_ns.get() < event_start as u64
        || quality.last_event_ns.get() >= event_end as u64
        || quality.available_through_ns.get() > available as u64
    {
        return Err(bad("quality_times"));
    }
    let universe = &value.universe;
    text(&universe.name, 1, 120, false)?;
    text(&universe.calendar_ref, 1, 120, false)?;
    text(&universe.calendar_version, 1, 120, false)?;
    if universe.coverage_start > value.event_start
        || universe.coverage_end < value.event_end
        || universe.coverage_start >= universe.coverage_end
        || universe.selection_asof > value.available_through
        || !(1..=4096).contains(&universe.membership.len())
        || !(1..=256).contains(&universe.instrument_definitions.len())
    {
        return Err(bad("universe"));
    }
    let mut members = BTreeSet::new();
    let mut unique_members = BTreeSet::new();
    for member in &universe.membership {
        text(&member.instrument_id, 1, 200, false)?;
        if member
            .valid_until
            .is_some_and(|until| until <= member.valid_from)
            || member.available_at > value.available_through
            || !unique_members.insert((&member.instrument_id, member.valid_from))
        {
            return Err(bad("universe.membership"));
        }
        members.insert(&member.instrument_id);
    }
    let mut instruments = BTreeSet::new();
    for instrument in &quality.instrument_ids {
        if !members.contains(instrument) || !instruments.insert(instrument) {
            return Err(bad("quality.instrument_ids"));
        }
    }
    if universe
        .instrument_definitions
        .iter()
        .any(|definition| !definition.is_object())
    {
        return Err(bad("universe.instrument_definitions"));
    }
    Ok(())
}
