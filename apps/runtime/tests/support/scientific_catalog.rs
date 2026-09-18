//! Actual Parquet observation and public Store registration for one native research test.
//! The market itself is synthetic and stays FIXTURE/PIT-UNVERIFIED throughout.
use super::{actual_probe, configured_transport, cycle_support, market, research_support, support};
use contracts::{
    artifacts::{ArtifactCreate, ResearchArtifactKind},
    brief::*,
    catalogs::{
        DataRevisionPolicy, NativeUniverseMemberV1, NativeUniverseV1, RuntimeCatalogMetadataV1,
    },
    codex::{CodexConnectionCreateV1, CodexProfileCreateV1, ProfileOrigin, SavedModelSettingsV1},
    control::ProjectCreate,
    data::*,
    evidence::{Comparator, MetricRequirementV1},
    execution::{NativeDataQualityReportV1, NativeDatasetQualityV1},
    execution_assumptions::ExecutionAssumptionsCreateV1,
    research::*,
    runs::RunKind,
    runtime::RuntimeDataKind,
    science::{NativeBarSelectionV1, NativeSimulationSettingsV1},
    settings::*,
    DbCounter, Id, SchemaV1,
};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use nautilus_model::instruments::Instrument;
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use server::runtime_transport::RuntimeTargets;
use sqlx::PgPool;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    sync::Arc,
};
use store::{authority::Actor, data_registration::RegistrationPreparation, Store, StoreError};

pub const REGISTRY: &str = "native-research-fixture";
pub const SOURCE: &str = r#"#![no_std]
#[panic_handler] fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
#[no_mangle] pub extern "C" fn predict(c:f64,p:f64,_f:f64,_s:f64,_v:f64,_o:f64,_h:f64,_l:f64)->f64 { c-p }
"#;

pub struct Prepared {
    pub store: Store,
    pub actor: Actor,
    pub data: cycle_support::Fixture,
    pub vault: Arc<SecretVault>,
    pub targets: RuntimeTargets,
    pub catalog: tempfile::TempDir,
}

fn instant(ns: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_nanos(ns.try_into().unwrap())
}

