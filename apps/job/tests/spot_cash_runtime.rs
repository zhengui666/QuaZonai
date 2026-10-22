//! Controlled native CASH/fee/replay closure, never real market/account evidence.
#[path = "../../../tests/support/execution_models.rs"]
mod models;
use contracts::{
    portfolio::NativeModelRefV1, science::*, spot_cash::*, spot_fees::*, DbCounter, Id, SchemaV1,
};
const MINUTE: u64 = 60_000_000_000;
const START: u64 = 86_400_000_000_000;
fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
fn money(s: &str) -> contracts::DecimalValue {
    s.parse().unwrap()
}
#[path = "support/spot_cash.rs"]
mod spot_fixture;
#[cfg(feature = "hyperliquid-offline")]
use spot_fixture::source_fixture;
use spot_fixture::{fixture, fixture_with_delay};
// Optional evidence export is restricted to this controlled test executable.
// Production paths do not read this variable or write these files.
fn preserve_controlled_result(
    name: &str,
    request: &NativeSimulationRequestV1,
    result: &NativeSimulationResultV1,
) {
    if let Some(directory) = std::env::var_os("QZ_CONTROLLED_SPOT_EVIDENCE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let evidence = serde_json::json!({
            "evidence_kind": "CONTROLLED_NATIVE_RUST_TEST_NOT_MARKET_OR_ACCOUNT_EVIDENCE",
            "request": request,
            "result": result,
        });
        std::fs::write(directory.join(name), serde_json::to_vec(&evidence).unwrap()).unwrap();
    }
}
#[test]
fn real_native_buy_base_fee_then_sell_quote_fee_uses_actual_inventory() {
    let (dir, request, sources) = fixture(6);
    let dataset = Id::new();
    let result =
        job::simulation::simulate_spot_cash_candidate(dir.path(), &request, dataset, sources)
            .unwrap();
    let report = result.spot_cash_report.as_ref().unwrap();
    assert_eq!(report.session.dataset_revision_id, dataset);
    assert_eq!(
        report.flow_evidence.external_flow_count,
        Some(DbCounter::ZERO)
    );
    assert_eq!(
        report.fee_acceptance,
        SpotCashFeeAcceptanceV1::PublicRateScenarioUnverifiedApplicability
    );
    assert!(report.observations.iter().any(|record| matches!(
        record.record,
        contracts::spot_cash_report::NativeSpotCashObservationKindV1::Snapshot {
            origin: NativeSpotSnapshotOriginV1::DecisionBuild,
            ..
        }
    )));
    let final_point = report
        .observations
        .iter()
        .rev()
        .find_map(|record| match &record.record {
            contracts::spot_cash_report::NativeSpotCashObservationKindV1::Snapshot {
                valuation,
                ..
            } => Some(valuation),
            _ => None,
        })
        .unwrap();
    let ReportCurrencyValuationOutcomeV1::Complete { total, legs } = &final_point.outcome else {
        panic!("final valuation unavailable")
    };
    assert_eq!(total, &money("999.300245"));
    assert_eq!(
        legs.iter()
            .find(|leg| leg.currency == "BTC")
            .unwrap()
            .native_total,
        money("0")
    );
    assert!(result.returns.is_empty());
    assert!(report.daily_returns.days.is_empty());
    assert!(report
        .statistics
        .iter()
        .all(|row| row.native_key.starts_with("Calendar ")));
    let geometric = report
        .statistics
        .iter()
        .find(|row| row.native_key == "Calendar Geometric Daily Return")
        .unwrap();
    assert!(geometric.value.is_none());
    assert!(geometric.reason_code.is_some());
    assert_eq!(result.orders.get(), 2);
    preserve_controlled_result("native-buy-sell.json", &request, &result);
    // Real producer evidence must pass consumer binding, not just DTO deserialization.
    let (metrics, _) =
        domain::execution::portfolio_simulation_metrics(Id::new(), Id::new(), &request, &result)
            .unwrap();
    assert!(metrics
        .iter()
        .all(|metric| metric.status == contracts::evidence::MetricStatus::InsufficientData));
    let curve = domain::execution::portfolio_equity_curve(
        &request,
        &result,
        &contracts::equity_curve::EquityCurveQuery::default(),
    )
    .unwrap();
    assert_eq!(
        curve.points.last().unwrap().value.as_ref(),
        Some(&money("999.300245"))
    );
}
#[test]
fn old_entrypoint_stays_closed_and_policy_acceptance_is_required() {
    let (dir, mut request, sources) = fixture(6);
    assert!(job::simulation::simulate(dir.path(), &request).is_err());
    request.settings.fee_model = models::fee();
    assert!(job::simulation::simulate_spot_cash_candidate(
        dir.path(),
        &request,
        Id::new(),
        sources
    )
    .is_err());
}
#[test]
fn fee_scenario_must_cover_the_entire_owned_execution_horizon() {
    let (dir, mut request, sources) = fixture(6);
    let NativeModelRefV1::FrozenSpotFeeScenario { parameters, .. } =
        &mut request.settings.fee_model
    else {
        panic!("fee scenario missing")
    };
    parameters.schedule.valid_until_ns = count(START + MINUTE);
    assert!(job::simulation::simulate_spot_cash_candidate(
        dir.path(),
        &request,
        Id::new(),
        sources
    )
    .is_err());
}

