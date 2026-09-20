//! Validate newly activated preferences against the latest native observation.
//! Receipt replay is handled by the caller before this check. No native process,
//! configuration file, credential or new model catalog is created here.
use super::{account, observation, Tx};
use crate::StoreError;
use chrono::{DateTime, Duration, Utc};
use contracts::codex::*;
use domain::{
    codex::{resolve_overrides, settings::fast_tier, CatalogContext},
    DomainError,
};

pub(super) async fn validate(
    tx: &mut Tx<'_>,
    profile: &CodexProfileViewV1,
    settings: &SavedModelSettingsV1,
) -> Result<(), StoreError> {
    if settings.use_default_model_settings {
        return Ok(());
    }
    // Do not filter for success: a newer failure supersedes an older catalog.
    let record = sqlx::query("SELECT * FROM app.codex_profile_observations WHERE profile_id=$1 ORDER BY observed_at DESC,id DESC LIMIT 1")
        .bind(profile.id.as_uuid()).fetch_optional(&mut **tx).await?
        .ok_or(DomainError::CapabilityUnavailable("fresh_codex_catalog_required"))?;
    let native = observation(&record)?;
    let unchanged =
        account::observation_after_account_operations(tx, profile.id, native.observed_at).await?;
    // The caller holds the profile/account group lock. Read the clock after the
    // native-binding callback and any account locks, immediately before mutation.
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if native.profile_revision != profile.revision || native.valid_until <= now || !unchanged {
        return Err(DomainError::CapabilityUnavailable("stale_model_catalog").into());
    }
    let CodexProbeOutcomeV1::Available {
        effective, models, ..
    } = &native.outcome
    else {
        return Err(DomainError::CapabilityUnavailable("native_model_catalog_unavailable").into());
    };
    let capabilities: Vec<_> = models
        .iter()
        .map(|model| model.capability.clone())
        .collect();
    let overrides = resolve_overrides(
        settings,
        &CatalogContext {
            models: &capabilities,
            complete: true,
            profile_revision: profile.revision,
            // Stored probes already enforce a 120s execution deadline and 5s
            // upstream clock tolerance; their separate 60s cache expiry is above.
            valid_after: native.observed_at - Duration::seconds(125),
            observed_effective_model: Some(&effective.model),
        },
    )?;
    if overrides.fast_mode == Some(true) {
        let selected = overrides.model.as_deref().unwrap_or(&effective.model);
        let model = models
            .iter()
            .find(|model| model.capability.model == selected)
            .ok_or(DomainError::CapabilityUnavailable("model_not_available"))?;
        fast_tier(model)?;
    }
    Ok(())
}
