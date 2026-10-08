//! Joined CLI → HTTP/PGMQ/Worker → native compiler/Wasmi/Nautilus → CLI acceptance.
//! All market inputs are synthetic FIXTURE/PIT-UNVERIFIED. No Agent or model provider.
use super::cycle_support::execution_models;
use super::{
    cycle_support, experiment_support, published, research_support, runtime_support,
    scientific_catalog, support as oci,
};
#[path = "../../../server/tests/support/client.rs"]
#[allow(dead_code)]
mod client;
#[path = "direct_strategy.rs"]
mod direct_strategy;
#[path = "../../../../tests/support/external_experiments.rs"]
mod external_support;
#[path = "../../../job/tests/support/experiment.rs"]
mod fixture;
#[path = "paper_account_readback.rs"]
mod paper_account_readback;
#[path = "../../../server/tests/support/mod.rs"]
#[allow(dead_code)]
mod support;

use axum::http::StatusCode;
use contracts::{
    artifacts::ResearchArtifactKind,
    catalogs::{
        DataRevisionPolicy, NativeUniverseMemberV1, NativeUniverseV1, RuntimeCatalogMetadataV1,
    },
    control::CommandResult,
    execution::{
        NativeDataQualityReportV1, NativeDatasetQualityV1, NativeModelCompilationV1,
        NativeTaskParametersV1,
    },
    experiment_summary::ExperimentSummaryV1,
    experiments::{
        ExperimentEvaluateIntent, ExperimentEvaluateV1, ExperimentOutcome, ExperimentProposalV1,
        ExperimentView,
    },
    research::{DataOrigin, DataPartition, PitStatus},
    runs::{RunSnapshotV1, RunState},
    runtime::RuntimeDataKind,
    runtime_jobs::{JobSpecV1, ResultManifestV1, RuntimeInputV1, RuntimeObjectReceiptV1},
    science::*,
    Id, SchemaV1,
};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use nautilus_model::instruments::Instrument;
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde_json::{json, Value};
use server::worker::Worker;
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

const VALIDATION_OFFSET: u64 = 100 * fixture::SECOND;

struct Catalog {
    directory: tempfile::TempDir,
    versions: Vec<RuntimeCatalogMetadataV1>,
    request: NativeExperimentEvaluationRequestV1,
    features: Vec<FeatureObservationsV1>,
    evaluation_offset: u64,
    forward_feature: Option<FeatureObservationsV1>,
}

fn instant(ns: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp_nanos(ns.try_into().unwrap())
}

fn catalog(market: fixture::FixtureMarket, strategy: bool) -> Catalog {
    let (seed, mut request, mut features) = fixture::market_fixture(false, market, 1);
    let mut observed = job::catalog::load_catalog(seed.path(), &request.selection).unwrap();
    // Freeze Paper-compatible identity, fees, capital and clocks before any native
    // evaluation. Never rewrite an accepted report or its resulting claim.
    let epoch = if strategy {
        direct_strategy::paper_market(&mut observed, &mut request);
        // SQL timestamp headers retain microseconds. Use whole-second window
        // bounds while keeping each native ts_init's original +1 ns below them.
        u64::try_from(runtime::now().timestamp()).unwrap() * fixture::SECOND - 400 * fixture::SECOND
    } else {
        0
    };
    assert_eq!(observed.rows, 40);
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755)).unwrap();
    let universe = NativeUniverseV1 {
        name: "Synthetic second-frequency direct research".into(),
        calendar_ref: "synthetic-second-calendar".into(),
        calendar_version: "1".into(),
        calendar_sessions: None,
        selection_asof: instant(epoch),
        has_historical_membership: false,
        coverage_start: instant(epoch),
        coverage_end: instant(epoch + 400 * fixture::SECOND),
        membership: observed
            .series
            .iter()
            .map(|series| NativeUniverseMemberV1 {
                instrument_id: series.instrument.id().to_string(),
                valid_from: instant(epoch),
                valid_until: None,
                available_at: instant(epoch),
                groups: None,
            })
            .collect(),
        instrument_definitions: observed
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
        let current = strategy && partition == DataPartition::Forward;
        // Current continuation reuses the complete original validation warmup,
        // then a fresh bar; the Sealed window is never mounted.
        let offset = epoch
            + if current {
                VALIDATION_OFFSET
            } else {
                index as u64 * VALIDATION_OFFSET
            };
        let root = directory.path().join(format!("window-{index}"));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        let native =
            ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, Some(16), None, None)
                .unwrap();
        native
            .write_instruments(
                observed
                    .series
                    .iter()
                    .map(|series| series.instrument.clone())
                    .collect(),
            )
            .unwrap();
        for series in &observed.series {
            let mut bars = series
                .bars
                .iter()
                .copied()
                .map(|mut bar| {
                    bar.ts_event = (bar.ts_event.as_u64() + offset).into();
                    bar.ts_init = (bar.ts_init.as_u64() + offset).into();
                    bar
                })
                .collect::<Vec<_>>();
            if current {
                let mut last = *bars.last().unwrap();
                last.ts_event = (epoch + 399 * fixture::SECOND).into();
                last.ts_init = (last.ts_event.as_u64() + 1).into();
                bars.push(last);
            }
            native.write_to_parquet(&bars, None, None, None).unwrap();
        }
        let mut selection = request.selection.clone();
        selection.event_start_ns = oci::count(selection.event_start_ns.get() + offset);
        selection.event_end_ns = oci::count(selection.event_end_ns.get() + offset);
        selection.decision_cutoff_ns = oci::count(selection.decision_cutoff_ns.get() + offset);
        if current {
            selection.event_end_ns = oci::count(epoch + 400 * fixture::SECOND);
            selection.decision_cutoff_ns = selection.event_end_ns;
        }
        let actual = job::catalog::load_catalog(&root, &selection).unwrap();
        assert_eq!(actual.rows, if current { 41 } else { 40 });
        let bars = &actual.series[0].bars;
        let first = bars.first().unwrap().ts_event.as_u64();
        let last = bars.last().unwrap().ts_event.as_u64();
        let available = bars.last().unwrap().ts_init.as_u64();
        let metadata = RuntimeCatalogMetadataV1 {
            recorded_feature_inputs: None,
            schema_version: SchemaV1,
            registered_ref: scientific_catalog::REGISTRY.into(),
            native_snapshot_ref: format!("direct-synthetic-{index}"),
            storage_version: format!("direct-window-{index}-v1"),
            provider_kind: "NAUTILUS_CATALOG".into(),
            data_kind: RuntimeDataKind::Bar,
            partition,
            event_start: instant(selection.event_start_ns.get()),
            event_end: instant(selection.event_end_ns.get()),
            available_through: instant(selection.decision_cutoff_ns.get()),
            row_count: oci::count(actual.rows as u64),
            origin: DataOrigin::Fixture,
            pit_status: PitStatus::Unverified,
            revision_policy: DataRevisionPolicy::AsKnownThen,
            provenance_reference: "Authored synthetic Parquet; no real market evidence".into(),
            availability_provenance: "Declared synthetic ts_init; no historical PIT attestation"
                .into(),
            universe: universe.clone(),
            quality: NativeDataQualityReportV1 {
                schema_version: SchemaV1,
                native_version: "nautilus-persistence/0.63.0".into(),
                checked_at: runtime::now(),
                datasets: vec![NativeDatasetQualityV1 {
                    settlements: vec![],
                    dataset_revision_id: Id::new(),
                    selection: selection.clone(),
                    row_count: oci::count(actual.rows as u64),
                    instrument_ids: vec![request.instrument_id.clone()],
                    first_event_ns: oci::count(first),
                    last_event_ns: oci::count(last),
                    available_through_ns: oci::count(available),
                    last_bar_notionals: None,
                }],
            },
        };
        domain::catalogs::metadata(&metadata, runtime::now()).unwrap();
        versions.push(metadata);
    }
    request.selection = versions[1].quality.datasets[0].selection.clone();
    let evaluation_offset = epoch + VALIDATION_OFFSET;
    for part in &mut features {
        for row in &mut part.observations {
            row.event_ns = oci::count(row.event_ns.get() + evaluation_offset);
            row.observed_available_ns = row
                .observed_available_ns
                .map(|v| oci::count(v.get() + evaluation_offset));
        }
    }
    let forward_feature = strategy.then(|| FeatureObservationsV1 {
        schema_version: SchemaV1,
        partition: DataPartition::Forward,
        feature_schema: request.feature_schema.clone(),
        observations: vec![FeatureObservationV1 {
            feature_index: 0,
            event_ns: oci::count(epoch + 399 * fixture::SECOND),
            observed_available_ns: Some(oci::count(epoch + 399 * fixture::SECOND + 1)),
            sequence: oci::count(100_000),
            value: Some(0.5),
            missing_reason: None,
        }],
    });
    Catalog {
        directory,
        versions,
        request,
        features,
        evaluation_offset,
        forward_feature,
    }
}