/// Bounded synchronous native work; SQLx keeps ownership of its current-thread runtime.
fn catalog() -> (
    tempfile::TempDir,
    Vec<RuntimeCatalogMetadataV1>,
    NativeSimulationSettingsV1,
) {
    let (seed, original) = market::market("0", 800);
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let all = job::catalog::load_catalog(seed.path(), &original.selection).unwrap();
    let end = 801 * market::INTERVAL_NS;
    let universe = NativeUniverseV1 {
        name: "Observed native fixture instruments".into(),
        calendar_ref: "synthetic-minute-calendar".into(),
        calendar_version: "1".into(),
        calendar_sessions: None,
        selection_asof: instant(0),
        has_historical_membership: false,
        coverage_start: instant(0),
        coverage_end: instant(end),
        membership: all
            .series
            .iter()
            .map(|series| NativeUniverseMemberV1 {
                instrument_id: series.instrument.id().to_string(),
                valid_from: instant(0),
                valid_until: None,
                available_at: instant(0),
                groups: None,
            })
            .collect(),
        instrument_definitions: all
            .series
            .iter()
            .map(|series| serde_json::to_value(&series.instrument).unwrap())
            .collect(),
    };
    let mut versions = Vec::new();
    for (index, partition) in [
        DataPartition::Discovery,
        DataPartition::Validation,
        DataPartition::Sealed,
        DataPartition::Forward,
    ]
    .into_iter()
    .enumerate()
    {
        let first = index as u64 * 200 * market::INTERVAL_NS;
        let last = (index as u64 + 1) * 200 * market::INTERVAL_NS;
        let selection = NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: original.selection.bar_types.clone(),
            event_start_ns: support::count(first),
            event_end_ns: support::count(last),
            decision_cutoff_ns: support::count(last),
            maximum_rows: 400,
        };
        let selected = job::catalog::load_catalog(seed.path(), &selection).unwrap();
        let root = directory.path().join(format!("window-{index}"));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        let native =
            ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, Some(16), None, None)
                .unwrap();
        native
            .write_instruments(
                selected
                    .series
                    .iter()
                    .map(|series| series.instrument.clone())
                    .collect(),
            )
            .unwrap();
        for series in &selected.series {
            native
                .write_to_parquet(&series.bars, None, None, None)
                .unwrap();
        }
        // Each authorized mount contains only its own partition, not other windows.
        let observed = job::catalog::load_catalog(&root, &selection).unwrap();
        assert_eq!(observed.rows, selected.rows);
        assert!(!observed.series.is_empty() && observed.rows > 0);
        let first_event = observed
            .series
            .iter()
            .flat_map(|series| &series.bars)
            .map(|bar| bar.ts_event.as_u64())
            .min()
            .unwrap();
        let last_event = observed
            .series
            .iter()
            .flat_map(|series| &series.bars)
            .map(|bar| bar.ts_event.as_u64())
            .max()
            .unwrap();
        let last_available = observed
            .series
            .iter()
            .flat_map(|series| &series.bars)
            .map(|bar| bar.ts_init.as_u64())
            .max()
            .unwrap();
        let metadata = RuntimeCatalogMetadataV1 {
            schema_version: SchemaV1,
            registered_ref: REGISTRY.into(),
            native_snapshot_ref: format!("native-window-{index}"),
            storage_version: format!("window-{index}-v1"),
            provider_kind: "NAUTILUS_CATALOG".into(),
            data_kind: RuntimeDataKind::Bar,
            partition,
            event_start: instant(first),
            event_end: instant(last),
            available_through: instant(last),
            row_count: support::count(observed.rows as u64),
            origin: DataOrigin::Fixture,
            pit_status: PitStatus::Unverified,
            revision_policy: DataRevisionPolicy::AsKnownThen,
            provenance_reference: "Actual native Parquet fixture; no market provenance claim"
                .into(),
            availability_provenance: "Observed synthetic ts_init; not historical PIT attestation"
                .into(),
            universe: universe.clone(),
            quality: NativeDataQualityReportV1 {
                schema_version: SchemaV1,
                native_version: "nautilus-persistence/0.63.0".into(),
                checked_at: runtime::now(),
                datasets: vec![NativeDatasetQualityV1 {
                    dataset_revision_id: Id::new(),
                    selection,
                    row_count: support::count(observed.rows as u64),
                    instrument_ids: observed
                        .series
                        .iter()
                        .map(|series| series.instrument.id().to_string())
                        .collect(),
                    first_event_ns: support::count(first_event),
                    last_event_ns: support::count(last_event),
                    available_through_ns: support::count(last_available),
                    last_bar_notionals: None,
                }],
            },
        };
        domain::catalogs::metadata(&metadata, runtime::now()).unwrap();
        versions.push(metadata);
    }
    (directory, versions, original.settings)
}

pub async fn read(
    objects: Arc<ArtifactStore>,
    id: Id,
    size: DbCounter,
) -> Result<Vec<u8>, StoreError> {
    tokio::task::spawn_blocking(move || objects.read(id, size).map_err(|_| StoreError::Integrity))
        .await
        .map_err(|_| StoreError::Integrity)?
}

pub async fn upload(
    store: &Store,
    actor: &Actor,
    objects: &ArtifactStore,
    project: Id,
    kind: ResearchArtifactKind,
    content: String,
) -> Id {
    let request = ArtifactCreate {
        schema_version: SchemaV1,
        project_id: project,
        kind,
        content,
    };
    let prepared = store
        .prepare_artifact_upload(actor, &Id::new().to_string(), &request)
        .await
        .unwrap();
    objects
        .put(prepared.id(), request.content.as_bytes())
        .unwrap();
    prepared.publish().await.unwrap().resource.id
}

