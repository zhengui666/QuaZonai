//! Deployment-owned native homes. Public requests select labels, never paths,
//! commands, environment maps, credentials or a different executable.
use crate::codex_native::{self as native, Account, Client, Launch, ThreadOptions};
use chrono::Utc;
use contracts::{codex::*, SchemaV1};
use domain::codex::{resolve_overrides, settings as rules, CatalogContext};
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

pub struct CodexDeploymentConfig {
    pub schema_version: SchemaV1,
    pub binary: PathBuf,
    pub executable_path: String,
    pub bindings: Vec<CodexDeploymentBinding>,
}

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
    /// Discover the current OS user's native installation without reading or
    /// copying native configuration/credentials into QZ. Missing Codex does not
    /// stop the local console; a probe reports deployment unavailability.
    pub fn discover() -> Self {
        Self::discover_from(
            std::env::var_os("PATH"),
            std::env::var_os("HOME"),
            std::env::var_os("CODEX_HOME"),
            std::env::current_dir().ok(),
        )
        .unwrap_or_default()
    }

    fn discover_from(
        executable_path: Option<OsString>,
        home: Option<OsString>,
        codex_home: Option<OsString>,
        working_directory: Option<PathBuf>,
    ) -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;
        let executable_path = executable_path?;
        let binary = std::env::split_paths(&executable_path)
            .filter(|path| path.is_absolute())
            .map(|path| path.join("codex"))
            .find(|path| {
                std::fs::metadata(path).is_ok_and(|metadata| {
                    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
                })
            })?;
        let binary = std::fs::canonicalize(binary).ok()?;
        let home = directory(&PathBuf::from(home?)).ok()?;
        let codex_home = directory(
            &codex_home
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex")),
        )
        .ok()?;
        let working_directory = directory(&working_directory?).ok()?;
        let environment = [
            "OPENAI_API_KEY",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "LANG",
            "LC_ALL",
        ]
        .into_iter()
        .filter_map(|name| std::env::var_os(name).map(|value| (OsString::from(name), value)))
        .collect::<BTreeMap<_, _>>();
        let gate = Arc::new(Mutex::new(()));
        let account = Arc::new(Mutex::new(None));
        let mut bindings = BTreeMap::new();
        for (reference, label) in [
            ("local-researcher", "研究员"),
            ("local-reviewer", "独立审阅员"),
        ] {
            bindings.insert(
                reference.to_owned(),
                Binding {
                    public: CodexHomeBindingV1 {
                        reference: reference.into(),
                        label: label.into(),
                        profile_origin: ProfileOrigin::OperatorMount,
                    },
                    home: home.clone(),
                    codex_home: codex_home.clone(),
                    working_directory: working_directory.clone(),
                    environment: environment.clone(),
                    gate: gate.clone(),
                    account: account.clone(),
                },
            );
        }
        Some(Self {
            binary,
            executable_path,
            bindings,
        })
    }

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

    pub fn available(&self) -> bool {
        !self.bindings.is_empty()
    }

    pub fn public_bindings(&self) -> Vec<CodexHomeBindingV1> {
        self.bindings
            .values()
            .map(|binding| binding.public.clone())
            .collect()
    }

    pub(crate) fn executable_path(&self) -> &std::ffi::OsStr {
        &self.executable_path
    }

    /// No paid request. Reuse the actual account/catalog/settings observation;
    /// the caller still needs a durable Run send permit and scoped MCP binding.
    pub(crate) async fn mission_connection(
        &self,
        snapshot: &CodexProfileSnapshot,
        workspace: &Path,
        resources: native::MissionProcess,
    ) -> Result<(Client, ThreadOptions), CodexProbeFailureV1> {
        let profile = &snapshot.profile;
        let binding = profile
            .home_binding
            .as_deref()
            .and_then(|label| self.binding(label, profile.profile_origin).ok())
            .ok_or(CodexProbeFailureV1::DeploymentUnavailable)?;
        let workspace =
            directory(workspace).map_err(|_| CodexProbeFailureV1::DeploymentUnavailable)?;
        if binding.home.starts_with(&workspace)
            || binding.codex_home.starts_with(&workspace)
            || workspace.starts_with(&binding.codex_home)
        {
            return Err(CodexProbeFailureV1::DeploymentUnavailable);
        }
        let _gate = binding.gate.lock().await;
        let launch = self.launch(binding, &workspace);
        let mut client = Client::start_mission(launch, resources)
            .await
            .map_err(native_failure)?;
        let (_, mut options) = inspect(&mut client, profile, &workspace).await?;
        options.ephemeral = false;
        Ok((client, options))
    }

    pub async fn verify(&self, request: CodexBindingCheck) -> Result<(), StoreError> {
        self.binding(&request.home_binding, request.profile_origin)?;
        Ok(())
    }

    fn binding(&self, reference: &str, origin: ProfileOrigin) -> Result<&Binding, StoreError> {
        self.bindings
            .get(reference)
            .filter(|binding| binding.public.profile_origin == origin)
            .ok_or(StoreError::IntegrationUnavailable)
    }

    pub async fn probe(&self, snapshot: &CodexProfileSnapshot) -> CodexProbeOutcomeV1 {
        match tokio::time::timeout(Duration::from_secs(110), self.observe(snapshot)).await {
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
    ) -> Result<CodexProbeOutcomeV1, CodexProbeFailureV1> {
        let profile = &snapshot.profile;
        let binding = profile
            .home_binding
            .as_deref()
            .and_then(|label| self.binding(label, profile.profile_origin).ok())
            .ok_or(CodexProbeFailureV1::DeploymentUnavailable)?;
        let _gate = binding.gate.lock().await;
        let launch = self.launch(binding, &binding.working_directory);
        let mut client = Client::start(launch).await.map_err(native_failure)?;
        let observed = inspect(&mut client, profile, &binding.working_directory).await;
        let closed = client.close().await.map_err(native_failure);
        match observed {
            Err(error) => Err(error),
            Ok((result, _)) => {
                closed?;
                Ok(result)
            }
        }
    }

    fn launch(&self, binding: &Binding, working_directory: &Path) -> Launch {
        Launch {
            binary: self.binary.clone(),
            home: binding.home.clone(),
            codex_home: binding.codex_home.clone(),
            working_directory: working_directory.to_owned(),
            executable_path: self.executable_path.clone(),
            native_environment: binding.environment.clone(),
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
) -> Result<(CodexProbeOutcomeV1, ThreadOptions), CodexProbeFailureV1> {
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
    Ok((
        CodexProbeOutcomeV1::Available {
            native_version: native::VERSION.into(),
            account,
            effective: CodexEffectiveSettingsV1 {
                model: effective.model,
                provider: effective.model_provider,
                reasoning_effort: effective.reasoning_effort,
                service_tier: effective.service_tier,
            },
            models,
        },
        options,
    ))
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

#[cfg(test)]
mod discovery_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn discovers_native_path_and_default_home_without_reading_credentials() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        let home = root.path().join("home");
        let native = home.join(".codex");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&native).unwrap();
        let executable = bin.join("codex");
        std::fs::write(&executable, b"not executed by this discovery test").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = native.join("config.toml");
        std::fs::write(&config, b"model = 'native-model'\n").unwrap();
        let deployment = CodexDeployment::discover_from(
            Some(bin.as_os_str().into()),
            Some(home.as_os_str().into()),
            None,
            Some(root.path().into()),
        )
        .unwrap();
        assert_eq!(deployment.binary, executable.canonicalize().unwrap());
        let researcher = &deployment.bindings["local-researcher"];
        let reviewer = &deployment.bindings["local-reviewer"];
        assert_eq!(researcher.codex_home, native.canonicalize().unwrap());
        assert_eq!(researcher.codex_home, reviewer.codex_home);
        assert!(Arc::ptr_eq(&researcher.gate, &reviewer.gate));
        assert!(Arc::ptr_eq(&researcher.account, &reviewer.account));
        assert_eq!(std::fs::read(config).unwrap(), b"model = 'native-model'\n");
        assert!(!native.join("auth.json").exists());

        let override_home = root.path().join("native");
        std::fs::create_dir(&override_home).unwrap();
        let explicit = CodexDeployment::discover_from(
            Some(bin.as_os_str().into()),
            Some(home.as_os_str().into()),
            Some(override_home.as_os_str().into()),
            Some(root.path().into()),
        )
        .unwrap();
        assert_eq!(
            explicit.bindings["local-researcher"].codex_home,
            override_home.canonicalize().unwrap()
        );
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(CodexDeployment::discover_from(
            Some(bin.as_os_str().into()),
            Some(home.as_os_str().into()),
            None,
            Some(root.path().into())
        )
        .is_none());
        assert!(CodexDeployment::discover_from(None, None, None, None).is_none());
    }
}
