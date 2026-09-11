//! Deployment-owned native homes. Public requests select labels, never paths,
//! commands, environment maps, credentials or a different executable.
use crate::codex_native::{self as native, Account, Client, Launch, ThreadOptions};
use chrono::Utc;
use contracts::{codex::*, SchemaV1};
use domain::codex::{resolve_overrides, settings as rules, CatalogContext};
use integrations::secrets::SecretVault;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use store::{
    codex_profiles::{CodexBindingCheck, CodexProfileSnapshot},
    StoreError,
};
use tokio::sync::Mutex;

mod account;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexDeploymentConfig {
    pub schema_version: SchemaV1,
    pub binary: PathBuf,
    pub executable_path: String,
    pub bindings: Vec<CodexDeploymentBinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexDeploymentBinding {
    pub reference: String,
    pub label: String,
    pub profile_origin: ProfileOrigin,
    pub home: PathBuf,
    pub codex_home: PathBuf,
    pub working_directory: PathBuf,
    /// Names only; their values are read by the trusted launcher and never returned.
    pub environment_names: Vec<String>,
}

struct Binding {
    public: CodexHomeBindingV1,
    home: PathBuf,
    codex_home: PathBuf,
    working_directory: PathBuf,
    environment: BTreeMap<OsString, OsString>,
    gate: Arc<Mutex<()>>,
    account: Arc<Mutex<Option<account::AccountMemory>>>,
}

#[derive(Default)]
pub struct CodexDeployment {
    binary: PathBuf,
    executable_path: OsString,
    bindings: BTreeMap<String, Binding>,
}

fn config_error() -> StoreError {
    StoreError::Invalid("codex_deployment_configuration")
}
fn directory(path: &Path) -> Result<PathBuf, StoreError> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(config_error());
    }
    std::fs::canonicalize(path).map_err(|_| config_error())
}

impl CodexDeployment {
    pub fn new(configuration: CodexDeploymentConfig) -> Result<Self, StoreError> {
        if !configuration.binary.is_absolute()
            || !configuration.binary.is_file()
            || configuration.executable_path.is_empty()
            || configuration.executable_path.contains('\0')
            || !(1..=32).contains(&configuration.bindings.len())
        {
            return Err(config_error());
        }
        let mut bindings = BTreeMap::new();
        let mut native_homes = BTreeSet::new();
        for value in configuration.bindings {
            rules::home_binding(&value.reference)?;
            domain::control::text(&value.label, 1, 120, false)?;
            let codex_home = directory(&value.codex_home)?;
            if !native_homes.insert(codex_home.clone()) || value.environment_names.len() > 32 {
                return Err(config_error());
            }
            let mut environment = BTreeMap::new();
            let mut names = BTreeSet::new();
            for name in value.environment_names {
                if name.is_empty()
                    || name.len() > 120
                    || !name.bytes().all(|byte| {
                        byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
                    })
                    || !names.insert(name.clone())
                    || matches!(name.as_str(), "HOME" | "CODEX_HOME" | "PATH" | "RUST_LOG")
                {
                    return Err(config_error());
                }
                if let Some(value) = std::env::var_os(&name) {
                    environment.insert(OsString::from(name), value);
                }
            }
            let binding = Binding {
                public: CodexHomeBindingV1 {
                    reference: value.reference.clone(),
                    label: value.label,
                    profile_origin: value.profile_origin,
                },
                home: directory(&value.home)?,
                codex_home,
                working_directory: directory(&value.working_directory)?,
                environment,
                gate: Arc::new(Mutex::new(())),
                account: Arc::new(Mutex::new(None)),
            };
            if bindings.insert(value.reference, binding).is_some() {
                return Err(config_error());
            }
        }
        Ok(Self {
            binary: configuration.binary,
            executable_path: configuration.executable_path.into(),
            bindings,
        })
    }

    pub fn public_bindings(&self) -> Vec<CodexHomeBindingV1> {
        self.bindings
            .values()
            .map(|binding| binding.public.clone())
            .collect()
    }

