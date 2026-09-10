//! Validate bounded native observations; never infer a missing capability.
use crate::{control::text, research::invalid, DomainError};
use chrono::{DateTime, Duration, Utc};
use contracts::runtime::*;
use std::collections::BTreeSet;

pub fn pinned_image(value: &str) -> bool {
    let Some((name, digest)) = value.rsplit_once("@sha256:") else {
        return false;
    };
    !name.is_empty()
        && value.len() <= 512
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'/' | b'_' | b'-' | b':')
        })
        && digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn unique<T: PartialEq>(values: &[T], minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&values.len())
        && values
            .iter()
            .enumerate()
            .all(|(index, value)| !values[..index].contains(value))
}

pub fn capabilities(value: &RuntimeCapabilitiesV1, now: DateTime<Utc>) -> Result<(), DomainError> {
    let bad = || invalid("runtime_capabilities", "UNSUPPORTED_NATIVE_CAPABILITIES");
    if value.protocol_versions.len() != 1
        || text(&value.runtime_version, 1, 120, false).is_err()
        || !(1..=64).contains(&value.engine_versions.len())
        || value.engine_versions.iter().any(|(name, version)| {
            text(name, 1, 120, false).is_err() || text(version, 1, 120, false).is_err()
        })
        || !unique(&value.job_kinds, 1, 8)
        || value.image_refs.len() != value.job_kinds.len()
        || !unique(&value.data_kinds, 1, 7)
        || !unique(&value.solver_capabilities, 0, 64)
        || value
            .solver_capabilities
            .iter()
            .any(|name| text(name, 1, 120, false).is_err())
        || value.max_cpu == 0
        || value.max_cpu > 1024
        || value.max_memory_mib == 0
        || value.max_output_bytes.get() == 0
        || value.max_wall_seconds == 0
        || value.checked_at > now + Duration::seconds(5)
        || value.checked_at < now - Duration::minutes(5)
    {
        return Err(bad());
    }
    for kind in &value.job_kinds {
        let images: Vec<_> = value
            .image_refs
            .iter()
            .filter(|image| image.job_kind == *kind)
            .collect();
        if images.len() != 1 || !pinned_image(&images[0].image_ref) {
            return Err(bad());
        }
    }
    let mut schemas = BTreeSet::new();
    if !(1..=64).contains(&value.artifact_schemas.len()) {
        return Err(bad());
    }
    for schema in &value.artifact_schemas {
        if text(&schema.name, 1, 120, false).is_err()
            || text(&schema.version, 1, 40, false).is_err()
            || !schemas.insert((&schema.name, &schema.version))
        {
            return Err(bad());
        }
    }
    let mut venues = BTreeSet::new();
    if value.venues.len() > 256 {
        return Err(bad());
    }
    for venue in &value.venues {
        if text(&venue.venue, 1, 120, false).is_err()
            || !venues.insert(&venue.venue)
            || !unique(&venue.instrument_classes, 1, 16)
            || venue
                .instrument_classes
                .iter()
                .any(|name| text(name, 1, 120, false).is_err())
            || !unique(&venue.data_kinds, 1, 7)
            || venue
                .data_kinds
                .iter()
                .any(|kind| !value.data_kinds.contains(kind))
        {
            return Err(bad());
        }
    }
    Ok(())
}
