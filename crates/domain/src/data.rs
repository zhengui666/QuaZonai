//! Data configuration and immutable license/registration semantics, not a market-data loader.
use crate::{control::text, research::invalid, DomainError};
use chrono::{DateTime, Utc};
use contracts::{catalogs::RuntimeCatalogMetadataV1, data::*};

fn bad(field: &str) -> DomainError {
    invalid(field, "DATA_CONTRACT_INVALID")
}

fn time(value: DateTime<Utc>, field: &str) -> Result<(), DomainError> {
    if !value.timestamp_subsec_nanos().is_multiple_of(1000) || value.timestamp_nanos_opt().is_none()
    {
        return Err(bad(field));
    }
    Ok(())
}

pub fn registry_key(value: &str) -> Result<(), DomainError> {
    text(value, 1, 512, false).map_err(|_| bad("native_catalog_ref"))?;
    if value.trim() != value
        || value.contains("://")
        || value.contains(['\\', '?', '#'])
        || value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(bad("native_catalog_ref"));
    }
    Ok(())
}

pub fn source_create(value: &DataSourceCreate) -> Result<(), DomainError> {
    text(&value.name, 1, 120, false).map_err(|_| bad("name"))?;
    registry_key(&value.native_catalog_ref)
}

pub fn source_update(value: &DataSourceUpdate) -> Result<(), DomainError> {
    text(&value.name, 1, 120, false).map_err(|_| bad("name"))
}

pub fn grant_create(value: &DataGrantCreate) -> Result<(), DomainError> {
    text(&value.license_reference, 1, 2000, false).map_err(|_| bad("license_reference"))?;
    time(value.valid_from, "valid_from")?;
    if let Some(until) = value.valid_until {
        time(until, "valid_until")?;
        if until <= value.valid_from {
            return Err(bad("valid_until"));
        }
    }
    Ok(())
}

pub fn grant_revoke(value: &DataGrantRevoke) -> Result<(), DomainError> {
    text(&value.reason_code, 1, 120, false).map_err(|_| bad("reason_code"))?;
    text(&value.reason, 1, 2000, true).map_err(|_| bad("reason"))?;
    if let Some(effective) = value.effective_at {
        time(effective, "effective_at")?;
    }
    Ok(())
}

pub fn dataset_register(value: &DatasetRegister) -> Result<(), DomainError> {
    crate::runtime_jobs::storage_version(&value.native_storage_version)
        .map_err(|_| bad("native_storage_version"))
}

pub fn registration_metadata(
    value: &RuntimeCatalogMetadataV1,
    request: &DatasetRegister,
    source: &DataSourceView,
    observed_at: DateTime<Utc>,
) -> Result<(), DomainError> {
    dataset_register(request)?;
    crate::catalogs::metadata(value, observed_at)?;
    if source.id != request.source_id
        || source.revision != request.expected_source_revision
        || value.registered_ref != source.native_catalog_ref
        || value.storage_version != request.native_storage_version
        || value.provider_kind != source.provider_kind
        || source.provider_kind != DataProviderKind::NautilusCatalog.code()
    {
        return Err(bad("native_catalog_identity"));
    }
    for (field, value) in [
        ("event_start", value.event_start),
        ("event_end", value.event_end),
        ("available_through", value.available_through),
        ("universe.selection_asof", value.universe.selection_asof),
        ("universe.coverage_start", value.universe.coverage_start),
        ("universe.coverage_end", value.universe.coverage_end),
    ] {
        time(value, field)?;
    }
    // A metadata document cannot raise its own source authority or erase the
    // original immutable snapshot. Only a trusted Runtime response reaches here.
    Ok(())
}

pub fn license_state(
    from: DateTime<Utc>,
    until: Option<DateTime<Utc>>,
    revoked: bool,
    now: DateTime<Utc>,
) -> DataLicenseState {
    if revoked {
        DataLicenseState::Revoked
    } else if now < from {
        DataLicenseState::NotYetValid
    } else if until.is_some_and(|until| now >= until) {
        DataLicenseState::Expired
    } else {
        DataLicenseState::Active
    }
}

pub fn validate_request(value: &DataValidateRequest) -> Result<(), DomainError> {
    let limits = &value.limits;
    if limits.experiments != 0 {
        return Err(bad("limits.experiments"));
    }
    if limits.cpu_seconds.get() == 0 {
        return Err(bad("limits.cpu_seconds"));
    }
    if !(1..=86_400).contains(&limits.wall_seconds) {
        return Err(bad("limits.wall_seconds"));
    }
    if !(1..=1_048_576).contains(&limits.memory_mib) {
        return Err(bad("limits.memory_mib"));
    }
    if !(1..=contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES).contains(&limits.output_bytes.get()) {
        return Err(bad("limits.output_bytes"));
    }
    Ok(())
}

pub fn list(value: &DataListQuery) -> Result<(), DomainError> {
    if !(1..=100).contains(&value.limit) {
        return Err(bad("limit"));
    }
    Ok(())
}