    pub async fn verify(
        &self,
        request: CodexBindingCheck,
        vault: Arc<SecretVault>,
    ) -> Result<(), StoreError> {
        self.binding(&request.home_binding, request.profile_origin)?;
        if let Some(id) = request.credential_ref {
            tokio::task::spawn_blocking(move || {
                let bytes = vault
                    .read(id, "CUSTOM_PROVIDER")
                    .map_err(|_| config_error())?;
                let value = std::str::from_utf8(&bytes).map_err(|_| config_error())?;
                domain::settings::secret_value(
                    contracts::settings::IntegrationSecretPurpose::CustomProvider,
                    value,
                )?;
                Ok::<_, StoreError>(())
            })
            .await
            .map_err(|_| StoreError::Integrity)??;
        }
        Ok(())
    }

    fn binding(&self, reference: &str, origin: ProfileOrigin) -> Result<&Binding, StoreError> {
        self.bindings
            .get(reference)
            .filter(|binding| binding.public.profile_origin == origin)
            .ok_or(StoreError::IntegrationUnavailable)
    }

    pub async fn probe(
        &self,
        snapshot: &CodexProfileSnapshot,
        vault: Arc<SecretVault>,
    ) -> CodexProbeOutcomeV1 {
        match tokio::time::timeout(Duration::from_secs(110), self.observe(snapshot, vault)).await {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(reason)) => CodexProbeOutcomeV1::Unavailable { reason },
            Err(_) => CodexProbeOutcomeV1::Unavailable {
                reason: CodexProbeFailureV1::NativeUnavailable,
            },
        }
    }

    async fn observe(
        &self,
        snapshot: &CodexProfileSnapshot,
        vault: Arc<SecretVault>,
    ) -> Result<CodexProbeOutcomeV1, CodexProbeFailureV1> {
        let profile = &snapshot.profile;
        let binding = profile
            .home_binding
            .as_deref()
            .and_then(|label| self.binding(label, profile.profile_origin).ok())
            .ok_or(CodexProbeFailureV1::DeploymentUnavailable)?;
        let _gate = binding.gate.lock().await;
        let custom_provider = if profile.connection_mode == ConnectionMode::CustomProvider {
            let id = snapshot
                .credential_ref
                .ok_or(CodexProbeFailureV1::DeploymentUnavailable)?;
            let api_key = tokio::task::spawn_blocking(move || vault.read(id, "CUSTOM_PROVIDER"))
                .await
                .map_err(|_| CodexProbeFailureV1::DeploymentUnavailable)?
                .map_err(|_| CodexProbeFailureV1::DeploymentUnavailable)?;
            Some(native::CustomProvider {
                base_url: profile
                    .custom_base_url
                    .clone()
                    .ok_or(CodexProbeFailureV1::DeploymentUnavailable)?,
                api_key: String::from_utf8(api_key)
                    .map_err(|_| CodexProbeFailureV1::DeploymentUnavailable)?,
            })
        } else {
            None
        };
        let launch = Launch {
            binary: self.binary.clone(),
            home: binding.home.clone(),
            codex_home: binding.codex_home.clone(),
            working_directory: binding.working_directory.clone(),
            executable_path: self.executable_path.clone(),
            native_environment: binding.environment.clone(),
            custom_provider,
        };
        let mut client = Client::start(launch).await.map_err(native_failure)?;
        let observed = inspect(&mut client, profile, &binding.working_directory).await;
        let closed = client.close().await.map_err(native_failure);
        match observed {
            Err(error) => Err(error),
            Ok(result) => {
                closed?;
                Ok(result)
            }
        }
    }
}

fn native_failure(value: native::NativeFailure) -> CodexProbeFailureV1 {
    use native::NativeFailure;
    match value {
        NativeFailure::Configuration => CodexProbeFailureV1::DeploymentUnavailable,
        NativeFailure::Version => CodexProbeFailureV1::VersionUnsupported,
        NativeFailure::Contract
        | NativeFailure::Correlation
        | NativeFailure::FrameLimit
        | NativeFailure::ObservationLimit => CodexProbeFailureV1::ContractUnsupported,
        NativeFailure::ModelUnavailable => CodexProbeFailureV1::ModelSettingsUnsupported,
        _ => CodexProbeFailureV1::NativeUnavailable,
    }
}