async fn execute(
    pool: &PgPool,
    data: &scientific_catalog::Prepared,
    remote: &mut oci::Fixture,
    worker: &Worker,
    run: Id,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> (RunSnapshotV1, JobSpecV1, store::lifecycle::RunMessage) {
    remote.runs.push(run);
    let message = data
        .store
        .read_native_run_messages(60, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|message| message.run_id == run)
        .expect("original Run must enter actual PGMQ");
    tokio::time::timeout(
        Duration::from_secs(150),
        worker.process_message(message.clone(), "direct-native-science", shutdown),
    )
    .await
    .expect("bounded production Worker execution")
    .unwrap();
    let finished = data.store.get_run(&data.actor, run).await.unwrap();
    let raw: Value = sqlx::query_scalar(
        "SELECT spec_json FROM app.run_native_attempts WHERE run_id=$1 AND attempt_id=$2",
    )
    .bind(run.as_uuid())
    .bind(finished.active_attempt_id.unwrap().as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    let spec: JobSpecV1 = serde_json::from_value(raw).unwrap();
    assert_eq!(spec.run_id, run);
    assert_eq!(spec.attempt_no, 1);
    assert_eq!(spec.image_ref, oci::image());
    let response = remote
        .client
        .get(remote.url(&["jobs", &spec.external_job_id, "result"]))
        .send()
        .await
        .unwrap();
    let response_status = response.status();
    let raw = response.bytes().await.unwrap();
    let decoded = serde_json::from_slice::<ResultManifestV1>(&raw);
    if finished.state != RunState::Succeeded {
        // Public terminal fields plus the original container's typed exit facts.
        // Keep the original failure visible before fixture cleanup; no input,
        // native stderr, credentials or arbitrary Docker metadata is printed.
        let container =
            tokio::time::timeout(Duration::from_secs(3), remote.native_container(&spec))
                .await
                .ok();
        let state = container.as_ref().and_then(|value| value.state.as_ref());
        eprintln!(
            "direct native failure: run={} kind={:?} requested_schemas={:?} state={:?} terminal_reason={:?} runtime_http={} runtime_state={:?} error={:?} usage={:?} exit_code={:?} oom_killed={:?} container_running={:?}",
            run,
            spec.job_kind,
            spec.requested_output_schemas
                .iter()
                .map(|schema| schema.name.as_str())
                .collect::<Vec<_>>(),
            finished.state,
            finished.terminal_reason_code,
            response_status,
            decoded.as_ref().ok().map(|manifest| manifest.state),
            decoded
                .as_ref()
                .ok()
                .and_then(|manifest| manifest.error.as_ref())
                .map(|error| (&error.class, &error.code)),
            decoded
                .as_ref()
                .ok()
                .map(|manifest| &manifest.resource_usage),
            state.and_then(|state| state.exit_code),
            state.and_then(|state| state.oom_killed),
            state.and_then(|state| state.running),
        );
    }
    assert_eq!(finished.state, RunState::Succeeded);
    assert_eq!(response_status, StatusCode::OK);
    let manifest = decoded.unwrap();
    assert_eq!(
        remote
            .native_container(&spec)
            .await
            .state
            .as_ref()
            .unwrap()
            .running,
        Some(false)
    );
    assert_eq!(manifest.run_id, run);
    assert_eq!(manifest.attempt_no, spec.attempt_no);
    let (receipt, size): (String, i64) = sqlx::query_as("SELECT id::text,byte_count FROM app.artifacts WHERE producer_run_id=$1 AND producer_attempt_id=$2 AND schema_name='qz.job_result'")
        .bind(run.as_uuid()).bind(finished.active_attempt_id.unwrap().as_uuid()).fetch_one(pool).await.unwrap();
    assert_eq!(
        data.data
            .objects
            .read(Id::try_from(receipt).unwrap(), oci::count(size as u64))
            .unwrap(),
        raw
    );
    let facts: (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$2),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$2)")
        .bind(run.as_uuid()).bind(message.message_id).fetch_one(pool).await.unwrap();
    assert_eq!(facts, (1, 1, 0, 1));
    (finished, spec, message)
}

async fn remote_bytes(remote: &oci::Fixture, spec: &JobSpecV1, id: Id) -> Vec<u8> {
    remote
        .client
        .get(remote.url(&["jobs", &spec.external_job_id, "artifacts", &id.to_string()]))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .bytes()
        .await
        .unwrap()
        .to_vec()
}

async fn assert_existing_input(remote: &oci::Fixture, spec: &JobSpecV1, id: Id, bytes: &[u8]) {
    let input = spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                storage_version,
                byte_count,
                ..
            } if *artifact_id == id => Some((storage_version, byte_count)),
            _ => None,
        })
        .expect("the original artifact must be bound in the actual native spec");
    assert_eq!(input.1.get(), bytes.len() as u64);
    // Inputs deliberately have no public GET. Exact immutable PUT replay is the
    // public proof that this byte sequence was already stored: mismatch is409,
    // a newly created object is201, and only an identical existing object is200.
    let response = remote
        .client
        .put(remote.url(&["objects", &id.to_string()]))
        .header("content-type", "application/octet-stream")
        .header("x-qz-storage-version", input.0.as_str())
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let receipt: RuntimeObjectReceiptV1 = response.json().await.unwrap();
    assert_eq!(receipt.artifact_id, id);
    assert_eq!(&receipt.storage_version, input.0);
    assert_eq!(receipt.byte_count, *input.1);
}

