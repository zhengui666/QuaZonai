//! Mandatory native Docker acceptance. This target is selected explicitly by the
//! native-runtime CI; no test is ignored and missing native prerequisites are failures.
#[path = "../../../tests/support/catalog_metadata.rs"]
mod catalog_fixture;
#[path = "../../job/tests/support/market.rs"]
mod market;
#[path = "../../job/tests/support/polymarket.rs"]
mod polymarket;
#[path = "support/oci.rs"]
mod support;
use bollard::{
    errors::Error as DockerError,
    models::ContainerCreateBody,
    query_parameters::{CreateContainerOptionsBuilder, RemoveContainerOptionsBuilder},
};
use contracts::{runtime_jobs::*, Id, SchemaV1};
use reqwest::{Method, StatusCode};
use runtime::engine::{NativeEngine, NativeImage};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, time::Duration};
use support::{count, docker, Fixture, SIGNAL, SLOW_SIGNAL};

#[tokio::test]
async fn native_capabilities_exclude_binary_options_without_hiding_supported_markets() {
    use contracts::runtime::{RuntimeCapabilitiesV1, RuntimeDataKind};
    let mut f = Fixture::open().await;
    let catalog = f.directory.path().join("catalog");
    fs::create_dir(&catalog).unwrap();
    let metadata_path = f.directory.path().join("catalog-metadata.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.config_path).unwrap()).unwrap();
    config["catalogs"] = serde_json::json!([{"root":catalog,"metadata_file":metadata_path}]);
    // Only capability advertisement is under test: metadata is explicitly synthetic,
    // and an empty directory must never be reported as a successful scientific run.
    for class in ["CurrencyPair", "BinaryOption", "Equity"] {
        f.crash();
        let mut metadata = catalog_fixture::metadata();
        metadata.universe.instrument_definitions = vec![serde_json::json!({
            class: {"id": "EUR/USD.SIM", "fixture_only": true}
        })];
        domain::catalogs::metadata(&metadata, runtime::now()).unwrap();
        fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
        fs::write(&f.config_path, serde_json::to_vec(&config).unwrap()).unwrap();
        f.restart().await;
        let capabilities: RuntimeCapabilitiesV1 = f
            .json(Method::GET, &["capabilities"], None, &[StatusCode::OK])
            .await;
        domain::runtime::capabilities(&capabilities, runtime::now()).unwrap();
        assert!(!capabilities.image_refs.is_empty());
        if class == "BinaryOption" {
            assert!(capabilities.venues.is_empty());
        } else {
            assert_eq!(capabilities.venues.len(), 1);
            let venue = &capabilities.venues[0];
            assert_eq!(venue.venue, "SIM");
            assert_eq!(venue.instrument_classes, [class]);
            assert_eq!(venue.data_kinds, [RuntimeDataKind::Bar]);
            assert!(!venue.expiry_and_settlement);
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_candidate_simulation_consumes_original_target_and_settings_files() {
    native_candidate_simulation(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_portfolio_sequence_consumes_all_original_target_files() {
    native_candidate_simulation(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_rolling_study_uses_original_models_in_one_account() {
    native_rolling_study(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_polymarket_study_preserves_collateral_and_calendar_year_metrics() {
    native_rolling_study(true).await;
}

async fn native_rolling_study(prediction: bool) {
    use contracts::{
        execution::NativeTaskParametersV1,
        research::{ArtifactInputRole, DataPartition},
        science::NativePortfolioStudyResultV1,
        Revision,
    };
    let (mut catalog, mut request, wasm) = market::study_liquidity("10000000", "0.4");
    if prediction {
        // Only source types change. The actual Runtime, OCI, original Wasm models,
        // rolling liquidity, shared account and output-binding checks remain identical.
        catalog = tempfile::tempdir().unwrap();
        let expiration = 4 * 86_400_000_000_000;
        polymarket::write_catalog(catalog.path(), 2 * 1440 + 20, "0", expiration, true);
        polymarket::settings(&mut request.execution_settings, "0", expiration);
        request.mandate.base_currency = "pUSD".into();
        request.source_selection.bar_types = polymarket::IDS
            .iter()
            .map(|id| format!("{id}-1-MINUTE-LAST-EXTERNAL"))
            .collect();
        for (asset, fee) in request
            .assets
            .iter_mut()
            .zip(&request.execution_settings.fee_rates)
        {
            asset.instrument_id = fee.instrument_id.clone();
            asset.currency = "pUSD".into();
            asset.transaction_cost_rate = fee.taker.clone();
        }
    }
    market::calendar_schedule(&mut request);
    let dataset = Id::new();
    let selection = &request.source_selection;
    let observed = job::catalog::load_catalog(catalog.path(), selection).unwrap();
    let mut metadata = catalog_fixture::metadata();
    let calendar = request.calendar.as_ref().unwrap();
    metadata.universe.calendar_ref = calendar.calendar.calendar_ref.clone();
    metadata.universe.calendar_version = calendar.calendar.calendar_version.clone();
    metadata.universe.calendar_sessions = Some(calendar.calendar.clone());
    metadata.partition = DataPartition::Validation;
    metadata.event_start =
        chrono::DateTime::from_timestamp_nanos(selection.event_start_ns.get() as i64);
    metadata.event_end =
        chrono::DateTime::from_timestamp_nanos(selection.event_end_ns.get() as i64);
    metadata.available_through =
        chrono::DateTime::from_timestamp_nanos(selection.decision_cutoff_ns.get() as i64);
    metadata.row_count = count(observed.rows as u64);
    metadata.universe.coverage_end = metadata.event_end;
    metadata.universe.instrument_definitions = observed
        .series
        .iter()
        .map(|s| serde_json::to_value(&s.instrument).unwrap())
        .collect();
    let member = metadata.universe.membership[0].clone();
    metadata.universe.membership = request
        .assets
        .iter()
        .map(|a| {
            let mut member = member.clone();
            member.instrument_id = a.instrument_id.clone();
            member
        })
        .collect();
    metadata.quality.checked_at = runtime::now();
    let quality = &mut metadata.quality.datasets[0];
    quality.dataset_revision_id = dataset;
    quality.selection = selection.clone();
    quality.row_count = metadata.row_count;
    quality.instrument_ids = request
        .assets
        .iter()
        .map(|a| a.instrument_id.clone())
        .collect();
    quality.first_event_ns = count(observed.series[0].bars[0].ts_event.as_u64());
    let last = observed.series[0].bars.last().unwrap();
    quality.last_event_ns = count(last.ts_event.as_u64());
    quality.available_through_ns = count(last.ts_init.as_u64());
    domain::catalogs::metadata(&metadata, runtime::now()).unwrap();
    fs::set_permissions(catalog.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let mut f = Fixture::open().await;
    f.crash();
    let metadata_path = f.directory.path().join("study-metadata.json");
    fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.config_path).unwrap()).unwrap();
    config["catalogs"] = serde_json::json!([{"root":catalog.path(),"metadata_file":metadata_path}]);
    fs::write(&f.config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    f.restart().await;
    let mut objects = request
        .members
        .iter()
        .map(|m| (m.model_artifact_id, wasm.clone(), ArtifactInputRole::Model))
        .collect::<Vec<_>>();
    objects.push((
        request.mandate.constraints.transaction_costs_ref,
        serde_json::to_vec(&request.execution_settings).unwrap(),
        ArtifactInputRole::Parameters,
    ));
    let operation = NativeTaskParametersV1::StudyPortfolio {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        request: Box::new(request),
    };
    let NativeTaskParametersV1::StudyPortfolio { request, .. } = &operation else {
        unreachable!()
    };
    objects.push((
        request.mandate.constraints.liquidity_ref.unwrap(),
        serde_json::to_vec(request.rolling_liquidity.as_ref().unwrap()).unwrap(),
        ArtifactInputRole::Parameters,
    ));
    let calendar = request.calendar.as_ref().unwrap();
    objects.push((
        calendar.artifact_id,
        serde_json::to_vec(&calendar.calendar).unwrap(),
        ArtifactInputRole::Parameters,
    ));
    let parameters_id = Id::new();
    objects.push((
        parameters_id,
        serde_json::to_vec(&operation).unwrap(),
        ArtifactInputRole::Parameters,
    ));
    let mut inputs = vec![RuntimeInputV1::Dataset {
        revision_id: dataset,
        registered_ref: metadata.registered_ref,
        storage_version: metadata.storage_version,
        role: DataPartition::Validation,
    }];
    for (id, bytes, role) in objects {
        f.object(id, &bytes).await;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: count(bytes.len() as u64),
            role,
        });
    }
    let run = Id::new();
    f.runs.push(run);
    let spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run, 1).unwrap(),
        job_kind: operation.job_kind(),
        image_ref: support::image(),
        input_set_id: Id::new(),
        inputs,
        parameters_artifact_id: parameters_id,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: count(30),
            memory_mib: 512,
            wall_seconds: 30,
            output_bytes: count(8 * 1024 * 1024),
        },
        deadline_at: runtime::now() + chrono::Duration::seconds(50),
        requested_output_schemas: operation.output_schemas(),
    };
    for field in ["calendar_ref", "calendar_version", "sessions"] {
        let mut changed = operation.clone();
        let NativeTaskParametersV1::StudyPortfolio { request, .. } = &mut changed else {
            unreachable!()
        };
        let calendar = &mut request.calendar.as_mut().unwrap().calendar;
        if field == "calendar_ref" {
            calendar.calendar_ref = "FOREIGN".into();
            request.mandate.rebalance_schedule.calendar_ref = Some("FOREIGN".into());
        } else if field == "calendar_version" {
            calendar.calendar_version = "foreign-version".into();
        } else {
            calendar.sessions[1].close_ns = market::count(calendar.sessions[1].close_ns.get() + 1);
        }
        let id = Id::new();
        f.object(id, &serde_json::to_vec(&changed).unwrap()).await;
        let mut changed_spec = spec.clone();
        changed_spec.parameters_artifact_id = id;
        changed_spec.inputs.retain(|i| !matches!(i, RuntimeInputV1::Artifact { artifact_id, .. } if *artifact_id == parameters_id));
        domain::execution::task(&changed_spec, &changed).unwrap();
        let response = f
            .client
            .post(f.url(&["jobs"]))
            .json(&changed_spec)
            .send()
            .await
            .unwrap();
        assert!(
            response.status().is_client_error(),
            "foreign {field} admitted"
        );
    }
    let accepted = f.submit(&spec).await;
    assert_eq!(f.terminal(&spec).await.state, RuntimeJobState::Succeeded);
    let manifest = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&manifest, &spec, accepted.submitted_at, runtime::now())
        .unwrap();
    assert_eq!(manifest.engine_versions["portfolio-study"], "6");
    assert_eq!(manifest.engine_versions["portfolio-calendar"], "2");
    assert_eq!(manifest.engine_versions["portfolio-rolling-liquidity"], "1");
    assert_eq!(manifest.engine_versions["portfolio-build-rolling"], "1");
    assert_eq!(manifest.engine_versions["portfolio-history"], "1");
    if prediction {
        assert_eq!(manifest.engine_versions["polymarket-research"], "1");
        let capabilities: contracts::runtime::RuntimeCapabilitiesV1 = f
            .json(Method::GET, &["capabilities"], None, &[StatusCode::OK])
            .await;
        assert!(capabilities.venues.iter().any(|v| v.venue == "POLYMARKET"
            && v.instrument_classes.iter().any(|c| c == "BinaryOption")
            && v.expiry_and_settlement));
    }
    assert_eq!(manifest.artifacts.len(), 3);
    let mut outputs = Vec::new();
    for artifact in &manifest.artifacts {
        let response = f
            .client
            .get(f.url(&[
                "jobs",
                &spec.external_job_id,
                "artifacts",
                &artifact.storage_ref.to_string(),
            ]))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], artifact.media_type);
        outputs.push((artifact.clone(), response.bytes().await.unwrap().to_vec()));
    }
    domain::execution::output_bindings(
        &operation,
        None,
        manifest.started_at.unwrap(),
        manifest.finished_at,
        &outputs,
    )
    .unwrap();
    let report: NativePortfolioStudyResultV1 = serde_json::from_slice(
        &outputs
            .iter()
            .find(|(a, _)| a.schema.name == "qz.portfolio_study")
            .unwrap()
            .1,
    )
    .unwrap();
    assert_eq!(report.frames.len(), 3);
    assert!(report.frames.iter().all(|f| f.bar_notionals.len() == 2));
    assert_ne!(
        report.frames[0].input.capital_assumption,
        report.frames[1].input.capital_assumption
    );
    let simulation = report.simulation.unwrap();
    assert_eq!(simulation.consumed_target_points.get(), 3);
    assert_eq!(
        simulation.canonical_result["accounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        simulation.returns_status,
        contracts::evidence::MetricStatus::Ok
    );
    assert!(simulation.orders.get() > 0 && simulation.returns.len() >= 2);
    // Exercise the publisher's actual metric adapter with Docker output, not
    // controlled statistic declarations. This is still a synthetic catalog,
    // not Store qualification or a REAL/PIT scientific acceptance claim.
    let native_request = report.simulation_request.unwrap();
    let source = manifest
        .artifacts
        .iter()
        .find(|a| a.schema.name == "qz.portfolio_study")
        .unwrap()
        .storage_ref;
    let evaluation = Id::new();
    let (metrics, capabilities) = domain::execution::portfolio_simulation_metrics(
        evaluation,
        source,
        &native_request,
        &simulation,
    )
    .unwrap();
    assert_eq!(metrics.len(), 3);
    assert_eq!(capabilities.len(), 3);
    let period = if prediction { 365 } else { 252 };
    assert_eq!(metrics[0].annualization_factor, None);
    assert_eq!(metrics[1].annualization_factor, Some(f64::from(period)));
    assert_eq!(metrics[2].annualization_factor, Some(f64::from(period)));
    for (metric, native_key) in metrics.iter().zip([
        "Average (Return)".to_owned(),
        format!("Returns Volatility ({period} days)"),
        format!("Sharpe Ratio ({period} days)"),
    ]) {
        let statistic = simulation
            .statistics
            .iter()
            .find(|s| {
                s.group == contracts::science::NativeStatisticGroup::Returns
                    && s.native_key == native_key
            })
            .unwrap();
        assert!(statistic.value.is_some());
        assert_eq!(metric.value, statistic.value);
        assert_eq!(metric.status, contracts::evidence::MetricStatus::Ok);
        assert_eq!(metric.evaluation_id, evaluation);
        assert_eq!(metric.source_artifact_id, source);
        assert_eq!(metric.scope, "portfolio");
        assert_eq!(metric.method_version, "0.63.0");
        assert_eq!(
            metric.observation_count.get(),
            simulation.returns.len() as u64
        );
    }
    let mut foreign = native_request;
    foreign.settings.starting_capital = "1".parse().unwrap();
    assert!(domain::execution::portfolio_simulation_metrics(
        evaluation,
        source,
        &foreign,
        &simulation
    )
    .is_err());
}

async fn native_candidate_simulation(sequence: bool) {
    use contracts::{
        execution::NativeTaskParametersV1,
        research::{ArtifactInputRole, DataPartition},
        science::*,
        Revision,
    };
    // Controlled target declaration, not a Store Candidate or qualification claim.
    let (catalog, mut request) = market::market("0.001", 2 * 1440 + 20);
    if !sequence {
        request.target_points.truncate(1);
    }
    let one: contracts::DecimalValue = "1".parse().unwrap();
    for point in &mut request.target_points {
        point.targets.truncate(1);
        point.cash_weight = (one.as_decimal() - point.targets[0].weight.as_decimal())
            .to_plain_string()
            .parse()
            .unwrap();
    }
    request.settings.fee_rates.truncate(1);
    request.selection.bar_types.truncate(1);
    request.selection.event_start_ns = request.target_points[0].asof_ns;
    let dataset = Id::new();
    let candidate = Id::new();
    let point = &request.target_points[0];
    let target = PortfolioTargetsV1 {
        schema_version: SchemaV1,
        candidate_id: candidate,
        base_currency: request.settings.base_currency.clone(),
        asof: chrono::DateTime::from_timestamp_nanos(point.asof_ns.get() as i64),
        valid_until: chrono::DateTime::from_timestamp_nanos(point.valid_until_ns.get() as i64),
        cash_weight: point.cash_weight.clone(),
        targets: point.targets.clone(),
    };
    let observed = job::catalog::load_catalog(catalog.path(), &request.selection).unwrap();
    let mut metadata = catalog_fixture::metadata();
    metadata.partition = DataPartition::Forward;
    metadata.event_start = target.asof;
    metadata.event_end = target.valid_until;
    metadata.available_through = target.valid_until;
    metadata.row_count = count(observed.rows as u64);
    metadata.universe.coverage_end = target.valid_until;
    metadata.universe.instrument_definitions =
        vec![serde_json::to_value(&observed.series[0].instrument).unwrap()];
    metadata.quality.checked_at = runtime::now();
    let quality = &mut metadata.quality.datasets[0];
    quality.dataset_revision_id = dataset;
    quality.selection = request.selection.clone();
    quality.row_count = metadata.row_count;
    quality.first_event_ns = count(observed.series[0].bars[0].ts_event.as_u64());
    let last = observed.series[0].bars.last().unwrap();
    quality.last_event_ns = count(last.ts_event.as_u64());
    quality.available_through_ns = count(last.ts_init.as_u64());
    domain::catalogs::metadata(&metadata, runtime::now()).unwrap();
    fs::set_permissions(catalog.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let mut f = Fixture::open().await;
    f.crash();
    let metadata_path = f.directory.path().join("candidate-metadata.json");
    fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.config_path).unwrap()).unwrap();
    config["catalogs"] = serde_json::json!([{"root":catalog.path(),"metadata_file":metadata_path}]);
    fs::write(&f.config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    f.restart().await;
    let target_id = Id::new();
    let settings_id = Id::new();
    let parameters_id = Id::new();
    let targets = serde_json::to_vec(&target).unwrap();
    let settings = serde_json::to_vec(&request.settings).unwrap();
    let mut target_objects = vec![(target_id, targets)];
    let operation = if sequence {
        let mut later = target.clone();
        let point = &request.target_points[1];
        later.candidate_id = Id::new();
        later.asof = chrono::DateTime::from_timestamp_nanos(point.asof_ns.get() as i64);
        later.targets = point.targets.clone();
        later.cash_weight = point.cash_weight.clone();
        let later_id = Id::new();
        target_objects.push((later_id, serde_json::to_vec(&later).unwrap()));
        NativeTaskParametersV1::SimulatePortfolioSequence {
            schema_version: SchemaV1,
            dataset_revision_id: dataset,
            source_selection: request.selection.clone(),
            settings_artifact_id: settings_id,
            sources: vec![
                NativePortfolioTargetSourceV1 {
                    candidate_id: candidate,
                    candidate_available_ns: request.target_points[0].asof_ns,
                    target_artifact_id: target_id,
                },
                NativePortfolioTargetSourceV1 {
                    candidate_id: later.candidate_id,
                    candidate_available_ns: point.asof_ns,
                    target_artifact_id: later_id,
                },
            ],
            request: Box::new(request),
        }
    } else {
        NativeTaskParametersV1::SimulateCandidate {
            schema_version: SchemaV1,
            candidate_id: candidate,
            candidate_available_ns: request.target_points[0].asof_ns,
            dataset_revision_id: dataset,
            target_artifact_id: target_id,
            settings_artifact_id: settings_id,
            source_selection: request.selection.clone(),
            request: Box::new(request),
        }
    };
    let parameters = serde_json::to_vec(&operation).unwrap();
    let mut inputs = vec![RuntimeInputV1::Dataset {
        revision_id: dataset,
        registered_ref: metadata.registered_ref,
        storage_version: metadata.storage_version,
        role: DataPartition::Forward,
    }];
    for (id, bytes, role) in target_objects
        .iter()
        .map(|(id, bytes)| (*id, bytes, ArtifactInputRole::Report))
        .chain([
            (settings_id, &settings, ArtifactInputRole::Parameters),
            (parameters_id, &parameters, ArtifactInputRole::Parameters),
        ])
    {
        f.object(id, bytes).await;
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: count(bytes.len() as u64),
            role,
        });
    }
    let run = Id::new();
    f.runs.push(run);
    let mut spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run, 1).unwrap(),
        job_kind: operation.job_kind(),
        image_ref: support::image(),
        input_set_id: Id::new(),
        inputs,
        parameters_artifact_id: parameters_id,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: count(30),
            memory_mib: 512,
            wall_seconds: 30,
            output_bytes: count(4 * 1024 * 1024),
        },
        deadline_at: runtime::now() + chrono::Duration::seconds(50),
        requested_output_schemas: operation.output_schemas(),
    };
    if !sequence {
        // Corrupt only this test-owned catalog, then restore it for the positive run.
        // Exercise the real container and public error channel, not a parser mock.
        let native = nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
            catalog.path().to_str().unwrap(),
            None,
            Some(16),
            None,
            None,
        )
        .unwrap();
        let bar = observed.series[0].bars[0];
        let directory = native
            .make_path(
                <nautilus_model::data::Bar as nautilus_persistence::backend::catalog::CatalogPathPrefix>::path_prefix(),
                Some(&bar.bar_type.to_string()),
            )
            .unwrap();
        let files = native.list_parquet_files(&directory).unwrap();
        assert_eq!(files.len(), 1);
        let path = catalog.path().join(&files[0]);
        let original = fs::read(&path).unwrap();
        let mut corrupted = original.clone();
        let footer = corrupted.len() - 8;
        corrupted[footer..footer + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let compressed_directory = tempfile::tempdir().unwrap();
        let compressed_catalog =
            nautilus_persistence::backend::catalog::ParquetDataCatalog::from_uri(
                compressed_directory.path().to_str().unwrap(),
                None,
                Some(4096),
                None,
                None,
            )
            .unwrap();
        assert_eq!(format!("{:?}", compressed_catalog.compression), "SNAPPY");
        let bars: Vec<_> = (0..50_000_u64)
            .map(|offset| {
                let mut value = bar;
                value.ts_event = (bar.ts_event.as_u64() + offset).into();
                value.ts_init = (bar.ts_init.as_u64() + offset).into();
                value
            })
            .collect();
        let compressed_path = compressed_directory.path().join(
            compressed_catalog
                .write_to_parquet(&bars, None, None, None)
                .unwrap(),
        );
        let compressed = fs::read(compressed_path).unwrap();
        assert!(compressed.len() * 4 < std::mem::size_of_val(bars.as_slice()));
        // Both malformed metadata and bounded decompression must fail through OCI.
        for (bytes, expected_limit) in [(corrupted, false), (compressed, true)] {
            fs::write(&path, &bytes).unwrap();
            if expected_limit {
                let error = job::catalog::load_catalog(
                    catalog.path(),
                    &metadata.quality.datasets[0].selection,
                )
                .err()
                .unwrap();
                assert_eq!(error.to_string(), "CATALOG_ROW_LIMIT");
            }
            let mut invalid = spec.clone();
            invalid.deadline_at = runtime::now() + chrono::Duration::seconds(50);
            invalid.run_id = Id::new();
            invalid.external_job_id = domain::runtime_jobs::external_id(invalid.run_id, 1).unwrap();
            f.runs.push(invalid.run_id);
            let accepted = f.submit(&invalid).await;
            let terminal = f.terminal(&invalid).await;
            assert_eq!(terminal.state, RuntimeJobState::Failed);
            let failed = f.manifest(&invalid).await;
            domain::runtime_jobs::manifest(
                &failed,
                &invalid,
                accepted.submitted_at,
                runtime::now(),
            )
            .unwrap();
            assert!(failed.artifacts.is_empty());
            assert_eq!(failed.resource_usage.output_bytes.get(), 0);
            let error = failed.error.as_ref().unwrap();
            assert_eq!(error.code, RuntimeFailureCode::NativeJobFailed);
            assert_eq!(error.safe_message, "The native job process failed.");
            let container = f.native_container(&invalid).await;
            let state = container.state.unwrap();
            assert_eq!(state.exit_code, Some(1));
            assert_eq!(state.oom_killed, Some(false));
            assert_eq!(f.submit(&invalid).await, terminal);
            assert_eq!(fs::read(&path).unwrap(), bytes);
            f.assert_private_logs();
        }
        fs::write(path, original).unwrap();
        spec.deadline_at = runtime::now() + chrono::Duration::seconds(50);
    }
    let accepted = f.submit(&spec).await;
    assert_eq!(f.terminal(&spec).await.state, RuntimeJobState::Succeeded);
    let manifest = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&manifest, &spec, accepted.submitted_at, runtime::now())
        .unwrap();
    assert_eq!(manifest.engine_versions["candidate-simulation"], "2");
    if sequence {
        assert_eq!(manifest.engine_versions["portfolio-sequence"], "1");
    }
    assert_eq!(manifest.artifacts.len(), 2);
    let output = manifest
        .artifacts
        .iter()
        .find(|output| output.schema.name == "qz.native_simulation")
        .unwrap();
    let response = f
        .client
        .get(f.url(&[
            "jobs",
            &spec.external_job_id,
            "artifacts",
            &output.storage_ref.to_string(),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.bytes().await.unwrap().to_vec();
    let quality = manifest
        .artifacts
        .iter()
        .find(|output| output.schema.name == "qz.data_quality")
        .unwrap();
    let quality_response = f
        .client
        .get(f.url(&[
            "jobs",
            &spec.external_job_id,
            "artifacts",
            &quality.storage_ref.to_string(),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(quality_response.status(), StatusCode::OK);
    let quality_bytes = quality_response.bytes().await.unwrap().to_vec();
    domain::execution::output_bindings(
        &operation,
        None,
        manifest.started_at.unwrap(),
        manifest.finished_at,
        &[
            (output.clone(), bytes.clone()),
            (quality.clone(), quality_bytes),
        ],
    )
    .unwrap();
    let result: NativeSimulationResultV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        result.consumed_target_points.get(),
        if sequence { 2 } else { 1 }
    );
    assert_eq!(
        result.canonical_result["accounts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(result.orders.get() > 0);
    assert_eq!(result.returns_kind, NativeReturnsKind::PortfolioDaily);
    assert_eq!(result.returns_status, contracts::evidence::MetricStatus::Ok);
    assert!(result.returns_reason.is_none());
    assert!(result.returns.len() >= 2);
    assert!(result
        .returns
        .iter()
        .all(|r| r.value.is_some_and(f64::is_finite)));
    assert!(result
        .returns
        .iter()
        .any(|r| r.value.is_some_and(|v| v != 0.0)));
    assert_eq!(
        f.submit(&spec).await.external_job_id,
        accepted.external_job_id
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_portfolio_aggregates_original_forecasts_before_optimizing() {
    native_portfolio(false, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_cvar_portfolio_preserves_original_scenarios_and_confidence() {
    native_portfolio(true, false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_risk_budget_portfolio_rechecks_original_risk_contributions() {
    native_portfolio(false, true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_cvar_risk_budget_retains_original_tail_dual_witness() {
    native_portfolio(true, true).await;
}

async fn native_portfolio(cvar: bool, risk_budget: bool) {
    use contracts::{
        execution::NativeTaskParametersV1,
        portfolio::*,
        research::{ArtifactInputRole, DataPartition},
        science::NativePortfolioBuildResultV1,
        Revision,
    };
    let (catalog, mut request, wasm) = if cvar && risk_budget {
        market::portfolio_with_losses()
    } else {
        market::portfolio()
    };
    let second = market::module("f64.const 0.03");
    if !cvar && !risk_budget {
        let policy = contracts::science::NativeRollingBarLiquidityPolicyV1 {
            schema_version: SchemaV1,
            maximum_age_seconds: 3600,
            participation_limit: "0.4".parse().unwrap(),
        };
        request.mandate.constraints.liquidity_ref = Some(Id::new());
        request.mandate.constraints.max_participation = Some(policy.participation_limit.clone());
        request.rolling_liquidity = Some(policy);
        let NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
            &mut request.execution_settings.fill_model
        else {
            unreachable!()
        };
        parameters.prob_slippage = "0.5".parse().unwrap();
    }
    request.mandate.objective = AllocationObjective::MaxUtility;
    request.mandate.constraints.max_ex_ante_risk = Some("1".parse().unwrap());
    if cvar {
        request.mandate.risk_measure = AllocationRisk::Cvar;
        let NativeModelRefV1::ClarabelQp { parameters, .. } = &mut request.mandate.optimizer else {
            panic!("native optimizer")
        };
        parameters.cvar_confidence = Some("0.95".parse().unwrap());
    }
    request.members[0].ensemble_weight = "0.25".parse().unwrap();
    if risk_budget {
        request.mandate.objective = AllocationObjective::RiskBudgeting;
        let NativeModelRefV1::ClarabelQp { parameters, .. } = &mut request.mandate.optimizer else {
            unreachable!()
        };
        parameters.risk_budgeting = Some(RiskBudgetSettingsV1 {
            schema_version: SchemaV1,
            risky_gross_exposure: "1".parse().unwrap(),
            assets: request
                .assets
                .iter()
                .map(|a| RiskBudgetAssetV1 {
                    instrument_id: a.instrument_id.clone(),
                    share: "0.5".parse().unwrap(),
                    sign: RiskBudgetSign::Long,
                })
                .collect(),
        });
    }
    request.members[1].ensemble_weight = "0.75".parse().unwrap();
    let model_ids = request
        .members
        .iter()
        .map(|m| m.model_artifact_id)
        .collect::<Vec<_>>();
    let dataset = Id::new();
    let observed = job::catalog::load_catalog(catalog.path(), &request.selection).unwrap();
    let mut metadata = catalog_fixture::metadata();
    metadata.partition = DataPartition::Forward;
    metadata.event_start =
        chrono::DateTime::from_timestamp_nanos(request.selection.event_start_ns.get() as i64);
    metadata.event_end =
        chrono::DateTime::from_timestamp_nanos(request.selection.event_end_ns.get() as i64);
    metadata.available_through = metadata.event_end;
    metadata.row_count = count(observed.rows as u64);
    metadata.universe.coverage_end = metadata.event_end;
    metadata.universe.membership = request
        .assets
        .iter()
        .map(|asset| {
            let mut member = metadata.universe.membership[0].clone();
            member.instrument_id = asset.instrument_id.clone();
            member
        })
        .collect();
    metadata.universe.instrument_definitions = request.assets.iter().map(|asset| serde_json::json!({"CurrencyPair":{"id":asset.instrument_id,"fixture_only":true}})).collect();
    metadata.quality.checked_at = runtime::now();
    let quality = &mut metadata.quality.datasets[0];
    quality.dataset_revision_id = dataset;
    quality.selection = request.selection.clone();
    quality.row_count = metadata.row_count;
    quality.instrument_ids = request
        .assets
        .iter()
        .map(|a| a.instrument_id.clone())
        .collect();
    quality.first_event_ns = count(
        observed
            .series
            .iter()
            .flat_map(|s| &s.bars)
            .map(|b| b.ts_event.as_u64())
            .min()
            .unwrap(),
    );
    quality.last_event_ns = count(
        observed
            .series
            .iter()
            .flat_map(|s| &s.bars)
            .map(|b| b.ts_event.as_u64())
            .max()
            .unwrap(),
    );
    quality.available_through_ns = count(
        observed
            .series
            .iter()
            .flat_map(|s| &s.bars)
            .map(|b| b.ts_init.as_u64())
            .max()
            .unwrap(),
    );
    domain::catalogs::metadata(&metadata, runtime::now()).unwrap();
    request.mandate.constraints.group_bounds = vec![GroupBoundV1 {
        group_id: "fixture-group".into(),
        min: "0".parse().unwrap(),
        max: "1".parse().unwrap(),
    }];
    let groups = domain::catalogs::portfolio_groups(
        &metadata.universe,
        &request
            .assets
            .iter()
            .map(|asset| asset.instrument_id.clone())
            .collect::<Vec<_>>(),
        &request.mandate.constraints.group_bounds,
        request.selection.decision_cutoff_ns,
    )
    .unwrap();
    for (asset, groups) in request.assets.iter_mut().zip(groups) {
        asset.groups = groups;
    }
    fs::set_permissions(catalog.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let mut f = Fixture::open().await;
    f.crash();
    let metadata_path = f.directory.path().join("portfolio-metadata.json");
    fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.config_path).unwrap()).unwrap();
    config["catalogs"] = serde_json::json!([{"root":catalog.path(),"metadata_file":metadata_path}]);
    fs::write(&f.config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    f.restart().await;
    f.object(model_ids[0], &wasm).await;
    f.object(model_ids[1], &second).await;
    let weights_id = request.current_weights_artifact_id;
    let weights = serde_json::to_vec(&request.current_weights).unwrap();
    f.object(weights_id, &weights).await;
    let cost_id = request.mandate.constraints.transaction_costs_ref;
    let costs = serde_json::to_vec(&request.execution_settings).unwrap();
    f.object(cost_id, &costs).await;
    let rolling_input = if let Some(policy) = &request.rolling_liquidity {
        let id = request.mandate.constraints.liquidity_ref.unwrap();
        let bytes = serde_json::to_vec(policy).unwrap();
        f.object(id, &bytes).await;
        Some(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: count(bytes.len() as u64),
            role: ArtifactInputRole::Parameters,
        })
    } else {
        None
    };
    let operation = NativeTaskParametersV1::BuildPortfolio {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        request: Box::new(request),
    };
    let bytes = serde_json::to_vec(&operation).unwrap();
    let parameters = Id::new();
    f.object(parameters, &bytes).await;
    let run = Id::new();
    f.runs.push(run);
    let spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run, 1).unwrap(),
        job_kind: operation.job_kind(),
        image_ref: support::image(),
        input_set_id: Id::new(),
        inputs: vec![
            RuntimeInputV1::Artifact {
                artifact_id: cost_id,
                storage_version: "1".into(),
                byte_count: count(costs.len() as u64),
                role: ArtifactInputRole::Parameters,
            },
            RuntimeInputV1::Artifact {
                artifact_id: weights_id,
                storage_version: "1".into(),
                byte_count: count(weights.len() as u64),
                role: ArtifactInputRole::Report,
            },
            RuntimeInputV1::Dataset {
                revision_id: dataset,
                registered_ref: metadata.registered_ref,
                storage_version: metadata.storage_version,
                role: DataPartition::Forward,
            },
            RuntimeInputV1::Artifact {
                artifact_id: model_ids[0],
                storage_version: "1".into(),
                byte_count: count(wasm.len() as u64),
                role: ArtifactInputRole::Model,
            },
            RuntimeInputV1::Artifact {
                artifact_id: model_ids[1],
                storage_version: "1".into(),
                byte_count: count(second.len() as u64),
                role: ArtifactInputRole::Model,
            },
            RuntimeInputV1::Artifact {
                artifact_id: parameters,
                storage_version: "1".into(),
                byte_count: count(bytes.len() as u64),
                role: ArtifactInputRole::Parameters,
            },
        ]
        .into_iter()
        .chain(rolling_input)
        .collect(),
        parameters_artifact_id: parameters,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: count(30),
            memory_mib: 512,
            wall_seconds: 30,
            output_bytes: count(4 * 1024 * 1024),
        },
        deadline_at: runtime::now() + chrono::Duration::seconds(50),
        requested_output_schemas: operation.output_schemas(),
    };
    let admitted = f.submit(&spec).await;
    assert_eq!(f.terminal(&spec).await.state, RuntimeJobState::Succeeded);
    let manifest = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&manifest, &spec, admitted.submitted_at, runtime::now())
        .unwrap();
    assert_eq!(manifest.engine_versions["portfolio-ensemble"], "1");
    assert_eq!(manifest.engine_versions["portfolio-models"], "4");
    assert_eq!(manifest.engine_versions["simulation-models"], "1");
    assert_eq!(manifest.engine_versions["portfolio-weights"], "1");
    assert_eq!(manifest.engine_versions["portfolio-variance-bound"], "1");
    assert_eq!(manifest.engine_versions["portfolio-cvar"], "1");
    assert_eq!(manifest.engine_versions["portfolio-risk-budget"], "1");
    assert_eq!(manifest.engine_versions["portfolio-cvar-risk-budget"], "1");
    assert_eq!(manifest.engine_versions["ndarray"], "0.17.1");
    let [output] = manifest.artifacts.as_slice() else {
        panic!("one original allocation report");
    };
    assert_eq!(output.schema.name, "qz.native_portfolio");
    let response = f
        .client
        .get(f.url(&[
            "jobs",
            &spec.external_job_id,
            "artifacts",
            &output.storage_ref.to_string(),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.bytes().await.unwrap().to_vec();
    domain::execution::output_bindings(
        &operation,
        None,
        manifest.started_at.unwrap(),
        manifest.finished_at,
        &[(output.clone(), bytes.clone())],
    )
    .unwrap();
    let result: NativePortfolioBuildResultV1 = serde_json::from_slice(&bytes).unwrap();
    if !cvar && !risk_budget {
        assert_eq!(manifest.engine_versions["portfolio-slippage"], "1");
        assert_eq!(result.slippage_references.len(), result.input.assets.len());
        assert!(result
            .input
            .assets
            .iter()
            .all(|a| a.transaction_cost_rate.is_positive()));
        assert_eq!(
            result.slippage_references[0].close_price,
            "1.02".parse().unwrap()
        );
        assert_eq!(result.bar_notionals.len(), 2);
        assert_eq!(manifest.engine_versions["portfolio-build-rolling"], "1");
        for (asset, value) in result.input.assets.iter().zip(&result.bar_notionals) {
            assert_eq!(
                asset.available_notional.as_ref(),
                Some(&value.notional_value)
            );
        }
    }
    assert!(result
        .input
        .assets
        .iter()
        .all(|asset| asset.groups == ["fixture-group"]));
    assert_eq!(result.input.constraints.group_bounds.len(), 1);
    assert_eq!(
        result.input.objective,
        if risk_budget {
            AllocationObjective::RiskBudgeting
        } else {
            AllocationObjective::MaxUtility
        }
    );
    assert_eq!(
        result.input.risk,
        if cvar {
            AllocationRisk::Cvar
        } else {
            AllocationRisk::Variance
        }
    );
    assert_eq!(
        domain::portfolio::cvar_confidence(result.input.risk, &result.input.optimizer)
            .unwrap()
            .cloned(),
        if cvar {
            Some("0.95".parse().unwrap())
        } else {
            None
        }
    );
    assert_eq!(
        result.allocation.solver_status,
        SolverStatus::Optimal,
        "{:?}",
        result.allocation
    );
    assert_eq!(
        result.input.constraints.max_ex_ante_risk,
        Some("1".parse().unwrap())
    );
    domain::portfolio::allocation_result(&result.input, &result.allocation).unwrap();
    assert!(result
        .allocation
        .targets
        .as_ref()
        .is_some_and(|v| v.len() == 2));
    assert_eq!(
        result.input.forecasts.members[0].forecasts,
        vec![0.01, 0.01]
    );
    assert_eq!(
        result.input.forecasts.members[1].forecasts,
        vec![0.03, 0.03]
    );
    let aggregate = job::validation::aligned_portfolio_forecast(&result.input.forecasts).unwrap();
    assert!(aggregate.iter().all(|v| (*v - 0.025).abs() < 1e-14));
    assert_eq!(result.input.return_history.end_ns.len(), 18);
    let expected_return = if cvar && risk_budget {
        0.997 / 0.999 - 1.0
    } else {
        1.003 / 1.001 - 1.0
    };
    assert!((result.input.return_history.asset_returns[0][0] - expected_return).abs() < 1e-14);
    assert_eq!(
        result.allocation.cvar_risk_budget_witness.is_some(),
        cvar && risk_budget
    );
    assert!(result.consumed_fuel.get() > 0);
    assert_eq!(
        f.native_container(&spec).await.state.unwrap().exit_code,
        Some(0)
    );
    assert_eq!(f.submit(&spec).await.submitted_at, admitted.submitted_at);
    if !cvar && !risk_budget {
        use contracts::execution::{NativeDataQualityReportV1, NativeDatasetSelectionV1};
        let NativeTaskParametersV1::BuildPortfolio { request, .. } = &operation else {
            unreachable!()
        };
        let quality_operation = NativeTaskParametersV1::ValidateData {
            schema_version: SchemaV1,
            selections: vec![NativeDatasetSelectionV1 {
                dataset_revision_id: dataset,
                selection: request.selection.clone(),
            }],
        };
        let mut quality_spec = spec.clone();
        quality_spec.run_id = Id::new();
        quality_spec.external_job_id =
            domain::runtime_jobs::external_id(quality_spec.run_id, 1).unwrap();
        quality_spec.job_kind = quality_operation.job_kind();
        quality_spec.parameters_artifact_id = Id::new();
        quality_spec.requested_output_schemas = quality_operation.output_schemas();
        quality_spec.deadline_at = runtime::now() + chrono::Duration::seconds(50);
        quality_spec
            .inputs
            .retain(|input| matches!(input, RuntimeInputV1::Dataset { .. }));
        let parameters = serde_json::to_vec(&quality_operation).unwrap();
        f.object(quality_spec.parameters_artifact_id, &parameters)
            .await;
        quality_spec.inputs.push(RuntimeInputV1::Artifact {
            artifact_id: quality_spec.parameters_artifact_id,
            storage_version: "1".into(),
            byte_count: count(parameters.len() as u64),
            role: ArtifactInputRole::Parameters,
        });
        f.runs.push(quality_spec.run_id);
        let accepted = f.submit(&quality_spec).await;
        assert_eq!(
            f.terminal(&quality_spec).await.state,
            RuntimeJobState::Succeeded
        );
        let manifest = f.manifest(&quality_spec).await;
        domain::runtime_jobs::manifest(
            &manifest,
            &quality_spec,
            accepted.submitted_at,
            runtime::now(),
        )
        .unwrap();
        assert_eq!(manifest.engine_versions["bar-notional"], "1");
        let [output] = manifest.artifacts.as_slice() else {
            panic!("one native quality report")
        };
        let response = f
            .client
            .get(f.url(&[
                "jobs",
                &quality_spec.external_job_id,
                "artifacts",
                &output.storage_ref.to_string(),
            ]))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.bytes().await.unwrap().to_vec();
        domain::execution::output_bindings(
            &quality_operation,
            None,
            manifest.started_at.unwrap(),
            manifest.finished_at,
            &[(output.clone(), bytes.clone())],
        )
        .unwrap();
        let report: NativeDataQualityReportV1 = serde_json::from_slice(&bytes).unwrap();
        let observations = report.datasets[0].last_bar_notionals.as_ref().unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(
            serde_json::to_value(&result.bar_notionals).unwrap(),
            serde_json::to_value(observations).unwrap()
        );
        for (index, value) in observations.iter().enumerate() {
            assert_eq!(value.instrument_id, request.assets[index].instrument_id);
            assert_eq!(value.currency, "USD");
            assert_eq!(value.close_price, ["1.02", "2.02"][index].parse().unwrap());
            assert_eq!(value.traded_volume, "10000000".parse().unwrap());
            assert_eq!(
                value.notional_value,
                ["10200000", "20200000"][index].parse().unwrap()
            );
        }
        // Consume these exact OCI-produced bytes in a separate native Build.
        let report_id = Id::new();
        f.object(report_id, &bytes).await;
        let mut liquidity_request = request.clone();
        liquidity_request.rolling_liquidity = None;
        liquidity_request.mandate.constraints.liquidity_ref = Some(report_id);
        liquidity_request.mandate.constraints.max_participation = Some("0.00001".parse().unwrap());
        liquidity_request.bar_liquidity = Some(contracts::science::NativePortfolioLiquidityV1 {
            schema_version: SchemaV1,
            assumption: contracts::execution_assumptions::BarLiquidityAssumptionV1 {
                schema_version: SchemaV1,
                report_artifact_id: report_id,
                maximum_age_seconds: 86400,
                participation_limit: "0.00001".parse().unwrap(),
            },
            source: NativeDatasetSelectionV1 {
                dataset_revision_id: dataset,
                selection: request.selection.clone(),
            },
        });
        for (asset, value) in liquidity_request.assets.iter_mut().zip(observations) {
            asset.available_notional = Some(value.notional_value.clone());
        }
        domain::execution::portfolio_build_liquidity(&liquidity_request, &report).unwrap();
        let liquidity_operation = NativeTaskParametersV1::BuildPortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: dataset,
            request: liquidity_request,
        };
        let parameters = serde_json::to_vec(&liquidity_operation).unwrap();
        let mut liquidity_spec = spec.clone();
        liquidity_spec.run_id = Id::new();
        liquidity_spec.external_job_id =
            domain::runtime_jobs::external_id(liquidity_spec.run_id, 1).unwrap();
        liquidity_spec.inputs.retain(|input| !matches!(input, RuntimeInputV1::Artifact {artifact_id,..} if *artifact_id == spec.parameters_artifact_id || Some(*artifact_id) == request.mandate.constraints.liquidity_ref));
        liquidity_spec.parameters_artifact_id = Id::new();
        liquidity_spec.deadline_at = runtime::now() + chrono::Duration::seconds(50);
        f.object(liquidity_spec.parameters_artifact_id, &parameters)
            .await;
        for (id, size, role) in [
            (
                liquidity_spec.parameters_artifact_id,
                parameters.len(),
                ArtifactInputRole::Parameters,
            ),
            (report_id, bytes.len(), ArtifactInputRole::DataQuality),
        ] {
            liquidity_spec.inputs.push(RuntimeInputV1::Artifact {
                artifact_id: id,
                storage_version: "1".into(),
                byte_count: count(size as u64),
                role,
            });
        }
        f.runs.push(liquidity_spec.run_id);
        let accepted = f.submit(&liquidity_spec).await;
        assert_eq!(
            f.terminal(&liquidity_spec).await.state,
            RuntimeJobState::Succeeded
        );
        let manifest = f.manifest(&liquidity_spec).await;
        domain::runtime_jobs::manifest(
            &manifest,
            &liquidity_spec,
            accepted.submitted_at,
            runtime::now(),
        )
        .unwrap();
        assert_eq!(manifest.engine_versions["portfolio-liquidity"], "1");
        assert_eq!(manifest.engine_versions["portfolio-cost-source"], "1");
        let [output] = manifest.artifacts.as_slice() else {
            panic!("one liquidity Build report")
        };
        let response = f
            .client
            .get(f.url(&[
                "jobs",
                &liquidity_spec.external_job_id,
                "artifacts",
                &output.storage_ref.to_string(),
            ]))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.bytes().await.unwrap().to_vec();
        domain::execution::output_bindings(
            &liquidity_operation,
            None,
            manifest.started_at.unwrap(),
            manifest.finished_at,
            &[(output.clone(), bytes.clone())],
        )
        .unwrap();
        let result: NativePortfolioBuildResultV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result.allocation.solver_status, SolverStatus::Optimal);
        assert!(result
            .input
            .assets
            .iter()
            .all(|a| a.available_notional.is_some()));
        domain::portfolio::allocation_result(&result.input, &result.allocation).unwrap();
    }
    f.assert_private_logs();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_native_sealed_job_reads_the_frozen_model_and_registered_parquet() {
    use contracts::{
        execution::NativeTaskParametersV1,
        research::{ArtifactInputRole, DataPartition},
        science::NativeAlphaSealedResultV1,
        Revision,
    };
    let (catalog, request, calibration, wasm) = market::sealed();
    let expected =
        job::validation::evaluate_sealed_alpha(catalog.path(), &request, &wasm, Some(&calibration))
            .unwrap();
    // Synthetic data with actual native observations; never a REAL/PIT attestation.
    let mut metadata = catalog_fixture::metadata();
    metadata.partition = DataPartition::Sealed;
    metadata.event_start = chrono::DateTime::from_timestamp_nanos(
        request.forecast.selection.event_start_ns.get() as i64,
    );
    metadata.event_end = chrono::DateTime::from_timestamp_nanos(
        request.forecast.selection.event_end_ns.get() as i64,
    );
    metadata.available_through = metadata.event_end;
    metadata.row_count = count(expected.forecast.points.len() as u64);
    metadata.universe.coverage_end = metadata.event_end;
    metadata.universe.membership = expected
        .assets
        .iter()
        .map(|asset| {
            let mut member = metadata.universe.membership[0].clone();
            member.instrument_id = asset.instrument_id.clone();
            member
        })
        .collect();
    metadata.universe.instrument_definitions = expected
        .assets
        .iter()
        .map(|asset| {
            serde_json::json!({
                "CurrencyPair":{"id":asset.instrument_id,"fixture_only":true},
            })
        })
        .collect();
    metadata.quality.checked_at = runtime::now();
    let dataset = Id::new();
    let observed = &mut metadata.quality.datasets[0];
    observed.dataset_revision_id = dataset;
    observed.selection = request.forecast.selection.clone();
    observed.row_count = metadata.row_count;
    observed.instrument_ids = expected
        .assets
        .iter()
        .map(|asset| asset.instrument_id.clone())
        .collect();
    observed.first_event_ns = expected
        .forecast
        .points
        .iter()
        .map(|p| p.event_ns)
        .min()
        .unwrap();
    observed.last_event_ns = expected
        .forecast
        .points
        .iter()
        .map(|p| p.event_ns)
        .max()
        .unwrap();
    observed.available_through_ns = expected
        .forecast
        .points
        .iter()
        .map(|p| p.available_ns)
        .max()
        .unwrap();
    domain::catalogs::metadata(&metadata, runtime::now()).unwrap();
    fs::set_permissions(catalog.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let mut f = Fixture::open().await;
    f.crash();
    let metadata_path = f.directory.path().join("sealed-metadata.json");
    fs::write(&metadata_path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&f.config_path).unwrap()).unwrap();
    config["catalogs"] = serde_json::json!([{"root":catalog.path(),"metadata_file":metadata_path}]);
    fs::write(&f.config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    f.restart().await;
    let model = Id::new();
    let fitted = Id::new();
    let parameters = Id::new();
    let operation = NativeTaskParametersV1::EvaluateSealedAlpha {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        model_artifact_id: model,
        calibration_artifact_id: Some(fitted),
        request: Box::new(request),
    };
    let encoded = serde_json::to_vec(&operation).unwrap();
    let fitted_bytes = serde_json::to_vec(&calibration).unwrap();
    f.object(model, &wasm).await;
    f.object(fitted, &fitted_bytes).await;
    f.object(parameters, &encoded).await;
    let run = Id::new();
    f.runs.push(run);
    let spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run, 1).unwrap(),
        job_kind: operation.job_kind(),
        image_ref: support::image(),
        input_set_id: Id::new(),
        inputs: vec![
            RuntimeInputV1::Dataset {
                revision_id: dataset,
                registered_ref: metadata.registered_ref,
                storage_version: metadata.storage_version,
                role: DataPartition::Sealed,
            },
            RuntimeInputV1::Artifact {
                artifact_id: model,
                storage_version: "1".into(),
                byte_count: count(wasm.len() as u64),
                role: ArtifactInputRole::Model,
            },
            RuntimeInputV1::Artifact {
                artifact_id: fitted,
                storage_version: "1".into(),
                byte_count: count(fitted_bytes.len() as u64),
                role: ArtifactInputRole::Model,
            },
            RuntimeInputV1::Artifact {
                artifact_id: parameters,
                storage_version: "1".into(),
                byte_count: count(encoded.len() as u64),
                role: ArtifactInputRole::Parameters,
            },
        ],
        parameters_artifact_id: parameters,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: count(30),
            memory_mib: 512,
            wall_seconds: 30,
            output_bytes: count(4 * 1024 * 1024),
        },
        deadline_at: runtime::now() + chrono::Duration::seconds(50),
        requested_output_schemas: operation.output_schemas(),
    };
    let admitted = f.submit(&spec).await;
    assert_eq!(f.terminal(&spec).await.state, RuntimeJobState::Succeeded);
    let manifest = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&manifest, &spec, admitted.submitted_at, runtime::now())
        .unwrap();
    let [output] = manifest.artifacts.as_slice() else {
        panic!("one original sealed report required")
    };
    assert_eq!(output.schema.name, "qz.alpha_sealed");
    let response = f
        .client
        .get(f.url(&[
            "jobs",
            &spec.external_job_id,
            "artifacts",
            &output.storage_ref.to_string(),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.bytes().await.unwrap().to_vec();
    domain::execution::output_bindings(
        &operation,
        Some(&calibration),
        manifest.started_at.unwrap(),
        manifest.finished_at,
        &[(output.clone(), bytes.clone())],
    )
    .unwrap();
    let result: NativeAlphaSealedResultV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    f.assert_private_logs();
}

#[tokio::test]
async fn real_native_compile_publishes_exact_model_and_concurrent_retry_has_one_container() {
    let mut f = Fixture::open().await;
    let spec = f.compile(SIGNAL, 30).await;
    let (first, replay) = tokio::join!(f.submit(&spec), f.submit(&spec));
    assert_eq!(first.run_id, replay.run_id);
    assert_eq!(first.attempt_no, replay.attempt_no);
    assert_eq!(first.submitted_at, replay.submitted_at);
    assert_eq!(first.external_job_id, spec.external_job_id);
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Succeeded);
    let manifest = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&manifest, &spec, first.submitted_at, runtime::now()).unwrap();
    assert_eq!(manifest.engine_versions["rustc"], "1.98.1");
    assert!(manifest.resource_usage.cpu_nanoseconds.is_none());
    assert!(manifest.resource_usage.peak_memory_bytes.is_none());
    let model = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.kind == RuntimeOutputKind::Model)
        .unwrap();
    let bytes = f
        .client
        .get(f.url(&[
            "jobs",
            &spec.external_job_id,
            "artifacts",
            &model.storage_ref.to_string(),
        ]))
        .send()
        .await
        .unwrap();
    assert_eq!(bytes.status(), StatusCode::OK);
    assert_eq!(bytes.headers()["content-type"], "application/wasm");
    let bytes = bytes.bytes().await.unwrap();
    assert!(bytes.starts_with(b"\0asm\x01\0\0\0"));
    assert_eq!(bytes.len() as u64, model.byte_count.get());
    let native = f.native_container(&spec).await;
    let native_id = native.id.unwrap();
    assert_eq!(native.state.unwrap().exit_code, Some(0));
    let repeated = f.submit(&spec).await;
    assert_eq!(repeated, terminal);
    assert_eq!(
        f.native_container(&spec).await.id.as_deref(),
        Some(native_id.as_str())
    );
    let mut conflict = spec.clone();
    conflict.limits.memory_mib += 1;
    let response = f
        .client
        .post(f.url(&["jobs"]))
        .json(&conflict)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    f.assert_private_logs();
}

#[tokio::test]
async fn completed_job_and_exact_bytes_survive_abrupt_gateway_restart_without_reexecution() {
    let mut f = Fixture::open().await;
    let spec = f.compile(SIGNAL, 30).await;
    f.submit(&spec).await;
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Succeeded);
    let before = f.manifest(&spec).await;
    let native_before = f.native_container(&spec).await;
    f.crash();
    f.restart().await;
    assert_eq!(f.submit(&spec).await, terminal);
    let after = f.manifest(&spec).await;
    assert_eq!(
        serde_json::to_value(after).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    let native_after = f.native_container(&spec).await;
    assert_eq!(native_after.id, native_before.id);
    assert_eq!(
        native_after.state.as_ref().unwrap().started_at,
        native_before.state.as_ref().unwrap().started_at
    );
    assert_eq!(native_after.restart_count, Some(0));
    f.assert_private_logs();
}

#[tokio::test]
async fn native_wall_deadline_stops_the_process_while_gateway_is_dead_then_reconciles_same_identity(
) {
    let mut f = Fixture::open().await;
    let spec = f.compile(SLOW_SIGNAL, 4).await;
    let submitted = f.submit(&spec).await;
    let docker = docker().await;
    let original = tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let container = f.native_container(&spec).await;
            if container.state.as_ref().unwrap().running == Some(true) {
                return container;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("native compiler must actually start before gateway crash");
    let id = original.id.clone().unwrap();
    f.crash();
    let stopped = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let current = docker.inspect_container(&id, None).await.unwrap();
            if current.state.as_ref().unwrap().running == Some(false) {
                return current;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("native deadline must not depend on a live gateway");
    let stopped_state = stopped.state.as_ref().unwrap();
    assert!(matches!(stopped_state.exit_code, Some(124 | 137)));
    assert_eq!(stopped_state.oom_killed, Some(false));
    let started =
        chrono::DateTime::parse_from_rfc3339(stopped_state.started_at.as_deref().unwrap()).unwrap();
    let finished =
        chrono::DateTime::parse_from_rfc3339(stopped_state.finished_at.as_deref().unwrap())
            .unwrap();
    let elapsed = (finished - started).num_milliseconds();
    assert!(
        (3_750..=7_000).contains(&elapsed),
        "the native deadline, not an unrelated startup error, must stop this job"
    );
    f.restart().await;
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Failed);
    let result = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&result, &spec, submitted.submitted_at, runtime::now()).unwrap();
    assert!(result.artifacts.is_empty());
    assert_eq!(
        f.native_container(&spec).await.id.as_deref(),
        Some(id.as_str())
    );
    assert_eq!(f.submit(&spec).await, terminal);
    f.assert_private_logs();
}

#[tokio::test]
async fn cancelled_native_identity_blocks_both_late_create_and_old_id_start() {
    let mut f = Fixture::open().await;
    let spec = f.compile(SLOW_SIGNAL, 60).await;
    f.submit(&spec).await;
    let original = f.native_container(&spec).await;
    let id = original.id.clone().unwrap();
    let name = original
        .name
        .clone()
        .unwrap()
        .trim_start_matches('/')
        .to_owned();
    let request = RuntimeCancelV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: 1,
        owner_epoch: spec.owner_epoch.next().unwrap(),
    };
    let pending: RuntimeJobStatusV1 = f
        .json(
            Method::POST,
            &["jobs", &spec.external_job_id, "cancel"],
            Some(serde_json::to_value(request).unwrap()),
            &[StatusCode::OK, StatusCode::ACCEPTED],
        )
        .await;
    assert!(matches!(
        pending.state,
        RuntimeJobState::CancelRequested | RuntimeJobState::Cancelled
    ));
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Cancelled);
    assert_eq!(f.submit(&spec).await, terminal);
    let barrier = f.native_container(&spec).await;
    assert_ne!(barrier.id.as_deref(), Some(id.as_str()));
    assert_eq!(
        barrier.config.as_ref().unwrap().labels.as_ref().unwrap()["io.quazonai.role"],
        "TOMBSTONE"
    );
    assert_eq!(barrier.state.as_ref().unwrap().running, Some(false));
    let docker = docker().await;
    assert!(matches!(
        docker.start_container(&id, None).await,
        Err(DockerError::DockerResponseServerError {
            status_code: 404,
            ..
        })
    ));
    let options = CreateContainerOptionsBuilder::default().name(&name).build();
    assert!(matches!(
        docker
            .create_container(
                Some(options),
                ContainerCreateBody {
                    image: Some(support::image()),
                    ..Default::default()
                }
            )
            .await,
        Err(DockerError::DockerResponseServerError {
            status_code: 409,
            ..
        })
    ));
    let manifest = f.manifest(&spec).await;
    assert_eq!(manifest.state, RuntimeResultState::Cancelled);
    assert!(manifest.artifacts.is_empty());
    assert_eq!(manifest.resource_usage.output_bytes.get(), 0);
    f.assert_private_logs();
}

#[tokio::test]
async fn real_native_compile_oom_is_reported_as_a_safe_resource_failure() {
    let mut f = Fixture::open().await;
    // The actual compiler materializes this retained constant inside its cgroup.
    // Never compile this input directly on the host.
    let code = format!("{SIGNAL}\n#[used] static PRESSURE: [u8; 1 << 30] = [1; 1 << 30];\n");
    let mut spec = f.compile(&code, 30).await;
    spec.limits.memory_mib = 64;
    let accepted = f.submit(&spec).await;
    let terminal = f.terminal(&spec).await;
    assert_eq!(terminal.state, RuntimeJobState::Failed);
    let manifest = f.manifest(&spec).await;
    domain::runtime_jobs::manifest(&manifest, &spec, accepted.submitted_at, runtime::now())
        .unwrap();
    assert!(manifest.artifacts.is_empty());
    assert_eq!(manifest.resource_usage.output_bytes.get(), 0);
    let error = manifest.error.unwrap();
    assert_eq!(error.code, RuntimeFailureCode::MemoryLimit);
    assert_eq!(error.class, RuntimeFailureClass::ResourceLimit);
    assert_eq!(
        error.safe_message,
        "The native job exceeded its memory limit."
    );
    let state = f.native_container(&spec).await.state.unwrap();
    assert_eq!(state.oom_killed, Some(true));
    assert_ne!(state.exit_code, Some(0));
    assert_eq!(f.submit(&spec).await, terminal);
    f.assert_private_logs();
}

#[tokio::test]
async fn compiler_cannot_read_a_test_owned_host_secret_or_runtime_credential_namespace() {
    let mut f = Fixture::open().await;
    let sentinel = f.directory.path().join("not-mounted-native-sentinel");
    fs::write(&sentinel, "fixture-host-secret-never-shared").unwrap();
    let code = format!(
        "{}\nconst _: &str = include_str!({:?});\n",
        SIGNAL, sentinel
    );
    let spec = f.compile(&code, 30).await;
    f.submit(&spec).await;
    assert_eq!(f.terminal(&spec).await.state, RuntimeJobState::Failed);
    let manifest = f.manifest(&spec).await;
    assert!(manifest.artifacts.is_empty());
    assert_eq!(
        manifest.error.unwrap().code,
        RuntimeFailureCode::NativeJobFailed
    );
    let response = f
        .client
        .get(f.url(&["objects", &spec.parameters_artifact_id.to_string()]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    f.assert_private_logs();
}

/// Exercise the exact production-generated HostConfig in an actual native container.
/// Only this CI helper changes the fixed process entrypoint to its probe. Neither
/// HTTP nor JobSpec has an entrypoint/command override, verified separately above.
async fn isolated_probe(
    mode: &str,
) -> (bollard::models::ContainerInspectResponse, tempfile::TempDir) {
    let mut fixture = Fixture::open().await;
    let mut spec = fixture.compile(SIGNAL, 10).await;
    spec.limits.memory_mib = 64;
    spec.limits.output_bytes = count(1024 * 1024);
    fixture.crash();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input");
    let output = directory.path().join("output");
    fs::create_dir(&input).unwrap();
    fs::create_dir(&output).unwrap();
    fs::set_permissions(&input, fs::Permissions::from_mode(0o755)).unwrap();
    fs::set_permissions(&output, fs::Permissions::from_mode(0o1777)).unwrap();
    fs::write(input.join("fixture.txt"), b"native read-only input").unwrap();
    fs::set_permissions(input.join("fixture.txt"), fs::Permissions::from_mode(0o444)).unwrap();
    let sentinel = directory.path().join("host-only-sentinel");
    fs::write(&sentinel, b"test owned secret").unwrap();
    let native_image = NativeImage {
        id: support::image(),
        versions: BTreeMap::from([("native-ci-probe".into(), "1".into())]),
    };
    let mut body = NativeEngine::launch(
        Id::new(),
        &spec,
        &native_image,
        vec![
            runtime::engine::bind(&input, "/input", true).unwrap(),
            runtime::engine::bind(&output, "/output", false).unwrap(),
        ],
        false,
    )
    .unwrap();
    body.entrypoint = Some(vec!["/usr/local/bin/isolation-probe".into()]);
    body.cmd = Some(vec![mode.into(), sentinel.to_str().unwrap().into()]);
    let docker = docker().await;
    let options = CreateContainerOptionsBuilder::default()
        .name(&format!("native-boundary-probe-{}", spec.run_id))
        .build();
    let created = docker.create_container(Some(options), body).await.unwrap();
    docker.start_container(&created.id, None).await.unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(15), async {
        let mut observed_terminal = None;
        loop {
            let current = docker.inspect_container(&created.id, None).await.unwrap();
            let state = current.state.as_ref().unwrap();
            if state.running == Some(false) {
                let snapshot = (state.exit_code, state.oom_killed);
                if observed_terminal != Some(snapshot) {
                    let limits = current.host_config.as_ref().unwrap();
                    // Native scalar evidence only; no credentials, paths or environment.
                    println!(
                        "{}",
                        serde_json::json!({
                            "scope": "test-owned native resource probe",
                            "mode": mode,
                            "running": state.running,
                            "exit_code": state.exit_code,
                            "oom_killed": state.oom_killed,
                            "memory": limits.memory,
                            "memory_swap": limits.memory_swap,
                            "pids_limit": limits.pids_limit
                        })
                    );
                    observed_terminal = Some(snapshot);
                }
                // Docker processes exit and OOM events separately. Do not delete
                // the memory probe before observing both required facts.
                if mode != "memory" || state.oom_killed == Some(true) {
                    return current;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    let cleanup = RemoveContainerOptionsBuilder::default()
        .force(true)
        .v(true)
        .build();
    docker
        .remove_container(&created.id, Some(cleanup))
        .await
        .unwrap();
    (outcome.expect("bounded native isolation probe"), directory)
}

#[tokio::test]
async fn actual_native_namespaces_forbid_secret_socket_network_root_input_writes_and_tmp_execution()
{
    let (outcome, directory) = isolated_probe("inspect").await;
    assert_eq!(outcome.state.unwrap().exit_code, Some(0));
    assert_eq!(
        fs::read(directory.path().join("output/verified")).unwrap(),
        b"native namespace and resource controls verified"
    );
}

#[tokio::test]
async fn actual_native_memory_pids_and_file_size_limits_are_enforced_by_the_kernel() {
    let (memory, _) = isolated_probe("memory").await;
    let state = memory.state.unwrap();
    assert_eq!(state.oom_killed, Some(true));
    assert_ne!(state.exit_code, Some(0));
    let (pids, directory) = isolated_probe("pids").await;
    assert_eq!(pids.state.unwrap().exit_code, Some(0));
    let children: u32 = fs::read_to_string(directory.path().join("output/pids-verified"))
        .unwrap()
        .parse()
        .unwrap();
    assert!((1..64).contains(&children));
    let (files, directory) = isolated_probe("output").await;
    let exit = files.state.unwrap().exit_code;
    assert!(exit.is_some() && exit != Some(0) && exit != Some(99));
    assert!(
        fs::metadata(directory.path().join("output/bounded-file"))
            .unwrap()
            .len()
            <= 1024 * 1024
    );
}
