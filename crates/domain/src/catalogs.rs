//! Structural provenance checks; an ordered timestamp is not proof of historical availability.
use crate::{control::text, research::invalid, DomainError};
use chrono::{DateTime, Utc};
use contracts::{catalogs::*, research::PitStatus, runtime::RuntimeDataKind};
use std::collections::BTreeSet;

fn bad(field: &str) -> DomainError {
    invalid(field, "NATIVE_CATALOG_METADATA_INVALID")
}

/// Inspect the original externally tagged Rust InstrumentAny payload, without rewriting it.
pub fn instrument_definition(
    value: &serde_json::Value,
) -> Result<(&str, &serde_json::Value), DomainError> {
    let object = value
        .as_object()
        .filter(|v| v.len() == 1)
        .ok_or_else(|| bad("instrument_definition"))?;
    let (class, payload) = object
        .iter()
        .next()
        .ok_or_else(|| bad("instrument_definition"))?;
    text(class, 1, 120, false)?;
    let id = payload
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| bad("instrument_definition.id"))?;
    text(id, 1, 200, false)?;
    Ok((class, payload))
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
    if value.partition == contracts::research::DataPartition::Sealed
        && quality.last_bar_notionals.is_some()
    {
        return Err(bad("quality.sealed_bar_values"));
    }
    bar_notionals(quality)?;
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
        if let Some(groups) = &member.groups {
            let mut unique = BTreeSet::new();
            if groups.len() > contracts::portfolio::MAX_ALLOCATION_GROUPS {
                return Err(bad("universe.groups"));
            }
            for group in groups {
                text(group, 1, 120, false)?;
                if !unique.insert(group) {
                    return Err(bad("universe.groups"));
                }
            }
        }
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
    let mut definitions = BTreeSet::new();
    for definition in &universe.instrument_definitions {
        let (_, payload) = instrument_definition(definition)?;
        let id = payload["id"]
            .as_str()
            .ok_or_else(|| bad("instrument_definition.id"))?;
        if !definitions.insert(id) {
            return Err(bad("universe.instrument_definitions"));
        }
    }
    if quality
        .instrument_ids
        .iter()
        .any(|id| !definitions.contains(id.as_str()))
    {
        return Err(bad("universe.instrument_definitions"));
    }
    Ok(())
}

/// Validate measured last-bar facts without inventing unobserved market capacity.
pub fn bar_notionals(
    quality: &contracts::execution::NativeDatasetQualityV1,
) -> Result<(), DomainError> {
    let Some(values) = &quality.last_bar_notionals else {
        return Ok(());
    };
    if values.is_empty() || values.len() != quality.instrument_ids.len() || values.len() > 256 {
        return Err(bad("quality.last_bar_notionals"));
    }
    for (value, instrument) in values.iter().zip(&quality.instrument_ids) {
        text(&value.currency, 1, 16, false)?;
        if &value.instrument_id != instrument
            || value.event_ns < quality.first_event_ns
            || value.event_ns > quality.last_event_ns
            || value.event_ns < quality.selection.event_start_ns
            || value.event_ns >= quality.selection.event_end_ns
            || value.available_ns < value.event_ns
            || value.available_ns > quality.available_through_ns
            || value.available_ns > quality.selection.decision_cutoff_ns
            || !value.close_price.is_positive()
            || !value.traded_volume.is_nonnegative()
            || !value.notional_value.is_nonnegative()
            || (!value.traded_volume.is_positive() && value.notional_value.is_positive())
        {
            return Err(bad("quality.last_bar_notionals"));
        }
    }
    if values.iter().map(|v| v.event_ns).max() != Some(quality.last_event_ns)
        || values.iter().map(|v| v.available_ns).max() != Some(quality.available_through_ns)
    {
        return Err(bad("quality.last_bar_notionals"));
    }
    Ok(())
}

/// Resolve only requested grouping, from original membership known at the decision.
pub fn portfolio_groups(
    universe: &NativeUniverseV1,
    instruments: &[String],
    required: &[contracts::portfolio::GroupBoundV1],
    cutoff: contracts::DbCounter,
) -> Result<Vec<Vec<String>>, DomainError> {
    if required.is_empty() {
        return Ok(vec![Vec::new(); instruments.len()]);
    }
    let bad_source = || bad("portfolio.group_source");
    let ns = cutoff.get();
    let at =
        DateTime::<Utc>::from_timestamp((ns / 1_000_000_000) as i64, (ns % 1_000_000_000) as u32)
            .ok_or_else(bad_source)?;
    if at < universe.coverage_start || at > universe.coverage_end || at < universe.selection_asof {
        return Err(bad_source());
    }
    let groups = instruments
        .iter()
        .map(|id| {
            let mut matches = universe.membership.iter().filter(|m| {
                &m.instrument_id == id
                    && m.valid_from <= at
                    && m.valid_until.is_none_or(|until| at < until)
                    && m.available_at <= at
            });
            let member = matches.next().ok_or_else(bad_source)?;
            if matches.next().is_some() {
                return Err(bad_source());
            }
            member.groups.clone().ok_or_else(bad_source)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if required.iter().any(|bound| {
        !groups
            .iter()
            .flatten()
            .any(|group| group == &bound.group_id)
    }) {
        return Err(bad_source());
    }
    Ok(groups)
}