fn output<T: serde::de::DeserializeOwned>(output: &std::process::Output) -> T {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn native_variant(value: &Value) -> &Value {
    let object = value.as_object().expect("native enum object");
    assert_eq!(object.len(), 1);
    object.values().next().unwrap()
}

fn native_clock(value: &Value) -> u64 {
    value
        .as_str()
        .expect("canonical native timestamp")
        .parse()
        .unwrap()
}

fn snapshot_return_input(value: &Value) -> nautilus_model::events::PortfolioSnapshot {
    // CanonicalBacktestResult intentionally normalizes UUIDs to event-N tokens.
    // It is not raw PortfolioSnapshot JSON. Read only the original fields used
    // by native snapshot-return analysis; never rewrite the canonical document.
    #[derive(serde::Deserialize)]
    struct ReturnInput {
        account_id: nautilus_model::identifiers::AccountId,
        account_type: nautilus_model::enums::AccountType,
        base_currency: Option<nautilus_model::types::Currency>,
        total_equity: Vec<nautilus_model::types::Money>,
        base_currency_equity: Option<nautilus_model::types::Money>,
        unpriced_instruments: Vec<nautilus_model::identifiers::InstrumentId>,
        ts_event: contracts::DbCounter,
        ts_init: contracts::DbCounter,
    }
    let input: ReturnInput = serde_json::from_value(value.clone())
        .expect("canonical snapshot financial fields and original clocks");
    nautilus_model::events::PortfolioSnapshot {
        account_id: input.account_id,
        account_type: input.account_type,
        base_currency: input.base_currency,
        balances: vec![],
        margins: vec![],
        unrealized_pnls: vec![],
        realized_pnls: vec![],
        total_equity: input.total_equity,
        base_currency_equity: input.base_currency_equity,
        is_stale: false,
        stale_instruments: vec![],
        stale_currencies: vec![],
        unpriced_instruments: input.unpriced_instruments,
        // The native analyzer never reads this transport identity or the empty
        // auxiliary fields above. This is an analysis view, not a restored event.
        event_id: Default::default(),
        ts_event: input.ts_event.get().into(),
        ts_init: input.ts_init.get().into(),
    }
}

fn snapshot_return_inputs(values: &[Value]) -> Vec<nautilus_model::events::PortfolioSnapshot> {
    let mut snapshots = values.iter().map(snapshot_return_input).collect::<Vec<_>>();
    // Canonical arrays are sorted by JSON bytes, not native emission order.
    // Restore the original event/init clocks for the stateful native analyzer.
    snapshots.sort_by_key(|snapshot| (snapshot.ts_event, snapshot.ts_init));
    for pair in snapshots.windows(2) {
        if (pair[0].ts_event, pair[0].ts_init) == (pair[1].ts_event, pair[1].ts_init) {
            assert_eq!(
                (
                    &pair[0].account_id,
                    &pair[0].total_equity,
                    &pair[0].base_currency_equity,
                    &pair[0].unpriced_instruments
                ),
                (
                    &pair[1].account_id,
                    &pair[1].total_equity,
                    &pair[1].base_currency_equity,
                    &pair[1].unpriced_instruments
                ),
                "canonical financial values at the same original clock are ambiguous: {}",
                pair[0].ts_event
            );
        }
    }
    snapshots
}

fn assert_native_snapshot_returns(result: &NativeSimulationResultV1) {
    let snapshots = snapshot_return_inputs(
        result.canonical_result["portfolio_snapshots"]
            .as_array()
            .unwrap(),
    );
    assert!(!snapshots.is_empty());
    let account = snapshots[0].account_id;
    assert!(snapshots
        .iter()
        .all(|snapshot| snapshot.account_id == account));
    let mut native = nautilus_analysis::analyzer::PortfolioAnalyzer::default();
    native.set_portfolio_returns_from_snapshots(&[account], &snapshots);
    let expected = native
        .portfolio_returns()
        .iter()
        .map(|(time, &value)| (time.as_u64(), Some(value), None))
        .collect::<Vec<_>>();
    let observed = result
        .returns
        .iter()
        .map(|point| {
            (
                point.timestamp_ns.get(),
                point.value,
                point.reason_code.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    // Native registration equity is assigned to the previous UTC day. At epoch
    // zero that day saturates to zero; a current-date Paper fixture therefore
    // need not have the same empty return series as the epoch-zero fixture.
    assert_eq!(result.returns_kind, NativeReturnsKind::PortfolioDaily);
    assert_eq!(observed, expected, "native snapshot-return mismatch");
    assert_eq!(
        result.returns_status,
        if expected.is_empty() {
            contracts::evidence::MetricStatus::InsufficientData
        } else {
            contracts::evidence::MetricStatus::Ok
        }
    );
    assert_eq!(
        result.returns_reason.as_deref(),
        expected
            .is_empty()
            .then_some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE")
    );
}

fn assert_summary_return_statistic(
    metric: &contracts::experiment_summary::ExperimentSummaryStatisticV1,
    result: &NativeSimulationResultV1,
) {
    if result.returns_status != contracts::evidence::MetricStatus::Ok {
        assert_eq!(metric.value, None);
        assert_eq!(metric.status, result.returns_status);
        assert_eq!(metric.reason_code, result.returns_reason);
    } else {
        let native = result
            .statistics
            .iter()
            .find(|value| {
                value.group == NativeStatisticGroup::Returns
                    && value.native_key == metric.native_key
                    && value.currency.is_none()
            })
            .unwrap();
        assert_eq!(metric.value, native.value);
        assert_eq!(
            metric.status,
            if native.value.is_some() {
                contracts::evidence::MetricStatus::Ok
            } else {
                contracts::evidence::MetricStatus::Failed
            }
        );
        assert_eq!(metric.reason_code, native.reason_code);
    }
}

#[test]
fn native_registration_baseline_is_not_epoch_invariant() {
    use nautilus_model::{
        enums::AccountType,
        events::PortfolioSnapshot,
        identifiers::AccountId,
        types::{Currency, Money},
    };
    let account = AccountId::from("SIM-001");
    let first = PortfolioSnapshot {
        account_id: account,
        account_type: AccountType::Cash,
        base_currency: Some(Currency::USD()),
        balances: vec![],
        margins: vec![],
        unrealized_pnls: vec![],
        realized_pnls: vec![],
        total_equity: vec![Money::new(1000.0, Currency::USD())],
        base_currency_equity: None,
        is_stale: false,
        stale_instruments: vec![],
        stale_currencies: vec![],
        unpriced_instruments: vec![],
        event_id: Default::default(),
        ts_event: 0.into(),
        ts_init: 0.into(),
    };
    let day = 86_400 * fixture::SECOND;
    for start in [0, 20_000 * day + day / 2] {
        let mut registration = first.clone();
        registration.ts_event = start.into();
        registration.ts_init = start.into();
        let mut closing = registration.clone();
        closing.ts_event = (start + 40 * fixture::SECOND).into();
        closing.ts_init = closing.ts_event;
        closing.total_equity = vec![Money::new(900.0, Currency::USD())];
        let mut inputs = [registration, closing]
            .iter()
            .enumerate()
            .map(|(i, source)| {
                let mut canonical = serde_json::to_value(source).unwrap();
                canonical["event_id"] = json!(format!("event-{}", i + 1));
                canonical["ts_event"] = json!(source.ts_event.to_string());
                canonical["ts_init"] = json!(source.ts_init.to_string());
                let original = canonical.clone();
                let projected = snapshot_return_input(&canonical);
                assert_eq!(
                    canonical, original,
                    "canonical identities and values stay unchanged"
                );
                assert_eq!(projected.account_id, source.account_id);
                assert_eq!(projected.ts_event, source.ts_event);
                assert_eq!(projected.ts_init, source.ts_init);
                assert_eq!(projected.total_equity, source.total_equity);
                canonical
            })
            .collect::<Vec<_>>();
        inputs.reverse();
        let original = inputs.clone();
        let snapshots = snapshot_return_inputs(&inputs);
        assert_eq!(inputs, original, "canonical array order remains unchanged");
        let mut native = nautilus_analysis::analyzer::PortfolioAnalyzer::default();
        native.set_portfolio_returns_from_snapshots(&[account], &snapshots);
        if start == 0 {
            assert!(native.portfolio_returns().is_empty());
        } else {
            assert_eq!(native.portfolio_returns().len(), 1);
            let (&time, &value) = native.portfolio_returns().iter().next().unwrap();
            assert_eq!(time.as_u64(), start / day * day);
            assert_eq!(value, 900.0 / 1000.0 - 1.0);
        }
    }
}

fn scientific_assertions(report: &NativeExperimentEvaluationResultV1, evaluation_offset: u64) {
    let first = &report.folds[0].decisions;
    assert_eq!(
        first.iter().map(|d| d.ordinal).collect::<Vec<_>>(),
        [11, 12, 13]
    );
    assert_eq!(first[0].features[0].value, Some(0.25));
    assert_eq!(first[0].features[0].sequence, Some(oci::count(2)));
    assert_eq!(
        first[0].features[0].event_ns,
        Some(oci::count(evaluation_offset + 10 * fixture::SECOND))
    );
    assert_eq!(
        first[0].features[0].observed_available_ns,
        Some(oci::count(evaluation_offset + 12 * fixture::SECOND + 1))
    );
    assert_eq!(first[1].features[0].value, Some(0.0));
    assert_eq!(first[1].features[0].missing_reason, None);
    assert_eq!(first[1].target_weight, "0".parse().unwrap());
    assert_eq!(
        first[2].features[0].missing_reason,
        Some(FeatureMissingReasonV1::Expired)
    );
    assert_eq!(first[2].features[0].value, None);
    assert_eq!(first[2].target_weight, "0.05".parse().unwrap());
    assert_eq!(first[0].features[1].observed_available_ns, None);
    assert_eq!(
        first[0].features[1].effective_available_ns,
        Some(oci::count(evaluation_offset + 12 * fixture::SECOND))
    );
    assert_eq!(
        first[0].features[2].missing_reason,
        Some(FeatureMissingReasonV1::NotYetAvailable)
    );
    assert_eq!(first[0].features[2].event_ns, None);
    assert!(report
        .folds
        .iter()
        .flat_map(|f| &f.decisions)
        .any(|d| d.features[0].missing_reason == Some(FeatureMissingReasonV1::SourceMissing)));
    assert_eq!(report.source_row_count.get(), 40);
    assert_eq!(report.feature_count, 3);
    assert!(report.consumed_fuel.unwrap().get() > 0);
    assert_eq!(report.native_versions["nautilus-backtest"], "0.63.0");
    assert!(report.folds.len() > 1);
    for fold in &report.folds {
        assert!(fold.training_end_available_ns < fold.decisions[0].decision_ns);
        assert_eq!(
            fold.simulation_request.settings.starting_capital,
            report.request.settings.starting_capital
        );
        assert_eq!(
            serde_json::to_value(&fold.simulation_request.settings).unwrap(),
            serde_json::to_value(&report.request.settings).unwrap()
        );
        assert!(fold.simulation.orders.get() > 0);
        eprintln!(
            "native fold={} window={}..{} daily_returns={:?} status={:?}",
            fold.fold_index,
            fold.simulation_request.selection.event_start_ns.get(),
            fold.simulation_request.selection.event_end_ns.get(),
            fold.simulation.returns,
            fold.simulation.returns_status
        );
        assert_native_snapshot_returns(&fold.simulation);
        let fills = fold.simulation.canonical_result["fills"]
            .as_array()
            .unwrap();
        assert!(!fills.is_empty(), "real Nautilus fills required");
        let (_, latency) = domain::portfolio::simulation_models(&report.request.settings).unwrap();
        let delay = latency.base_latency_ns.get() + latency.insert_latency_ns.get();
        let initialized = fold.simulation.canonical_result["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|order| {
                let core = &native_variant(order)["core"];
                let event = core["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find_map(|event| event.get("Initialized"))
                    .unwrap();
                let clock = native_clock(&event["ts_init"]);
                assert!(fold
                    .decisions
                    .iter()
                    .any(|decision| decision.decision_ns.get() == clock));
                (event["client_order_id"].as_str().unwrap(), clock)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        for fill in fills {
            let submitted = initialized[fill["client_order_id"].as_str().unwrap()];
            let event = native_variant(&fill["event"]);
            assert_eq!(event["instrument_id"], report.instrument_id);
            let commission: nautilus_model::types::Money =
                serde_json::from_value(event["commission"].clone()).unwrap();
            assert_eq!(
                commission.currency.to_string(),
                report.request.settings.base_currency
            );
            assert!(commission.as_decimal() > rust_decimal::Decimal::ZERO);
            for field in ["ts_event", "ts_init"] {
                let actual = native_clock(&event[field]);
                assert!(actual >= submitted + delay);
                assert!(actual <= fold.simulation_request.selection.decision_cutoff_ns.get());
            }
        }
        for decision in &fold.decisions {
            assert_eq!(
                decision.event_ns.get(),
                evaluation_offset + fixture::event(decision.ordinal as usize)
            );
            assert_eq!(decision.decision_ns.get(), decision.event_ns.get() + 1);
            assert_eq!(
                decision.label_end_ns,
                Some(oci::count(
                    evaluation_offset + fixture::event(decision.ordinal as usize + 2)
                ))
            );
            assert_eq!(
                decision.label_available_ns,
                Some(oci::count(decision.label_end_ns.unwrap().get() + 1))
            );
            assert!(decision.decision_ns <= report.request.selection.decision_cutoff_ns);
        }
    }
    assert!(report
        .folds
        .iter()
        .flat_map(|f| &f.decisions)
        .any(|d| d.label_end_ns.unwrap().get() - d.event_ns.get() > 2 * fixture::SECOND));
}

async fn scenario(
    pool: PgPool,
    market: fixture::FixtureMarket,
    strategy: bool,
    paper_harness: Option<std::path::PathBuf>,
) {
    let native_profiles = external_support::unused_native_profiles(&pool).await;
    let cli = std::env::var_os("QUAZONAI_NATIVE_CLI_BIN")
        .expect("build the candidate portable CLI and set QUAZONAI_NATIVE_CLI_BIN");
    assert!(std::path::Path::new(&cli).is_absolute() && std::path::Path::new(&cli).is_file());
    let cookie_key = tower_sessions::cookie::Key::generate();
    let mut auth = support::fixture_with_key(pool.clone(), None, None, cookie_key.clone()).await;
    let cookie = support::local_session(&auth).await.cookie.unwrap();
    let mut remote = oci::Fixture::open().await;
    let Catalog {
        directory,
        versions,
        mut request,
        features,
        evaluation_offset,
        forward_feature,
    } = tokio::task::spawn_blocking(move || catalog(market, strategy))
        .await
        .unwrap();
    let prepared = scientific_catalog::prepare_catalog_with_paper(
        &pool,
        &mut remote,
        (directory, versions, request.settings.clone()),
        Some(request.split_policy.clone()),
        strategy,
    )
    .await;
    let data = &prepared.data;
    let store = &prepared.store;
    let actor = &prepared.actor;
    let root = data.directory.as_ref().unwrap().path();
    // Issue the real machine verifier into the same vault used by TCP and Worker.
    // Retain the original encrypted owner-session cookie and PostgreSQL session.
    auth.app = server::router(
        server::AppState::new(
            store.clone(),
            SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap(),
            server::WebPolicy::new(
                "https://localhost",
                "127.0.0.1:8080".parse().unwrap(),
                false,
            )
            .unwrap(),
        ),
        cookie_key,
    );
    request.split_policy.sealed_revision_id = data.data.sealed;
    let cycle = external_support::start(store, actor, data).await;
    assert_eq!(
        external_support::unused_native_profiles(&pool).await,
        native_profiles,
        "direct setup must not configure the migration-created native profiles"
    );
    let mut feature_ids = Vec::new();
    for part in &features {
        feature_ids.push(
            external_support::upload(
                store,
                actor,
                data,
                ResearchArtifactKind::Parameters,
                serde_json::to_string(part).unwrap(),
            )
            .await,
        );
    }
    let parameters = ExperimentEvaluationParametersV1 {
        schema_version: SchemaV1,
        dataset_revision_id: data.data.validation,
        feature_artifact_ids: feature_ids.clone(),
        instrument_id: request.instrument_id.clone(),
        feature_schema: request.feature_schema.clone(),
        label_horizon_observations: request.label_horizon_observations,
        total_fuel: request.total_fuel,
        target_ttl_ns: request.target_ttl_ns,
        decision_output: request.decision_output,
        settings: request.settings.clone(),
    };
    let parameter_id = external_support::upload(
        store,
        actor,
        data,
        ResearchArtifactKind::Parameters,
        serde_json::to_string(&parameters).unwrap(),
    )
    .await;
    let code = external_support::upload(
        store,
        actor,
        data,
        ResearchArtifactKind::Code,
        fixture::SOURCE.into(),
    )
    .await;
    let proposal = external_support::upload(store, actor, data, ResearchArtifactKind::Report, json!({"schema_version":1,"hypothesis":"Synthetic source clocks survive direct native research"}).to_string()).await;
    let family: uuid::Uuid =
        sqlx::query_scalar("SELECT family_id FROM app.evaluation_policies WHERE id=$1")
            .bind(data.brief.content.evaluation_policy_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let experiment = store
        .propose_experiment(
            actor,
            "direct-proposal",
            &ExperimentProposalV1 {
                schema_version: SchemaV1,
                cycle_id: cycle.id,
                family_id: family.to_string().try_into().unwrap(),
                parent_experiment_id: None,
                hypothesis: "As-of source clocks drive a frozen target policy".into(),
                expected_failure_modes: "Missing inputs and native costs remain visible".into(),
                proposal_artifact_id: proposal,
                parameter_artifact_id: parameter_id,
                code_artifact_id: Some(code),
            },
        )
        .await
        .unwrap()
        .resource;
    let mut compile_limits = experiment_support::limits();
    compile_limits.cpu_seconds = Some(oci::count(60));
    compile_limits.output_bytes = Some(oci::count(4 * 1024 * 1024));
    let mut evaluation_limits = compile_limits.clone();
    evaluation_limits.experiments = 0;
    let intent = ExperimentEvaluateIntent {
        schema_version: SchemaV1,
        experiment_id: experiment.id,
        request: ExperimentEvaluateV1 {
            schema_version: SchemaV1,
            expected_revision: experiment.revision,
            compile_limits,
            evaluation_limits,
        },
    };
    let principal = client::browser(&auth, &cookie, "direct-cli-principal", "/api/v2/machine-principals", json!({"schema_version":1,"name":"Direct native owner CLI","kind":"CLI","project_id":experiment.project_id,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = client::browser(&auth, &cookie, "direct-cli-credential", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), json!({"schema_version":1,"scope_codes":["RESEARCH_READ","RUN_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = auth._state.path().join("direct-token");
    fs::write(&token, credential.body["token"].as_str().unwrap()).unwrap();
    fs::set_permissions(&token, fs::Permissions::from_mode(0o600)).unwrap();
    let (origin, _listener) = client::listen_with_resources(
        store.clone(),
        SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap(),
        ArtifactStore::open(&root.join("objects")).unwrap(),
        prepared.targets.clone(),
    )
    .await;
    let id = experiment.id.to_string();
    let denied = client::invoke(
        &origin,
        &token,
        &[
            "--idempotency-key",
            "direct-command",
            "experiment",
            "evaluate",
            &id,
        ],
        serde_json::to_value(&intent.request).unwrap(),
    )
    .await;
    assert!(
        !denied.status.success(),
        "owner CLI still needs the bound operator grant"
    );
    let grant: Value = output(&client::invoke(&origin, &token, &["--idempotency-key","direct-grant","operator-grant"], json!({"schema_version":1,"command":{"operation":"EXPERIMENT_EVALUATE","request":intent},"target_id":experiment.id})).await);
    let args = [
        "--idempotency-key",
        "direct-command",
        "--operator-grant",
        grant["resource"]["id"].as_str().unwrap(),
        "experiment",
        "evaluate",
        &id,
    ];
    let admitted: CommandResult<RunSnapshotV1> = output(
        &client::invoke(
            &origin,
            &token,
            &args,
            serde_json::to_value(&intent.request).unwrap(),
        )
        .await,
    );
    let replay: CommandResult<RunSnapshotV1> = output(
        &client::invoke(
            &origin,
            &token,
            &args,
            serde_json::to_value(&intent.request).unwrap(),
        )
        .await,
    );
    assert!(!admitted.replayed);
    assert!(replay.replayed);
    assert_eq!(admitted.resource.id, replay.resource.id);
    let admitted_counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.runs WHERE cycle_id=$1),(SELECT reserved_experiments FROM app.research_cycles WHERE id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$2)",
    ).bind(cycle.id.as_uuid()).bind(admitted.resource.id.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(admitted_counts, (1, 1, 1));
    let worker = Worker::new(
        store.clone(),
        SecretVault::open(&root.join("secrets"), &root.join("master.key")).unwrap(),
        ArtifactStore::open(&root.join("objects")).unwrap(),
        prepared.targets.clone(),
        1,
    )
    .unwrap();
    let (_stop, shutdown) = tokio::sync::watch::channel(false);
    let (compiled, compile_spec, compile_message) = execute(
        &pool,
        &prepared,
        &mut remote,
        &worker,
        admitted.resource.id,
        shutdown.clone(),
    )
    .await;
    let compilation = published(
        &pool,
        &data.objects,
        compiled.id,
        compiled.active_attempt_id.unwrap(),
        "qz.model_compilation",
    )
    .await;
    let model = published(
        &pool,
        &data.objects,
        compiled.id,
        compiled.active_attempt_id.unwrap(),
        "qz.wasm_model",
    )
    .await;
    let compiler: NativeModelCompilationV1 = serde_json::from_slice(&compilation.bytes).unwrap();
    assert_eq!(compiler.code_artifact_id, code);
    assert_eq!(compiler.model_storage_ref, model.remote_storage_ref);
    assert_eq!(compiler.module_bytes.get(), model.bytes.len() as u64);
    assert_eq!(compiler.abi, FEATURE_MODEL_ABI_V2);
    assert_eq!(compiler.target, "wasm32-unknown-unknown");
    assert!(compiler.rustc_version.starts_with("rustc "));
    assert!(model.bytes.starts_with(b"\0asm"));
    assert_existing_input(&remote, &compile_spec, code, fixture::SOURCE.as_bytes()).await;
    assert_eq!(
        remote_bytes(&remote, &compile_spec, model.remote_storage_ref).await,
        model.bytes
    );
    assert_eq!(
        remote_bytes(&remote, &compile_spec, compilation.remote_storage_ref).await,
        compilation.bytes
    );
    assert!(compile_spec.inputs.iter().any(|input| matches!(input, RuntimeInputV1::Artifact { artifact_id, role: contracts::research::ArtifactInputRole::Code, .. } if *artifact_id == code)));
    let follow: ExperimentView =
        output(&client::invoke(&origin, &token, &["experiment", "show", &id], Value::Null).await);
    let evaluation = follow.run_id.unwrap();
    assert_ne!(evaluation, compiled.id);
    assert_eq!(follow.outcome, Some(ExperimentOutcome::Pending));
    assert!(
        !client::invoke(&origin, &token, &["experiment", "result", &id], Value::Null)
            .await
            .status
            .success()
    );
    // A repeated delivery must reuse the already-created continuation and charge.
    worker
        .process_message(
            compile_message,
            "direct-compile-redelivery",
            shutdown.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        store.experiment(actor, experiment.id).await.unwrap().run_id,
        Some(evaluation)
    );
    let (finished, evaluation_spec, evaluation_message) = execute(
        &pool,
        &prepared,
        &mut remote,
        &worker,
        evaluation,
        shutdown.clone(),
    )
    .await;
    let report = published(
        &pool,
        &data.objects,
        evaluation,
        finished.active_attempt_id.unwrap(),
        "qz.experiment_evaluation",
    )
    .await;
    assert_eq!(
        remote_bytes(&remote, &evaluation_spec, report.remote_storage_ref).await,
        report.bytes
    );
    let scientific: NativeExperimentEvaluationResultV1 =
        serde_json::from_slice(&report.bytes).unwrap();
    domain::execution::check_experiment_evaluation(
        &request,
        data.data.validation,
        model.artifact_id,
        &feature_ids,
        &scientific,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&scientific.request).unwrap(),
        serde_json::to_value(&request).unwrap()
    );
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(evaluation_spec.parameters_artifact_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &data
            .objects
            .read(
                evaluation_spec.parameters_artifact_id,
                oci::count(size as u64),
            )
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::EvaluateExperiment {
        model_artifact_id,
        feature_artifact_ids,
        ..
    } = task
    else {
        panic!("original native experiment task required")
    };
    assert_eq!(model_artifact_id, model.artifact_id);
    assert_eq!(feature_artifact_ids, feature_ids);
    scientific_assertions(&scientific, evaluation_offset);
    let adopted: ExperimentView =
        output(&client::invoke(&origin, &token, &["experiment", "show", &id], Value::Null).await);
    assert_eq!(adopted.run_id, Some(evaluation));
    assert_eq!(adopted.conclusion_artifact_id, Some(report.artifact_id));
    assert_eq!(adopted.outcome, Some(ExperimentOutcome::Inconclusive));
    assert_eq!(
        adopted.outcome_reason.as_deref(),
        Some("EXTERNAL_EVALUATION_COMPLETE")
    );
    let raw = client::invoke(&origin, &token, &["experiment", "result", &id], Value::Null).await;
    let readback: Value = output(&raw);
    assert_eq!(
        readback,
        serde_json::from_slice::<Value>(&report.bytes).unwrap()
    );
    let summary: ExperimentSummaryV1 = output(
        &client::invoke(
            &origin,
            &token,
            &["experiment", "summary", &id],
            Value::Null,
        )
        .await,
    );
    assert_eq!(summary.experiment_id, experiment.id);
    assert_eq!(summary.run_id, evaluation);
    assert_eq!(summary.report_artifact_id, report.artifact_id);
    assert_eq!(summary.dataset_revision_id, data.data.validation);
    assert_eq!(summary.model_artifact_id, model.artifact_id);
    assert_eq!(summary.feature_artifact_ids, feature_ids);
    assert_eq!(summary.native_versions, scientific.native_versions);
    assert_eq!(summary.instrument_id, request.instrument_id);
    assert_eq!(summary.folds.len(), scientific.folds.len());
    for (summary, fold) in summary.folds.iter().zip(&scientific.folds) {
        assert_eq!(summary.fold_index, fold.fold_index);
        assert_eq!(summary.orders, fold.simulation.orders);
        assert_eq!(summary.positions, fold.simulation.positions);
        assert_eq!(summary.base_currency, request.settings.base_currency);
        assert_eq!(summary.account_kind, request.settings.account_kind);
        assert_eq!(summary.starting_capital, request.settings.starting_capital);
        assert_eq!(
            summary.daily_return_count.get(),
            fold.simulation.returns.len() as u64
        );
        assert_summary_return_statistic(&summary.sharpe_ratio, &fold.simulation);
        assert_summary_return_statistic(&summary.max_drawdown, &fold.simulation);
        let variant = match summary.account_kind {
            NativeAccountKind::Cash => "Cash",
            NativeAccountKind::Margin => "Margin",
        };
        let account = &fold.simulation.canonical_result["accounts"][0][variant]["base"];
        assert_eq!(account["id"], summary.account_id);
        let commissions: nautilus_model::types::Money =
            serde_json::from_value(account["commissions"][&summary.base_currency].clone()).unwrap();
        assert_eq!(commissions.currency.to_string(), summary.base_currency);
        let exact_commissions: contracts::DecimalValue =
            commissions.as_decimal().to_string().parse().unwrap();
        assert!(exact_commissions.is_positive());
        assert_eq!(summary.commissions, Some(exact_commissions));
        let native_pnl = fold.simulation.statistics.iter().find(|statistic| {
            statistic.group == NativeStatisticGroup::Pnl
                && statistic.native_key == "PnL (total)"
                && statistic.currency.as_deref() == Some(summary.base_currency.as_str())
        });
        assert_eq!(
            summary.native_account_pnl.value,
            native_pnl.and_then(|statistic| statistic.value)
        );
        assert!(summary.commissions_reason.is_none());
        assert!(!summary.equity_preview.points.is_empty());
        assert!(summary.equity_preview.points.len() <= 64);
    }
    let accounting = store.cycle(actor, cycle.id).await.unwrap();
    assert_eq!(accounting.reserved_experiments, 0);
    assert_eq!(accounting.used_experiments, 1);
    assert_eq!(
        accounting.reserved_cpu_seconds.get(),
        intent.request.compile_limits.cpu_seconds.unwrap().get()
            + intent.request.evaluation_limits.cpu_seconds.unwrap().get()
    );
    let before: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.research_cycles c WHERE id=$1")
            .bind(cycle.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    worker
        .process_message(
            evaluation_message,
            "direct-result-redelivery",
            shutdown.clone(),
        )
        .await
        .unwrap();
    let late: CommandResult<RunSnapshotV1> = output(
        &client::invoke(
            &origin,
            &token,
            &args,
            serde_json::to_value(&intent.request).unwrap(),
        )
        .await,
    );
    assert!(late.replayed);
    assert_eq!(late.resource.id, compiled.id);
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(c) FROM app.research_cycles c WHERE id=$1")
            .bind(cycle.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "redelivery/readback cannot double-charge the original cycle"
    );
    let counts: (i64,i64,i64,i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs WHERE cycle_id=$1),(SELECT count(*) FROM app.run_attempts WHERE run_id IN ($2,$3)),(SELECT count(*) FROM app.external_experiment_requests WHERE experiment_id=$4),(SELECT count(*) FROM app.external_experiment_tasks WHERE experiment_id=$4),(SELECT count(*) FROM app.external_experiment_results WHERE experiment_id=$4),(SELECT used_experiments FROM app.research_cycles WHERE id=$1),(SELECT count(*) FROM app.run_missions),(SELECT count(*) FROM app.qualifications)")
        .bind(cycle.id.as_uuid()).bind(compiled.id.as_uuid()).bind(evaluation.as_uuid()).bind(experiment.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (2, 2, 1, 1, 1, 1, 0, 0));
    let repeated =
        client::invoke(&origin, &token, &["experiment", "result", &id], Value::Null).await;
    assert!(repeated.status.success());
    assert_eq!(repeated.stdout, raw.stdout);
    let unchanged = published(
        &pool,
        &data.objects,
        evaluation,
        finished.active_attempt_id.unwrap(),
        "qz.experiment_evaluation",
    )
    .await;
    assert_eq!(unchanged.artifact_id, report.artifact_id);
    assert_eq!(unchanged.bytes, report.bytes);
    assert_eq!(
        external_support::unused_native_profiles(&pool).await,
        native_profiles
    );
    if let Some(forward_feature) = forward_feature {
        let claimed = direct_strategy::research_to_claim(
            &pool,
            &prepared,
            &mut remote,
            &worker,
            shutdown,
            &adopted,
            &scientific,
            forward_feature,
        )
        .await;
        if let Some(harness) = paper_harness {
            // Keep this producer's original SQLx database, shared vault and TCP
            // listener alive until the Paper stream and its replay are read back.
            paper_account_readback::run(&pool, &auth, &cookie, &origin, &claimed, &harness, &token)
                .await;
        }
    }
    remote.assert_private_logs();
    println!(
        "direct native research: {} → original compiler/model/report → CLI raw/summary; original 2 native research Runs, 1 trial, no Agent/qualification; synthetic clocks only",
        request.instrument_id
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn direct_cli_equity_research_roundtrip(pool: PgPool) {
    tokio::time::timeout(
        Duration::from_secs(360),
        Box::pin(scenario(pool, fixture::FixtureMarket::Equity, false, None)),
    )
    .await
    .expect("joined direct Equity research deadline");
}

#[sqlx::test(migrations = "../../migrations")]
async fn direct_cli_btc_usdt_research_roundtrip(pool: PgPool) {
    tokio::time::timeout(
        Duration::from_secs(360),
        Box::pin(scenario(
            pool,
            fixture::FixtureMarket::CryptoUsdtCurrencyPair,
            false,
            None,
        )),
    )
    .await
    .expect("joined direct BTC/USDT research deadline");
}

#[sqlx::test(migrations = "../../migrations")]
async fn direct_native_strategy_research_to_claim(pool: PgPool) {
    tokio::time::timeout(
        Duration::from_secs(540),
        Box::pin(scenario(
            pool,
            fixture::FixtureMarket::CryptoUsdtCurrencyPair,
            true,
            None,
        )),
    )
    .await
    .expect(
        "native accepted Evaluate → alpha → historical/current composition → SQL claim deadline",
    );
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires same-candidate native_paper_target executable in QUAZONAI_NATIVE_PAPER_TEST_BIN, portable CLI and native OCI/SQLx prerequisites; select this exact test explicitly"]
async fn direct_native_strategy_paper_account_readback(pool: PgPool) {
    let harness = std::env::var_os("QUAZONAI_NATIVE_PAPER_TEST_BIN")
        .map(std::path::PathBuf::from)
        .expect("build this candidate's job/native_paper_target with native-paper-test and supply its Cargo-reported executable path");
    assert!(harness.is_absolute() && harness.is_file());
    tokio::time::timeout(
        Duration::from_secs(660),
        Box::pin(scenario(
            pool,
            fixture::FixtureMarket::CryptoUsdtCurrencyPair,
            true,
            Some(harness),
        )),
    )
    .await
    .expect("original native research → SQL claim → Paper → account relay/readback deadline");
}
