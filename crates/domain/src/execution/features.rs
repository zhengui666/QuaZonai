//! Bounded as-of feature state. This validates caller declarations; it cannot
//! establish that an external source or pretrained policy was historically PIT.
use crate::{control::text, DomainError};
use contracts::{research::DataPartition, science::*, DbCounter, Id};
use std::collections::BTreeSet;

fn bad(field: &str) -> DomainError {
    super::bad(field)
}

pub fn artifact_ids(ids: &[Id]) -> Result<(), DomainError> {
    if !(1..=MAX_FEATURE_ARTIFACTS).contains(&ids.len())
        || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
    {
        return Err(bad("feature_artifact_ids"));
    }
    Ok(())
}

pub fn schema(value: &[FeatureDefinitionV1]) -> Result<(), DomainError> {
    if !(1..=MAX_EXPERIMENT_FEATURES).contains(&value.len()) {
        return Err(bad("feature_schema"));
    }
    let mut keys = BTreeSet::new();
    for definition in value {
        text(&definition.feature_key, 1, 120, false)?;
        text(&definition.source_ref, 1, 512, false)?;
        text(&definition.source_key, 1, 200, false)?;
        if !keys.insert(&definition.feature_key) {
            return Err(bad("feature_schema.feature_key"));
        }
    }
    Ok(())
}

pub fn effective_available_ns(
    definition: &FeatureDefinitionV1,
    row: &FeatureObservationV1,
) -> Result<DbCounter, DomainError> {
    if row
        .observed_available_ns
        .is_some_and(|time| time < row.event_ns)
        || row.value.is_some_and(|v| !v.is_finite())
        || row.value.is_some() == row.missing_reason.is_some()
    {
        return Err(bad("feature_observation"));
    }
    if let Some(reason) = &row.missing_reason {
        text(reason, 1, 120, false)?;
    }
    match definition.availability {
        FeatureAvailabilityV1::Observed => row
            .observed_available_ns
            .ok_or_else(|| bad("feature_observation.observed_available_ns")),
        FeatureAvailabilityV1::ModeledLag { lag_ns } => row
            .event_ns
            .get()
            .checked_add(lag_ns.get())
            .and_then(|n| DbCounter::new(n).ok())
            .ok_or_else(|| bad("feature_observation.modeled_available_ns")),
    }
}

pub fn observations(value: &FeatureObservationsV1) -> Result<(), DomainError> {
    schema(&value.feature_schema)?;
    if !matches!(
        value.partition,
        DataPartition::Discovery | DataPartition::Validation | DataPartition::Forward
    ) || !(1..=MAX_FEATURE_OBSERVATIONS).contains(&value.observations.len())
    {
        return Err(bad("feature_observations"));
    }
    let mut identities = BTreeSet::new();
    for row in &value.observations {
        let definition = value
            .feature_schema
            .get(usize::from(row.feature_index))
            .ok_or_else(|| bad("feature_index"))?;
        effective_available_ns(definition, row)?;
        if !identities.insert((row.feature_index, row.sequence)) {
            return Err(bad("feature_observation.sequence"));
        }
    }
    Ok(())
}

pub fn bind_observations(
    parts: &[FeatureObservationsV1],
    definitions: &[FeatureDefinitionV1],
    partition: DataPartition,
) -> Result<(), DomainError> {
    schema(definitions)?;
    if !(1..=MAX_FEATURE_ARTIFACTS).contains(&parts.len()) {
        return Err(bad("feature_parts"));
    }
    let mut count = 0usize;
    let mut identities = BTreeSet::new();
    for part in parts {
        observations(part)?;
        count = count
            .checked_add(part.observations.len())
            .ok_or_else(|| bad("feature_observations"))?;
        if part.feature_schema != definitions
            || part.partition != partition
            || count > MAX_FEATURE_OBSERVATIONS
        {
            return Err(bad("feature_parts.binding"));
        }
        for row in &part.observations {
            if !identities.insert((row.feature_index, row.sequence)) {
                return Err(bad("feature_parts.duplicate"));
            }
        }
    }
    Ok(())
}

/// Native CustomData delivers by effective availability, observations before a
/// decision at an equal clock. Among arrivals, latest source event wins; late
/// older events cannot overwrite a newer observation. No per-decision row scan.
pub struct FeatureState {
    definitions: Vec<FeatureDefinitionV1>,
    latest: Vec<Option<(DbCounter, FeatureObservationV1)>>,
    last_arrival: DbCounter,
    last_decision: Option<DbCounter>,
}
impl FeatureState {
    pub fn new(definitions: &[FeatureDefinitionV1]) -> Result<Self, DomainError> {
        schema(definitions)?;
        Ok(Self {
            definitions: definitions.to_vec(),
            latest: vec![None; definitions.len()],
            last_arrival: DbCounter::ZERO,
            last_decision: None,
        })
    }
    pub fn observe(&mut self, row: &FeatureObservationV1) -> Result<(), DomainError> {
        let index = usize::from(row.feature_index);
        let definition = self
            .definitions
            .get(index)
            .ok_or_else(|| bad("feature_index"))?;
        let available = effective_available_ns(definition, row)?;
        if available < self.last_arrival || self.last_decision.is_some_and(|time| available <= time)
        {
            return Err(bad("feature_arrival_order"));
        }
        self.last_arrival = available;
        let rank = (row.event_ns, available, row.sequence);
        if self.latest[index]
            .as_ref()
            .is_none_or(|(old_available, old)| rank > (old.event_ns, *old_available, old.sequence))
        {
            self.latest[index] = Some((available, row.clone()));
        }
        Ok(())
    }
    pub fn values(&mut self, decision_ns: DbCounter) -> Result<Vec<FeatureValueV1>, DomainError> {
        if decision_ns < self.last_arrival
            || self.last_decision.is_some_and(|time| decision_ns < time)
        {
            return Err(bad("feature_decision_order"));
        }
        self.last_decision = Some(decision_ns);
        Ok(self
            .latest
            .iter()
            .zip(&self.definitions)
            .map(|(latest, definition)| {
                let Some((available, row)) = latest else {
                    return FeatureValueV1 {
                        value: None,
                        missing_reason: Some(FeatureMissingReasonV1::NotYetAvailable),
                        event_ns: None,
                        observed_available_ns: None,
                        effective_available_ns: None,
                        sequence: None,
                    };
                };
                let expired = definition
                    .max_age_ns
                    .is_some_and(|age| decision_ns.get() - row.event_ns.get() > age.get());
                let missing_reason = if expired {
                    Some(FeatureMissingReasonV1::Expired)
                } else if row.value.is_none() {
                    Some(FeatureMissingReasonV1::SourceMissing)
                } else {
                    None
                };
                FeatureValueV1 {
                    value: if missing_reason.is_none() {
                        row.value
                    } else {
                        None
                    },
                    missing_reason,
                    event_ns: Some(row.event_ns),
                    observed_available_ns: row.observed_available_ns,
                    effective_available_ns: Some(*available),
                    sequence: Some(row.sequence),
                }
            })
            .collect())
    }
}