#[test]
fn complete_utc_day_from_real_publications_feeds_official_report_statistics() {
    let (dir, request, sources) = fixture(1442);
    let result =
        job::simulation::simulate_spot_cash_candidate(dir.path(), &request, Id::new(), sources)
            .unwrap();
    let report = result.spot_cash_report.as_ref().unwrap();
    assert_eq!(report.daily_returns.days.len(), 1);
    let day = &report.daily_returns.days[0];
    assert!(day.reason.is_none());
    assert!((day.value.unwrap() - (-0.000699755)).abs() < 1e-12);
    assert_eq!(report.daily_returns.annualization_days, 365);
    let calendar = |key: &str| {
        report
            .statistics
            .iter()
            .find(|s| s.native_key == key)
            .unwrap()
            .value
    };
    assert_eq!(calendar("Calendar Coverage Ratio"), Some(1.0));
    assert_eq!(calendar("Calendar Negative Days"), Some(1.0));
    assert!(
        (calendar("Calendar Geometric Daily Return").unwrap() - day.value.unwrap()).abs() < 1e-14
    );
    assert!(result.returns.is_empty());
    assert!(result
        .statistics
        .iter()
        .all(|stat| stat.group != NativeStatisticGroup::Returns));
    let (metrics, _) =
        domain::execution::portfolio_simulation_metrics(Id::new(), Id::new(), &request, &result)
            .unwrap();
    let average = metrics
        .iter()
        .find(|m| m.metric_code == "PORTFOLIO_DAILY_RETURN_MEAN")
        .unwrap();
    assert_eq!(average.status, contracts::evidence::MetricStatus::Ok);
    assert!((average.value.unwrap() - (-0.000699755)).abs() < 1e-12);
    assert_eq!(average.scope, "portfolio.report_currency_daily");
    let mut falsified = result.clone();
    falsified
        .spot_cash_report
        .as_mut()
        .unwrap()
        .statistics
        .iter_mut()
        .find(|row| row.native_key == "Calendar Coverage Ratio")
        .unwrap()
        .value = Some(0.5);
    assert!(domain::execution::portfolio_simulation_metrics(
        Id::new(),
        Id::new(),
        &request,
        &falsified
    )
    .is_err());
    preserve_controlled_result("native-complete-utc-day.json", &request, &result);
}

#[test]
fn native_insert_at_target_expiry_boundary_remains_rejected() {
    let (dir, mut request, sources) = fixture(6);
    request.target_points[0].valid_until_ns = count(START + MINUTE);
    // Frozen latency, not an accidental wait until the next BAR, defines arrival.
    request.settings.latency_model = models::latency(MINUTE);
    let error =
        job::simulation::simulate_spot_cash_candidate(dir.path(), &request, Id::new(), sources)
            .unwrap_err();
    assert!(format!("{error:#}").contains("SIMULATION_TARGET_EXPIRES_BEFORE_INSERT"));
}

#[test]
fn final_spot_bar_order_settles_at_native_latency_without_fabricating_a_return_tail() {
    let (dir, mut request, sources) = fixture(6);
    request.target_points[1].asof_ns = count(START + 5 * MINUTE);
    request.target_points[1].valid_until_ns = count(START + 6 * MINUTE);
    let result =
        job::simulation::simulate_spot_cash_candidate(dir.path(), &request, Id::new(), sources)
            .unwrap();
    assert_eq!(result.consumed_target_points.get(), 2);
    assert_eq!(result.summary["orders.open"], "0");
    assert_eq!(result.summary["orders.inflight"], "0");
    assert_eq!(
        result.settled_replay_ns,
        Some(count(START + 5 * MINUTE + 1))
    );
    assert_eq!(
        result.canonical_result["run"]["backtest_end_ns"],
        (START + 5 * MINUTE + 1).to_string()
    );
    assert!(result
        .spot_cash_report
        .as_ref()
        .unwrap()
        .daily_returns
        .days
        .is_empty());
}

