//! Resolve application overrides using a fresh, complete native capability set.
//! There are no hard-coded model IDs/efforts and no Provider credentials here.
use contracts::{
    codex::{ModelCapabilityV1, SavedModelSettingsV1},
    Revision, Timestamp,
};

use crate::DomainError;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModelOverrides {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub fast_mode: Option<bool>,
}

/// A caller must obtain the effective SYSTEM model from native config/thread
/// observation, not guess it from `is_default` in the public model catalog.
#[derive(Clone, Copy, Debug)]
pub struct CatalogContext<'a> {
    pub models: &'a [ModelCapabilityV1],
    pub complete: bool,
    pub profile_revision: Revision,
    pub valid_after: Timestamp,
    pub observed_effective_model: Option<&'a str>,
}

pub fn resolve_overrides(
    saved: &SavedModelSettingsV1,
    catalog: &CatalogContext<'_>,
) -> Result<ModelOverrides, DomainError> {
    if saved.use_default_model_settings {
        // Do not validate ignored saved values or mutate/delete them.
        return Ok(ModelOverrides::default());
    }
    if saved
        .saved_model
        .as_ref()
        .is_some_and(|value| value.trim().is_empty())
        || saved
            .saved_reasoning_effort
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Err(DomainError::Invalid("model_settings"));
    }
    if saved.saved_model.is_none() && saved.saved_reasoning_effort.is_none() {
        return Ok(ModelOverrides {
            fast_mode: saved.saved_fast_mode.then_some(true),
            ..Default::default()
        });
    }
    if !catalog.complete {
        return Err(DomainError::CapabilityUnavailable(
            "incomplete_model_catalog",
        ));
    }
    let model_id = saved
        .saved_model
        .as_deref()
        .or(catalog.observed_effective_model)
        .ok_or(DomainError::CapabilityUnavailable(
            "effective_system_model_unknown",
        ))?;
    let mut matches = catalog
        .models
        .iter()
        .filter(|model| model.model == model_id);
    let model = matches
        .next()
        .ok_or(DomainError::CapabilityUnavailable("model_not_available"))?;
    if matches.next().is_some() {
        return Err(DomainError::Invalid("duplicate_native_model"));
    }
    if model.profile_revision != catalog.profile_revision || model.fetched_at < catalog.valid_after
    {
        return Err(DomainError::CapabilityUnavailable("stale_model_catalog"));
    }
    if let Some(effort) = &saved.saved_reasoning_effort {
        if !model
            .supported_reasoning_efforts
            .iter()
            .any(|supported| &supported.reasoning_effort == effort)
        {
            return Err(DomainError::CapabilityUnavailable(
                "unsupported_model_effort",
            ));
        }
    }
    Ok(ModelOverrides {
        model: saved.saved_model.clone(),
        reasoning_effort: saved.saved_reasoning_effort.clone(),
        fast_mode: saved.saved_fast_mode.then_some(true),
    })
}
