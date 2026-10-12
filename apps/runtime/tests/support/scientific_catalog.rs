//! Actual Parquet observation and public Store registration for one native research test.
//! The market itself is synthetic and stays FIXTURE/PIT-UNVERIFIED throughout.
use super::{actual_probe, configured_transport, cycle_support, research_support, support};
use contracts::{
    artifacts::{ArtifactCreate, ResearchArtifactKind},
    brief::*,
    catalogs::RuntimeCatalogMetadataV1,
    control::ProjectCreate,
    cycles::{BriefExecutionContextV1, BriefFreezeV1},
    data::*,
    evidence::{Comparator, MetricRequirementV1},
    execution_assumptions::ExecutionAssumptionsCreateV1,
    research::*,
    runs::RunKind,
    science::NativeSimulationSettingsV1,
    settings::*,
    DbCounter, Id, SchemaV1,
};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use server::runtime_transport::RuntimeTargets;
use sqlx::PgPool;
use std::{fs, os::unix::fs::DirBuilderExt, sync::Arc};
use store::{authority::Actor, data_registration::RegistrationPreparation, Store, StoreError};

pub const REGISTRY: &str = "native-research-fixture";

pub struct Prepared {
    pub store: Store,
    pub actor: Actor,
    pub data: cycle_support::Fixture,
    pub vault: Arc<SecretVault>,
    pub targets: RuntimeTargets,
    // Own the registered source directory for the entire native test.
    pub _catalog: tempfile::TempDir,
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

/// Paper permission is an explicit test input, frozen before dataset registration.
pub async fn prepare_catalog_with_paper(
    pool: &PgPool,
    remote: &mut support::Fixture,
    (catalog, versions, settings): (
        tempfile::TempDir,
        Vec<RuntimeCatalogMetadataV1>,
        NativeSimulationSettingsV1,
    ),
    external_split: Option<SplitPolicyV1>,
    paper: bool,
) -> Prepared {
    let external = external_split.is_some();
    let base_currency = settings.base_currency.clone();
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
        if paper {
            r#"{"schema_version":1,"license":"Locally generated synthetic data for research and Paper engineering acceptance; not market evidence."}"#.into()
        } else {
            r#"{"schema_version":1,"license":"Locally generated synthetic test data; research only, not market evidence."}"#.into()
        }).await;
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
                    allowed_capabilities: if paper {
                        vec![
                            RunKind::DataValidate,
                            RunKind::AlphaEvaluate,
                            RunKind::PortfolioBuild,
                            RunKind::PortfolioSimulate,
                        ]
                    } else {
                        vec![
                            RunKind::DataValidate,
                            RunKind::AlphaEvaluate,
                            RunKind::PortfolioSimulate,
                        ]
                    },
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
                license_reference: if paper {
                    "Locally authored synthetic fixture for research and Paper acceptance"
                } else {
                    "Locally authored synthetic fixture, research only"
                }
                .into(),
                evidence_artifact_id: proof,
                allowed_uses: if paper {
                    DataUse::ResearchAndPaper
                } else {
                    DataUse::Research
                },
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
    if let Some(mut split) = external_split {
        split.sealed_revision_id = datasets[2];
        policy.split_policy = split;
        policy.selection.frequency = "1-SECOND-LAST-EXTERNAL;horizon=2".into();
    }
    let horizon = policy.split_policy.label_horizon_observations.unwrap();
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
    request.content.base_currency = base_currency;
    request.content.horizon_value = Some(horizon);
    if external {
        request.content.budget.max_output_bytes = Some(support::count(4 * 1024 * 1024));
    }
    request.content.execution_assumptions_id = assumption;
    request.content.evaluation_policy_id = policy.id;
    // Explicitly smaller than the generic contract fixture and within this Runtime's
    // advertised bound; no production limit or failed-test threshold is relaxed.
    request.content.budget.max_wall_seconds = Some(120);
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
        directory: Some(root),
    };
    Prepared {
        store,
        actor,
        data,
        vault,
        targets,
        _catalog: catalog,
    }
}