#[test]
fn stale_delayed_close_cannot_size_an_initial_buy_with_zero_base_inventory() {
    let (dir, mut request, sources) = fixture_with_delay(6, 30_000_000_000);
    request.target_points.truncate(1);
    request
        .settings
        .multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .maximum_price_age_ns = count(1_000_000_000);
    let result =
        job::simulation::simulate_spot_cash_candidate(dir.path(), &request, Id::new(), sources);
    if let Ok(result) = &result {
        preserve_controlled_result("stale-initial-buy-before-fix.json", &request, result);
    }
    assert!(
        result.is_err(),
        "stale sizing returned a successful native report"
    );
    let error = result.unwrap_err();
    assert!(
        format!("{error:#}").contains("SPOT_EXECUTION_PRICE_STALE"),
        "{error:#}"
    );
}

#[test]
fn cash_only_valuation_and_zero_exposure_do_not_require_a_stale_asset_mark() {
    let (dir, mut request, sources) = fixture_with_delay(6, 30_000_000_000);
    request.target_points.truncate(1);
    request.target_points[0].targets[0].weight = money("0");
    request.target_points[0].cash_weight = money("1");
    request
        .settings
        .multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .maximum_price_age_ns = count(1_000_000_000);
    let result =
        job::simulation::simulate_spot_cash_candidate(dir.path(), &request, Id::new(), sources)
            .unwrap();
    assert_eq!(result.orders, DbCounter::ZERO);
    let report = result.spot_cash_report.as_ref().unwrap();
    for observation in &report.observations {
        if let contracts::spot_cash_report::NativeSpotCashObservationKindV1::Snapshot {
            valuation,
            ..
        } = &observation.record
        {
            assert!(
                matches!(&valuation.outcome, ReportCurrencyValuationOutcomeV1::Complete { total, .. } if total == &money("1000"))
            );
        }
    }
}

#[path = "support/command.rs"]
mod native_command;

#[cfg(feature = "hyperliquid-offline")]
#[test]
fn explicit_cli_requires_revision_and_preserves_source_clocks_and_unverified_status() {
    let (root, request) = source_fixture();
    let missing = native_command::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
        ],
        &request,
    );
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("SPOT_DATASET_REVISION_ID_REQUIRED"));
    let id = Id::new();
    let id_text = id.to_string();
    let output = native_command::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
            "--dataset-revision-id".as_ref(),
            id_text.as_ref(),
        ],
        &request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: NativeSimulationResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    let report = result.spot_cash_report.as_ref().unwrap();
    assert_eq!(report.session.dataset_revision_id, id);
    assert_eq!(
        report
            .source_evidence
            .as_ref()
            .unwrap()
            .historical_availability,
        contracts::spot_cash_source::SpotCashHistoricalAvailabilityV1::Unverified
    );
    assert_eq!(result.orders.get(), 2);
    for observation in &report.observations {
        if let contracts::spot_cash_report::NativeSpotCashObservationKindV1::Bar {
            event_time,
            native,
            bar_open_ns,
            bar_close_ns,
            ..
        } = &observation.record
        {
            assert_eq!(*event_time, NativeSpotBarEventTimeV1::Open);
            let event = native["ts_event"]
                .as_u64()
                .or_else(|| native["ts_event"].as_str().and_then(|v| v.parse().ok()))
                .unwrap();
            assert_eq!(event, bar_open_ns.get());
            assert_eq!(
                bar_close_ns.get() + 500_000_000,
                observation.native_clock_ns.get()
            );
        }
    }
    domain::execution::portfolio_simulation_metrics(Id::new(), Id::new(), &request, &result)
        .unwrap();
}

#[cfg(feature = "hyperliquid-offline")]
#[test]
fn explicit_cli_refuses_changed_source_against_unchanged_native_catalog() {
    let (root, request) = source_fixture();
    let path = root
        .path()
        .join(contracts::spot_cash_source::SPOT_CANDLE_SOURCE_FILE);
    let mut source: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for field in ["o", "h", "l", "c"] {
        source["responses"][0]["response"][0][field] = serde_json::json!("99.00");
    }
    std::fs::write(path, serde_json::to_vec(&source).unwrap()).unwrap();
    let id = Id::new().to_string();
    let output = native_command::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
            "--dataset-revision-id".as_ref(),
            id.as_ref(),
        ],
        &request,
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SPOT_SOURCE_NATIVE_ROW_CHANGED"));
}