pub async fn prepare(pool: &PgPool, remote: &mut support::Fixture) -> Prepared {
    let (catalog, versions, settings) = tokio::task::spawn_blocking(catalog).await.unwrap();
    remote.crash();
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&remote.config_path).unwrap()).unwrap();
    let mut registries = Vec::new();
    for (index, metadata) in versions.iter().enumerate() {
        let path = remote
            .directory
            .path()
            .join(format!("research-catalog-{index}.json"));
        fs::write(&path, serde_json::to_vec(metadata).unwrap()).unwrap();
        registries.push(serde_json::json!({"root":catalog.path().join(format!("window-{index}")),"metadata_file":path}));
    }
    config["catalogs"] = serde_json::Value::Array(registries);
    fs::write(&remote.config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    remote.restart().await;

    let root = tempfile::tempdir().unwrap();
    for name in ["native", "workspaces", "secrets"] {
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.path().join(name))
            .unwrap();
    }
    let key = root.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let vault = Arc::new(SecretVault::open(&root.path().join("secrets"), &key).unwrap());
    let credential = vault.put("RUNTIME", support::SECRET.as_bytes()).unwrap();
    let objects = Arc::new(ArtifactStore::open(&root.path().join("objects")).unwrap());
    let targets = RuntimeTargets::new(
        vec![server::runtime_transport::RuntimeTarget {
            origin: remote.origin.as_str().trim_end_matches('/').to_owned(),
            addresses: vec![remote.config.bind],
        }],
        true,
    )
    .unwrap();
    let (store, actor) = research_support::operator(pool).await;
    let project = store
        .create_project(
            &actor,
            "native-science-project",
            &ProjectCreate {
                schema_version: SchemaV1,
                name: "Native scientific feedback".into(),
                description: "Real computation on an explicitly synthetic catalog".into(),
                fork_from_project_id: None,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let proof = upload(&store, &actor, &objects, project, ResearchArtifactKind::Report,
        r#"{"schema_version":1,"license":"Locally generated synthetic test data; research only, not market evidence."}"#.into()).await;
    let checking = vault.clone();
    let integration = store
        .create_runtime(
            &actor,
            "native-science-runtime",
            &RuntimeCreate {
                schema_version: SchemaV1,
                configuration: RuntimeConfigurationV1 {
                    name: "Test-owned actual Runtime".into(),
                    endpoint: remote.origin.as_str().trim_end_matches('/').to_owned(),
                    tls_policy: TlsPolicy::SystemCa,
                    allowed_capabilities: vec![
                        RunKind::DataValidate,
                        RunKind::AlphaEvaluate,
                        RunKind::PortfolioSimulate,
                    ],
                    enabled: true,
                    development_http: true,
                },
                credential_ref: credential,
                ca_certificate_ref: None,
            },
            move |bindings| {
                let vault = checking.clone();
                async move {
                    tokio::task::spawn_blocking(move || {
                        for binding in bindings {
                            vault
                                .read(binding.id, binding.purpose.code())
                                .map_err(|_| StoreError::Integrity)?;
                        }
                        Ok(())
                    })
                    .await
                    .map_err(|_| StoreError::Integrity)?
                }
            },
        )
        .await
        .unwrap()
        .resource;
    actual_probe(
        &store,
        &actor,
        (integration.id, integration.revision),
        objects.clone(),
        vault.clone(),
        &targets,
    )
    .await;
    let source = store
        .create_data_source(
            &actor,
            "native-science-source",
            &DataSourceCreate {
                schema_version: SchemaV1,
                name: "Actual Parquet fixture".into(),
                runtime_id: integration.id,
                native_catalog_ref: REGISTRY.into(),
                provider_kind: DataProviderKind::NautilusCatalog,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let grant = store
        .create_data_grant(
            &actor,
            "native-science-grant",
            &DataGrantCreate {
                schema_version: SchemaV1,
                source_id: source.id,
                license_reference: "Locally authored synthetic fixture, research only".into(),
                evidence_artifact_id: proof,
                allowed_uses: DataUse::Research,
                valid_from: runtime::now() - chrono::Duration::hours(1),
                valid_until: None,
            },
        )
        .await
        .unwrap()
        .resource;
    let mut universe = None;
    let mut datasets = Vec::new();
    for metadata in &versions {
        let request = DatasetRegister {
            schema_version: SchemaV1,
            source_id: source.id,
            grant_id: grant.id,
            expected_source_revision: source.revision,
            expected_runtime_revision: integration.revision,
            native_storage_version: metadata.storage_version.clone(),
            existing_universe_version_id: universe,
        };
        let RegistrationPreparation::Execute(ticket) = store
            .prepare_dataset_registration(&actor, &Id::new().to_string(), &request)
            .await
            .unwrap()
        else {
            panic!("new dataset observation expected");
        };
        let transport =
            configured_transport(&ticket.runtime_snapshot, vault.clone(), &targets).await;
        let received = transport
            .catalog_metadata(
                &ticket.source.native_catalog_ref,
                &ticket.request.native_storage_version,
            )
            .await
            .unwrap();
        assert_eq!(received.metadata.row_count, metadata.row_count);
        assert_eq!(received.metadata.origin, DataOrigin::Fixture);
        let reading = objects.clone();
        let publishing = objects.clone();
        let registered = store
            .complete_dataset_registration(
                *ticket,
                received.raw_document,
                move |id, size| read(reading.clone(), id, size),
                move |batch| async move {
                    tokio::task::spawn_blocking(move || {
                        for item in batch {
                            publishing
                                .put(item.id, &item.bytes)
                                .map_err(|_| StoreError::Integrity)?;
                        }
                        Ok(())
                    })
                    .await
                    .map_err(|_| StoreError::Integrity)?
                },
            )
            .await
            .unwrap()
            .resource;
        assert_eq!(registered.pit_status, PitStatus::Unverified);
        universe = Some(registered.universe_version_id);
        datasets.push(registered.id);
    }
    let mut inputs = Vec::new();
    for (index, purpose) in [
        InputPurpose::Discovery,
        InputPurpose::Validation,
        InputPurpose::Sealed,
    ]
    .into_iter()
    .enumerate()
    {
        let input = store
            .create_input_set(
                &actor,
                &Id::new().to_string(),
                &InputSetCreate {
                    schema_version: SchemaV1,
                    project_id: project,
                    purpose,
                    decision_cutoff: versions[2].available_through,
                    items: vec![InputItemV1::Dataset {
                        dataset_revision_id: datasets[index],
                        role: versions[index].partition,
                    }],
                },
            )
            .await
            .unwrap()
            .resource
            .header
            .id;
        inputs.push(input);
    }
    let reading = objects.clone();
    let publishing = objects.clone();
    let assumption = store
        .create_execution_assumptions(
            &actor,
            "native-science-assumptions",
            &ExecutionAssumptionsCreateV1 {
                schema_version: SchemaV1,
                project_id: project,
                runtime_id: integration.id,
                expected_runtime_revision: integration.revision,
                input_set_id: inputs[0],
                dataset_revision_id: datasets[0],
                settlement_rule_ref: "synthetic-fixture-settlement".into(),
                settings,
                bar_liquidity: None,
                rolling_liquidity: None,
            },
            move |id, size| read(reading.clone(), id, size),
            move |object| {
                let objects = publishing.clone();
                async move {
                    tokio::task::spawn_blocking(move || objects.put(object.id, &object.bytes))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let mut policy: EvaluationPolicyCreate = serde_json::from_str(include_str!(
        "../../../../tests/contracts/research-policy.json"
    ))
    .unwrap();
    policy.project_id = project;
    policy.comparison_input_set_id = inputs[1];
    policy.execution_assumptions_id = assumption;
    policy.require_real_data = false;
    policy.required_capabilities.clear();
    policy.split_policy.sealed_revision_id = datasets[2];
    policy.split_policy.label_horizon_observations = Some(support::count(5));
    policy.split_policy.purge_observations = support::count(5);
    policy.selection.metric_code = "PEARSON_IC".into();
    policy.selection.metric_scope = "asset:0/fold:0".into();
    policy.selection.method_id = "ndarray-stats.pearson_correlation".into();
    policy.selection.method_version = "0.7.0".into();
    policy.selection.unit = "CORRELATION".into();
    policy.selection.frequency = "1-MINUTE-LAST-EXTERNAL;horizon=5".into();
    policy.selection.candidate_count = 1;
    policy.metric_requirements = vec![MetricRequirementV1 {
        schema_version: SchemaV1,
        metric_code: "PEARSON_IC".into(),
        scope: "asset:0/fold:0".into(),
        comparator: Comparator::Ge,
        threshold_low: Some("0.1".parse().unwrap()),
        threshold_high: None,
        required: true,
        minimum_observations: support::count(2),
        method_allowlist: vec!["ndarray-stats.pearson_correlation".into()],
    }];
    policy.sealed_metric_requirements = vec![MetricRequirementV1 {
        scope: "asset:0".into(),
        threshold_low: Some("0.2".parse().unwrap()),
        ..policy.metric_requirements[0].clone()
    }];
    let policy = store
        .create_evaluation_policy(&actor, "native-science-policy", &policy)
        .await
        .unwrap()
        .resource;
    let mut request: BriefCreate = serde_json::from_str(include_str!(
        "../../../../tests/contracts/research-brief.json"
    ))
    .unwrap();
    request.content.universe_version_id = universe.unwrap();
    request.content.execution_assumptions_id = assumption;
    request.content.evaluation_policy_id = policy.id;
    // Explicitly smaller than the generic contract fixture and within this Runtime's
    // advertised bound; no production limit or failed-test threshold is relaxed.
    request.content.budget.max_wall_seconds = 120;
    request.bindings = vec![
        BriefBindingV1 {
            dataset_revision_id: datasets[0],
            role: DataPartition::Discovery,
            access_policy: DataAccess::ResearchRead,
        },
        BriefBindingV1 {
            dataset_revision_id: datasets[1],
            role: DataPartition::Validation,
            access_policy: DataAccess::ResearchRead,
        },
        BriefBindingV1 {
            dataset_revision_id: datasets[2],
            role: DataPartition::Sealed,
            access_policy: DataAccess::EvaluatorOnly,
        },
    ];
    let brief = store
        .create_brief(
            &actor,
            "native-science-brief",
            &BriefCreateIntent {
                schema_version: SchemaV1,
                project_id: project,
                request,
            },
        )
        .await
        .unwrap()
        .resource;
    let profile = store
        .create_codex_profile(
            &actor,
            "native-science-profile",
            &CodexProfileCreateV1 {
                schema_version: SchemaV1,
                name: "Native account-waived Responses fixture".into(),
                home_binding: format!("native-{}", Id::new()),
                profile_origin: ProfileOrigin::ManagedVolume,
                connection: CodexConnectionCreateV1::System {},
                model_settings: SavedModelSettingsV1 {
                    schema_version: SchemaV1,
                    use_default_model_settings: true,
                    saved_model: None,
                    saved_reasoning_effort: None,
                    saved_fast_mode: false,
                },
            },
            |binding| async move {
                domain::codex::settings::home_binding(&binding.home_binding)?;
                Ok(())
            },
        )
        .await
        .unwrap()
        .resource;
    let choice = CodexProfileChoiceV1 {
        profile_id: profile.id,
        expected_revision: profile.revision,
    };
    let data = cycle_support::Fixture {
        data: research_support::ResearchFixture {
            project,
            artifact: proof,
            universe: universe.unwrap(),
            assumptions: assumption,
            runtime: integration.id,
            source: source.id,
            grant: grant.id,
            discovery: datasets[0],
            validation: datasets[1],
            sealed: datasets[2],
            forward: datasets[3],
        },
        freeze: BriefFreezeV1 {
            schema_version: SchemaV1,
            expected_revision: brief.revision,
            execution_context: BriefExecutionContextV1 {
                schema_version: SchemaV1,
                runtime_id: integration.id,
                runtime_revision: integration.revision,
                discovery_input_set_id: inputs[0],
                validation_input_set_id: inputs[1],
                sealed_input_set_id: inputs[2],
            },
        },
        brief,
        objects,
        researcher_profile: choice,
        reviewer_profile: choice,
        directory: Some(root),
    };
    Prepared {
        store,
        actor,
        data,
        vault,
        targets,
        catalog,
    }
}
