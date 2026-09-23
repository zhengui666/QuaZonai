//! Profile intent and nonsecret native-observation rules. This module performs
//! no model lookup, token handling, OAuth, filesystem access or native tool loop.
use crate::{control::text, research::invalid, DomainError};
use chrono::{DateTime, Duration, Utc};
use contracts::{codex::*, Revision};
use std::collections::BTreeSet;

fn bad(field: &str) -> DomainError {
    invalid(field, "CODEX_PROFILE_INVALID")
}

pub fn account_snapshot(account: &CodexAccountV1) -> Result<(), DomainError> {
    if account.plan_type.is_some()
        != (account.authentication_kind == Some(CodexAuthenticationKind::Chatgpt))
    {
        return Err(bad("account.plan_type"));
    }
    if let Some(plan) = &account.plan_type {
        text(plan, 1, 120, false).map_err(|_| bad("account.plan_type"))?;
    }
    Ok(())
}

pub fn home_binding(value: &str) -> Result<(), DomainError> {
    if !(1..=64).contains(&value.len())
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
    {
        return Err(bad("home_binding"));
    }
    Ok(())
}

pub fn model_settings(value: &SavedModelSettingsV1) -> Result<(), DomainError> {
    for (field, value) in [
        ("model_settings.saved_model", &value.saved_model),
        (
            "model_settings.saved_reasoning_effort",
            &value.saved_reasoning_effort,
        ),
    ] {
        if let Some(value) = value {
            text(value, 1, 200, false).map_err(|_| bad(field))?;
            if value.trim() != value {
                return Err(bad(field));
            }
        }
    }
    // This validates shape only. New active overrides additionally require a
    // fresh native catalog in Store; dormant values and native defaults do not.
    Ok(())
}

pub fn profile_create(value: &CodexProfileCreateV1) -> Result<(), DomainError> {
    text(&value.name, 1, 120, false).map_err(|_| bad("name"))?;
    home_binding(&value.home_binding)?;
    model_settings(&value.model_settings)?;
    Ok(())
}

pub fn profile_update(value: &CodexProfileUpdateV1) -> Result<(), DomainError> {
    model_settings(&value.model_settings)?;
    Ok(())
}

/// Supported by the native service-tier contract. Selection
/// still requires this exact tier to be advertised by the actual chosen model.
pub fn fast_tier(model: &CodexAdvertisedModelV1) -> Result<String, DomainError> {
    ["priority", "fast"]
        .into_iter()
        .find(|id| model.service_tiers.iter().any(|tier| tier.id == *id))
        .map(str::to_owned)
        .ok_or(DomainError::CapabilityUnavailable(
            "unsupported_model_service_tier",
        ))
}

pub fn probe_outcome(
    value: &CodexProbeOutcomeV1,
    settings: &SavedModelSettingsV1,
    mode: ConnectionMode,
    revision: Revision,
    started: DateTime<Utc>,
    completed: DateTime<Utc>,
) -> Result<(), DomainError> {
    let CodexProbeOutcomeV1::Available {
        native_version,
        account,
        effective,
        native_default_model,
        models,
    } = value
    else {
        return Ok(());
    };
    if mode != ConnectionMode::System
        || !super::valid_codex_version(native_version)
        || !(1..=4096).contains(&models.len())
    {
        return Err(bad("native_catalog"));
    }
    if account.requires_openai_auth && account.authentication_kind.is_none() {
        return Err(DomainError::CapabilityUnavailable(
            "codex_authentication_required",
        ));
    }
    account_snapshot(account)?;
    let native_default = native_default_model
        .as_deref()
        .ok_or_else(|| bad("native_default_model"))?;
    text(native_default, 1, 200, false).map_err(|_| bad("native_default_model"))?;
    if native_default.trim() != native_default {
        return Err(bad("native_default_model"));
    }
    if (settings.use_default_model_settings || settings.saved_model.is_none())
        && effective.model != native_default
    {
        return Err(DomainError::CapabilityUnavailable(
            "codex_settings_not_honored",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for item in models {
        let model = &item.capability;
        for value in [
            &model.id,
            &model.model,
            &model.display_name,
            &model.default_reasoning_effort,
        ] {
            text(value, 1, 200, false).map_err(|_| bad("native_catalog.model"))?;
        }
        if !ids.insert(&model.id)
            || !names.insert(&model.model)
            || model.profile_revision != revision
            || model.fetched_at < started - Duration::seconds(5)
            || model.fetched_at > completed + Duration::seconds(5)
            || !(1..=64).contains(&model.supported_reasoning_efforts.len())
            || item.service_tiers.len() > 64
        {
            return Err(bad("native_catalog.model"));
        }
        let mut efforts = BTreeSet::new();
        for effort in &model.supported_reasoning_efforts {
            text(&effort.reasoning_effort, 1, 200, false)
                .map_err(|_| bad("native_catalog.effort"))?;
            if effort.description.chars().count() > 8192
                || !efforts.insert(&effort.reasoning_effort)
            {
                return Err(bad("native_catalog.effort"));
            }
        }
        if !efforts.contains(&model.default_reasoning_effort) {
            return Err(bad("native_catalog.effort"));
        }
        let mut tiers = BTreeSet::new();
        for tier in &item.service_tiers {
            text(&tier.id, 1, 200, false).map_err(|_| bad("native_catalog.service_tier"))?;
            text(&tier.name, 1, 200, false).map_err(|_| bad("native_catalog.service_tier"))?;
            if tier.description.chars().count() > 8192 || !tiers.insert(&tier.id) {
                return Err(bad("native_catalog.service_tier"));
            }
        }
        if item
            .default_service_tier
            .as_ref()
            .is_some_and(|tier| !tiers.contains(tier))
        {
            return Err(bad("native_catalog.service_tier"));
        }
    }
    text(&effective.model, 1, 200, false).map_err(|_| bad("effective.model"))?;
    text(&effective.provider, 1, 200, false).map_err(|_| bad("effective.provider"))?;
    let observed = models
        .iter()
        .find(|item| item.capability.model == effective.model)
        .ok_or(DomainError::CapabilityUnavailable(
            "effective_model_not_advertised",
        ))?;
    if effective.reasoning_effort.as_ref().is_some_and(|value| {
        !observed
            .capability
            .supported_reasoning_efforts
            .iter()
            .any(|effort| effort.reasoning_effort == *value)
    }) || effective
        .service_tier
        .as_ref()
        .is_some_and(|value| !observed.service_tiers.iter().any(|tier| tier.id == *value))
    {
        return Err(bad("effective"));
    }
    if !settings.use_default_model_settings
        && (settings
            .saved_model
            .as_ref()
            .is_some_and(|model| *model != effective.model)
            || settings
                .saved_reasoning_effort
                .as_ref()
                .is_some_and(|effort| Some(effort) != effective.reasoning_effort.as_ref())
            || (settings.saved_fast_mode
                && effective.service_tier.as_ref() != Some(&fast_tier(observed)?)))
    {
        return Err(DomainError::CapabilityUnavailable(
            "codex_settings_not_honored",
        ));
    }
    Ok(())
}