#[cfg(not(feature = "hyperliquid-offline"))]
#[test]
fn explicit_spot_cli_has_a_clear_missing_source_capability_failure() {
    let (root, request, _) = fixture(6);
    let id = Id::new().to_string();
    let output = native_command::command(
        &[
            "simulate".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
            "--dataset-revision-id".as_ref(),
            id.as_ref(),
        ],
        &request,
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SPOT_SOURCE_CAPABILITY_NOT_BUILT"));
}

#[cfg(feature = "hyperliquid-offline")]
#[test]
fn actual_managed_simulate_portfolio_binds_source_to_the_declared_revision() {
    managed_simulate_portfolio_binds_source_with_optional_limits(false);
}

#[cfg(feature = "hyperliquid-offline")]
#[test]
fn actual_managed_spot_cash_preserves_source_and_actual_bytes_without_optional_limits() {
    managed_simulate_portfolio_binds_source_with_optional_limits(true);
}

#[cfg(feature = "hyperliquid-offline")]
fn managed_simulate_portfolio_binds_source_with_optional_limits(unlimited: bool) {
    use contracts::{execution::*, runtime_jobs::*};
    let (catalog, request) = source_fixture();
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("input");
    let output = root.path().join("output");
    std::fs::create_dir_all(input.join("objects")).unwrap();
    std::fs::create_dir(input.join("catalogs")).unwrap();
    std::fs::create_dir(&output).unwrap();
    let dataset = Id::new();
    std::fs::rename(
        catalog.path(),
        input.join("catalogs").join(dataset.to_string()),
    )
    .unwrap();
    let parameters = NativeTaskParametersV1::SimulatePortfolio {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        request: Box::new(request.clone()),
    };
    let parameters_id = Id::new();
    let bytes = serde_json::to_vec(&parameters).unwrap();
    std::fs::write(
        input.join("objects").join(parameters_id.to_string()),
        &bytes,
    )
    .unwrap();
    let run = Id::new();
    let started =
        chrono::DateTime::from_timestamp_micros(chrono::Utc::now().timestamp_micros()).unwrap();
    // Explicit finite values serialize identically before/after the separately
    // owned optional-budget patch; this test does not redefine that interface.
    let mut spec: JobSpecV1 = serde_json::from_value(serde_json::json!({
        "schema_version":1,"run_id":run,"attempt_no":1,"owner_epoch":"1",
        "external_job_id":domain::runtime_jobs::external_id(run,1).unwrap(),
        "job_kind":parameters.job_kind(),"image_ref":format!("example.invalid/controlled@sha256:{}", "a".repeat(64)),
        "input_set_id":Id::new(),"parameters_artifact_id":parameters_id,
        "inputs":[{"kind":"DATASET","revision_id":dataset,"registered_ref":"controlled-native-source","storage_version":"1","role":"DISCOVERY"},{"kind":"ARTIFACT","artifact_id":parameters_id,"storage_version":"1","byte_count":bytes.len().to_string(),"role":"PARAMETERS"}],
        "limits":{"cpu":1,"cpu_seconds":"20","memory_mib":512,"wall_seconds":60,"output_bytes":"8388608"},
        "deadline_at":started+chrono::Duration::seconds(60),"requested_output_schemas":parameters.output_schemas()
    })).unwrap();
    if unlimited {
        spec.deadline_at = None;
        spec.limits.cpu_seconds = None;
        spec.limits.wall_seconds = None;
        spec.limits.output_bytes = None;
    }
    std::fs::write(input.join("spec.json"), serde_json::to_vec(&spec).unwrap()).unwrap();
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_job"))
        .args(["execute", "--input-root"])
        .arg(&input)
        .arg("--output-root")
        .arg(&output)
        .env_clear()
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
    let index: NativeJobOutputIndexV1 =
        serde_json::from_slice(&std::fs::read(output.join("index.json")).unwrap()).unwrap();
    let outputs = index
        .artifacts
        .into_iter()
        .map(|item| {
            let bytes = std::fs::read(output.join(item.storage_ref.to_string())).unwrap();
            assert_eq!(item.byte_count.get(), bytes.len() as u64);
            (item, bytes)
        })
        .collect::<Vec<_>>();
    domain::execution::output_bindings(&parameters, None, started, chrono::Utc::now(), &outputs)
        .unwrap();
    let report: NativeSimulationResultV1 = serde_json::from_slice(&outputs[0].1).unwrap();
    let report = report.spot_cash_report.unwrap();
    assert_eq!(report.session.dataset_revision_id, dataset);
    assert_eq!(
        report.source_evidence.unwrap().historical_availability,
        contracts::spot_cash_source::SpotCashHistoricalAvailabilityV1::Unverified
    );
}

#[cfg(all(feature = "hyperliquid-offline", feature = "catalog-prepare"))]
#[test]
fn actual_prepare_keeps_only_selected_raw_responses_and_refuses_pit_promotion() {
    use contracts::{
        catalogs::RuntimeCatalogMetadataV1, execution::NativeDatasetSelectionV1,
        spot_cash_source::*,
    };
    let (source, request) = source_fixture();
    let root = tempfile::tempdir().unwrap();
    let mut selection = NativeDatasetSelectionV1 {
        dataset_revision_id: Id::new(),
        settlements: vec![],
        selection: request.selection,
    };
    selection.selection.event_start_ns = count(START + 2 * MINUTE);
    selection.selection.event_end_ns = count(START + 5 * MINUTE);
    selection.selection.decision_cutoff_ns = count(START + 5 * MINUTE + 500_000_000);
    selection.selection.maximum_rows = 3;
    let time = |ns| chrono::DateTime::from_timestamp_nanos(ns as i64);
    let mut declaration = serde_json::json!({
        "schema_version":1,"registered_ref":"controlled-spot-source","native_snapshot_ref":"controlled-selected-native-spot","storage_version":"fixture-v1","provider_kind":"NAUTILUS_CATALOG","data_kind":"BAR","partition":"DISCOVERY",
        "event_start":time(selection.selection.event_start_ns.get()),"event_end":time(selection.selection.event_end_ns.get()),"available_through":time(selection.selection.decision_cutoff_ns.get()),
        "origin":"FIXTURE","pit_status":"UNVERIFIED","revision_policy":"AS_KNOWN_THEN","provenance_reference":"CONTROLLED_SOURCE_JSON_FIXTURE","availability_provenance":"Controlled native parser fixture only; historical public availability is not established",
        "universe":{"name":"Controlled source","calendar_ref":"synthetic-calendar","calendar_version":"1","selection_asof":time(0),"has_historical_membership":false,"coverage_start":time(0),"coverage_end":time(START+10*MINUTE),"membership":[{"instrument_id":"BTC-SPOT.HYPERLIQUID","valid_from":time(0),"valid_until":null,"available_at":time(0),"groups":null}]}
    });
    let declaration_path = root.path().join("declaration.json");
    let selection_path = root.path().join("selection.json");
    std::fs::write(&selection_path, serde_json::to_vec(&selection).unwrap()).unwrap();
    for promote in [false, true] {
        declaration["pit_status"] =
            serde_json::json!(if promote { "VERIFIED" } else { "UNVERIFIED" });
        std::fs::write(&declaration_path, serde_json::to_vec(&declaration).unwrap()).unwrap();
        let output = root
            .path()
            .join(if promote { "promoted" } else { "prepared" });
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_catalog-prepare"))
            .arg("--catalog")
            .arg(source.path())
            .arg("--declaration")
            .arg(&declaration_path)
            .arg("--selection")
            .arg(&selection_path)
            .arg("--output")
            .arg(&output)
            .env_clear()
            .output()
            .unwrap();
        if promote {
            assert!(!result.status.success());
            assert!(!output.join("catalog-metadata.json").exists());
        } else {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let metadata: RuntimeCatalogMetadataV1 =
                serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(
                metadata.pit_status,
                contracts::research::PitStatus::Unverified
            );
            let selected: FrozenSpotCandleSourceV1 = serde_json::from_slice(
                &std::fs::read(output.join("catalog").join(SPOT_CANDLE_SOURCE_FILE)).unwrap(),
            )
            .unwrap();
            assert_eq!(
                selected
                    .responses
                    .iter()
                    .map(|r| r.sequence.get())
                    .collect::<Vec<_>>(),
                vec![3, 4, 5]
            );
            assert_eq!(
                job::spot_cash_source::load_closed_rows(
                    &output.join("catalog"),
                    &selection.selection
                )
                .unwrap()
                .rows
                .len(),
                3
            );
        }
    }
}