async fn inspect(
    client: &mut Client,
    profile: &CodexProfileViewV1,
    working_directory: &Path,
) -> Result<CodexProbeOutcomeV1, CodexProbeFailureV1> {
    let account = account_view(client.account().await.map_err(native_failure)?);
    if account.requires_openai_auth && account.authentication_kind.is_none() {
        return Err(CodexProbeFailureV1::AuthenticationRequired);
    }
    let native_models = client.models().await.map_err(native_failure)?;
    let fetched_at = chrono::DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .ok_or(CodexProbeFailureV1::NativeUnavailable)?;
    let models: Vec<CodexAdvertisedModelV1> = native_models
        .into_iter()
        .map(|model| CodexAdvertisedModelV1 {
            capability: ModelCapabilityV1 {
                schema_version: SchemaV1,
                id: model.id,
                model: model.model,
                display_name: model.display_name,
                hidden: model.hidden,
                default_reasoning_effort: model.default_reasoning_effort,
                supported_reasoning_efforts: model
                    .supported_reasoning_efforts
                    .into_iter()
                    .map(|effort| ReasoningEffortCapability {
                        reasoning_effort: effort.reasoning_effort,
                        description: effort.description,
                    })
                    .collect(),
                is_default: model.is_default,
                fetched_at,
                profile_revision: profile.revision,
            },
            service_tiers: model
                .service_tiers
                .into_iter()
                .map(|tier| CodexServiceTierV1 {
                    id: tier.id,
                    name: tier.name,
                    description: tier.description,
                })
                .collect(),
            default_service_tier: model.default_service_tier,
        })
        .collect();
    let capabilities: Vec<_> = models
        .iter()
        .map(|model| model.capability.clone())
        .collect();
    let mut options = ThreadOptions::read_only(working_directory.to_path_buf());
    options.ephemeral = true;
    if profile.connection_mode == ConnectionMode::CustomProvider {
        options.expected_provider = Some("quazonai_custom".into());
    }
    let default = client
        .start_thread(&options)
        .await
        .map_err(native_failure)?;
    let overrides = resolve_overrides(
        &profile.model_settings,
        &CatalogContext {
            models: &capabilities,
            complete: true,
            profile_revision: profile.revision,
            valid_after: fetched_at,
            observed_effective_model: Some(&default.model),
        },
    )
    .map_err(|_| CodexProbeFailureV1::ModelSettingsUnsupported)?;
    options.model = overrides.model;
    options.reasoning_effort = overrides.reasoning_effort;
    if overrides.fast_mode == Some(true) {
        let selected = options.model.as_deref().unwrap_or(&default.model);
        let model = models
            .iter()
            .find(|model| model.capability.model == selected)
            .ok_or(CodexProbeFailureV1::ModelSettingsUnsupported)?;
        options.service_tier = Some(
            rules::fast_tier(model).map_err(|_| CodexProbeFailureV1::ModelSettingsUnsupported)?,
        );
    }
    let effective = if options.model.is_some()
        || options.reasoning_effort.is_some()
        || options.service_tier.is_some()
    {
        client
            .start_thread(&options)
            .await
            .map_err(native_failure)?
    } else {
        default
    };
    Ok(CodexProbeOutcomeV1::Available {
        native_version: native::VERSION.into(),
        account,
        effective: CodexEffectiveSettingsV1 {
            model: effective.model,
            provider: effective.model_provider,
            reasoning_effort: effective.reasoning_effort,
            service_tier: effective.service_tier,
        },
        models,
    })
}

fn account_view(account: native::AccountState) -> CodexAccountV1 {
    let (authentication_kind, plan_type) = match account.account {
        None => (None, None),
        Some(Account::ApiKey) => (Some(CodexAuthenticationKind::ApiKey), None),
        Some(Account::Chatgpt { plan_type }) => {
            (Some(CodexAuthenticationKind::Chatgpt), Some(plan_type))
        }
        Some(Account::AmazonBedrock) => (Some(CodexAuthenticationKind::AmazonBedrock), None),
    };
    CodexAccountV1 {
        requires_openai_auth: account.requires_openai_auth,
        authentication_kind,
        plan_type,
    }
}
