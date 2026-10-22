//! Original multi-day scientific TEST source; no market/PIT qualification.
use super::*;
use contracts::portfolio::NativeModelRefV1;
use contracts::{ensemble_fit::*, factors::*};
use sha2::{Digest, Sha256};

#[test]
fn export_multi_day_observed_fit_when_requested() {
    let Some(directory) = std::env::var_os("QZ_PM_OBSERVED_FIXTURE_DIR") else {
        return;
    };
    export_observed_science(
        &std::path::PathBuf::from(directory).join("science"),
        false,
        2,
    );
}

#[test]
fn export_cash_heavy_observed_fit_when_requested() {
    let Some(directory) = std::env::var_os("QZ_PM_OBSERVED_FIXTURE_DIR") else {
        return;
    };
    export_observed_science(
        &std::path::PathBuf::from(directory).join("science/cash-heavy"),
        true,
        2,
    );
}

#[test]
fn export_one_hour_cash_heavy_observed_fit_when_requested() {
    let Some(directory) = std::env::var_os("QZ_PM_OBSERVED_FIXTURE_DIR") else {
        return;
    };
    // A separate, pre-registered prediction horizon. The original two-minute
    // method and its legitimate cost-driven HOLD comparison remain unchanged.
    export_observed_science(
        &std::path::PathBuf::from(directory).join("science/cash-heavy-hour"),
        true,
        60,
    );
}

fn export_observed_science(destination: &std::path::Path, cash_heavy: bool, horizon: u32) {
    std::fs::create_dir_all(&destination).unwrap();
    let destination = std::fs::canonicalize(destination).unwrap();
    let (catalog, mut source_simulation) =
        spot::source_fixture_with_rows(if cash_heavy { 17_310 } else { 17_280 }, true);
    let heldout_end = count(START + 17_280 * MINUTE);
    if cash_heavy {
        // A distinct registered TEST scenario, fixed before any fit or replay.
        // Rates are unchanged; this is no claim about future actual venue fees.
        let NativeModelRefV1::FrozenSpotFeeScenario { parameters, .. } =
            &mut source_simulation.settings.fee_model
        else {
            unreachable!()
        };
        parameters.schedule.source.source_ref =
            format!("CONTROLLED_FORWARD_PUBLIC_RATE_SCENARIO_{}", Id::new());
        parameters.schedule.valid_until_ns = count(heldout_end.get() + 2 * 86_400_000_000_000);
    }
    // One 60s candle plus its original 500ms delivery delay. This registered
    // TEST policy permits a past mark at midnight without reading a future close.
    source_simulation
        .settings
        .multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .maximum_price_age_ns = count(65_000_000_000);
    let train_end = count(START + 4320 * MINUTE);
    let validation_end = count(START + 11520 * MINUTE);
    let dataset = Id::new();
    let mut origin_request = source_simulation.clone();
    origin_request.selection.event_end_ns = train_end;
    origin_request.selection.decision_cutoff_ns = count(train_end.get() + 500_000_000);
    origin_request.target_points.truncate(1);
    if cash_heavy {
        origin_request.target_points[0].targets[0].weight = money("0.01");
        origin_request.target_points[0].cash_weight = money("0.99");
    }
    let origin =
        job::simulation::simulate_explicit(catalog.path(), &origin_request, Some(dataset)).unwrap();
    let initial = original_initial_state(&origin);
    assert!(initial.snapshot.ts_event >= train_end);
    let mut heldout_selection = source_simulation.selection.clone();
    heldout_selection.event_end_ns = heldout_end;
    heldout_selection.decision_cutoff_ns = count(heldout_end.get() + 500_000_000);
    let heldout = NativeFactorEvaluationRequestV1 {
        schema_version: SchemaV1,
        forecast: NativeForecastRequestV1 {
            schema_version: SchemaV1,
            selection: heldout_selection,
            parameters: NativeForecastParametersV1 {
                schema_version: SchemaV1,
                fast_period: 2,
                slow_period: 3,
                label_horizon_observations: horizon,
                total_fuel: Some(count(100_000_000)),
            },
        },
        train_end_ns: train_end,
        validation_end_ns: validation_end,
    };
    let mut request = heldout.clone();
    request.forecast.selection.event_end_ns = validation_end;
    request.forecast.selection.decision_cutoff_ns = validation_end;
    let bodies = ["local.get 0", "f64.const 1 local.get 0 f64.div"];
    let texts:Vec<_> = bodies.iter().map(|body| format!("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) {body}))")).collect();
    let modules: Vec<_> = texts
        .iter()
        .map(|text| wat::parse_str(text).unwrap())
        .collect();
    let catalog_prefix =
        job::catalog_prefix::digest(catalog.path(), &request.forecast.selection).unwrap();
    let fit_request = NativeEnsembleFitRequestV1 {
        schema_version: SchemaV1,
        fit_end_ns: train_end,
        folds: [(1080, 2160), (2160, 3240), (3240, 4320)]
            .into_iter()
            .map(|(start, end)| NativeEnsembleFoldV1 {
                train_end_ns: count(START + start * MINUTE),
                validation_start_ns: count(START + start * MINUTE),
                validation_end_ns: count(START + end * MINUTE),
            })
            .collect(),
        members: modules
            .iter()
            .enumerate()
            .map(|(index, module)| NativeEnsembleFitMemberV1 {
                factor_version_id: format!("controlled-spot-factor-{index}"),
                source_evidence_ref: format!("controlled-spot-original-report-{index}"),
                report: job::factors::evaluate_factor(catalog.path(), &request, module).unwrap(),
            })
            .collect(),
    };
    let fit = job::ensemble_fit::fit(&fit_request).unwrap();
    assert_eq!(
        serde_json::to_value(&catalog_prefix).unwrap(),
        serde_json::to_value(
            job::catalog_prefix::digest(catalog.path(), &request.forecast.selection).unwrap()
        )
        .unwrap()
    );
    let frozen = job::ensemble_fit::freeze(&NativeEnsembleFreezeRequestV1 {
        schema_version: SchemaV1,
        fit: fit.clone(),
        members: fit_request
            .members
            .iter()
            .map(|member| NativeEnsembleFreezeMemberV1 {
                factor_version_id: member.factor_version_id.clone(),
                report_artifact_id: Id::new(),
                report: member.report.clone(),
            })
            .collect(),
    })
    .unwrap();
    fn copy_tree(source: &std::path::Path, dest: &std::path::Path) {
        std::fs::create_dir_all(dest).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = dest.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target)
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let persisted = destination.join(format!("controlled-spot-catalog-{dataset}"));
    copy_tree(catalog.path(), &persisted);
    // Binds the actual frozen instrument/raw responses/receive clocks/row IDs.
    let source_bytes =
        std::fs::read(persisted.join(contracts::spot_cash_source::SPOT_CANDLE_SOURCE_FILE))
            .unwrap();
    let content_ref = format!("sha256:{:x}", Sha256::digest(&source_bytes));
    let ttl = 86_400_000_000_000_u64;
    let policy = serde_json::json!({"name":"CONTROLLED_SPOT_HALF_EXPOSURE_BASELINE_NOT_LEARNED_OWNER_PREFERENCE",
        "kind":"factor-threshold","instrumentId":"BTC-SPOT.HYPERLIQUID","instrumentIds":["BTC-SPOT.HYPERLIQUID"],
        "threshold":0,"investedWeight":"0.5","targetTtlNs":ttl.to_string(),
        "simulationSettings":source_simulation.settings,"forecastMode":"price-volume"});
    let members: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            serde_json::json!({"factorVersionId":fit_request.members[index].factor_version_id,
        "sourceTrialId":index+1,"model":text})
        })
        .collect();
    let mut fitted = serde_json::json!({"fit":fit,"fitRequest":fit_request,"members":members,"request":request,"policy":policy,"catalogPrefix":catalog_prefix,
        "data":{"catalog":persisted,"request":request},"heldoutData":{"catalog":persisted,"request":heldout},"catalog":persisted,
        "fixtureProvenance":"CONTROLLED_ORIGINAL_SPOT_SOURCE_NATIVE_FIT_NOT_MARKET_PIT_OR_ACCOUNT_QUALIFICATION"});
    std::fs::write(
        destination.join("fitted.pending.json"),
        serde_json::to_vec(&fitted).unwrap(),
    )
    .unwrap();
    let output=std::process::Command::new("node")
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../research"))
        .args(["--experimental-strip-types","--input-type=module","-e",r#"
import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT} from '@earendil-works/chord/context';
import {canonical} from './src/documents.ts';
import {iterationRequest} from './plugins/native-wasm.ts';
import {replayFittedEnsemble} from './plugins/native-fitted-portfolio.ts';
const directory=process.env.QZ_PM_OBSERVED_SCIENCE_EXPORT;
const source=JSON.parse(await readFile(join(directory,'fitted.pending.json'),'utf8'));
assert.equal(canonical(iterationRequest(source.heldoutData,true)),canonical(source.request));
const replay=await replayFittedEnsemble({data:source.data,members:source.members,fit:source.fit,catalogPrefix:source.catalogPrefix,policy:source.policy,partition:'VALIDATION',binary:process.env.QZ_JOB_BINARY,
 datasetBridge:{kind:'RESEARCH_DATASET_REVISION_BRIDGE',id:process.env.QZ_PM_NATIVE_DATASET_ID,simulationSettings:source.policy.simulationSettings}},BACKGROUND_CONTEXT);
const report=replay.evidence.simulation.spot_cash_report;
assert.ok(report.daily_returns.days.filter(day=>day.value!==null).length>=2);
assert.equal(report.statistics.find(x=>x.native_key==='Calendar Coverage Ratio').value,1);
await writeFile(join(directory,'registered-policy-replay.json'),JSON.stringify(replay));
"#])
        .env("QZ_PM_OBSERVED_SCIENCE_EXPORT",&destination).env("QZ_PM_NATIVE_DATASET_ID",dataset.to_string())
        .env("QZ_JOB_BINARY",env!("CARGO_BIN_EXE_job")).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let replay: serde_json::Value = serde_json::from_slice(
        &std::fs::read(destination.join("registered-policy-replay.json")).unwrap(),
    )
    .unwrap();
    fitted["simulationRequest"] = replay["evidence"]["simulationRequest"].clone();
    fitted["simulation"] = replay["evidence"]["simulation"].clone();
    let simulation: NativeSimulationRequestV1 =
        serde_json::from_value(fitted["simulationRequest"].clone()).unwrap();
    assert!(initial.available_ns.get() < simulation.target_points[0].asof_ns.get() + ttl);
    let comparison = NativeObservedSpotComparisonRequestV1 {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        initial_state: initial.clone(),
        simulation: simulation.clone(),
    };
    let (mut study, mut objects) = study_request(&comparison);
    study.study.source_selection = request.forecast.selection.clone();
    // Preserve the registered grid phase. The source publication arrives just
    // after its first grid point, so the first usable decision is the next one.
    let anchor = simulation.target_points[0].asof_ns.get() + ttl;
    study.study.evaluation_start_ns = count(anchor);
    study.study.manual_cutoffs_ns = Some(
        (anchor..validation_end.get())
            .step_by(ttl as usize)
            .map(count)
            .collect(),
    );
    study.study.research_available_through_ns = train_end;
    study.study.mandate.rebalance_schedule.target_ttl_seconds = 86400;
    study.study.mandate.rebalance_schedule.max_input_age_seconds = 86400;
    for (index, member) in study.study.members.iter_mut().enumerate() {
        member.parameters = request.forecast.parameters.clone();
        member.target_kind = contracts::brief::TargetKind::Score;
        member.ensemble_weight = fit.members[index].weight.clone();
        let calibration = Id::new();
        member.calibration_artifact_id = Some(calibration);
        objects.insert(
            calibration,
            serde_json::to_vec(&frozen.members[index].calibration).unwrap(),
        );
        objects.insert(member.model_artifact_id, modules[index].clone());
    }
    let result =
        job::observed_spot::study(&persisted, &study, |id| Ok(objects[&id].clone())).unwrap();
    completed(&result.hold);
    assert_eq!(
        result.rebalance.status,
        NativeObservedSpotLegStatusV1::Completed,
        "{:?}",
        result.rebalance.reason
    );
    for run in [
        result.hold.result.as_ref().unwrap(),
        result
            .rebalance
            .result
            .as_ref()
            .unwrap()
            .simulation
            .as_ref()
            .unwrap(),
    ] {
        let report = run.spot_cash_report.as_ref().unwrap();
        assert!(
            report
                .daily_returns
                .days
                .iter()
                .filter(|day| day.value.is_some())
                .count()
                >= 2
        );
        assert_eq!(
            report
                .statistics
                .iter()
                .find(|metric| metric.native_key == "Calendar Coverage Ratio")
                .unwrap()
                .value,
            Some(1.0)
        );
    }
    let mut valuation_selection = request.forecast.selection.clone();
    valuation_selection.event_end_ns = initial.snapshot.ts_event;
    valuation_selection.decision_cutoff_ns = initial.snapshot.ts_event;
    let valuation = job::observed_spot::value(
        &persisted,
        &NativeObservedSpotValuationRequestV1 {
            schema_version: SchemaV1,
            dataset_revision_id: dataset,
            initial_state: initial.clone(),
            selection: valuation_selection.clone(),
            settings: simulation.settings.clone(),
        },
    )
    .unwrap();
    let objects_directory = destination.join("objects");
    std::fs::create_dir_all(&objects_directory).unwrap();
    for (id, bytes) in objects {
        std::fs::write(objects_directory.join(id.to_string()), bytes).unwrap();
    }
    let document = serde_json::json!({"evidenceKind":"CONTROLLED_NATIVE_TEST_NOT_MARKET_OR_ACCOUNT_QUALIFICATION",
        "catalog":persisted,"nativeDatasetRevisionId":dataset,"pmDatasetRevisionId":format!("controlled-pm-spot-science-{dataset}"),
        "contentRef":content_ref,"initial_state":initial,"simulation":simulation,"settings":simulation.settings,
        "valuationSelection":valuation_selection,"study":study.study,"objectsDirectory":objects_directory,"fittedSource":fitted});
    for (name, value) in [
        ("origin.json", serde_json::to_value(origin).unwrap()),
        ("study.json", serde_json::to_value(result).unwrap()),
        ("value.json", serde_json::to_value(valuation).unwrap()),
    ] {
        std::fs::write(destination.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
    std::fs::write(
        destination.join("source.pending.json"),
        serde_json::to_vec(&document).unwrap(),
    )
    .unwrap();
    std::fs::rename(
        destination.join("source.pending.json"),
        destination.join("source.json"),
    )
    .unwrap();
    if cash_heavy {
        export_cash_heavy_account(
            &destination,
            &persisted,
            dataset,
            &origin_request,
            &source_simulation,
            &document,
        );
    }
}

/// An additional original account path, not an edited balance or a selected
/// strategy. Selection and independent economic survival remain service tests.
fn export_cash_heavy_account(
    directory: &std::path::Path,
    catalog: &std::path::Path,
    dataset: Id,
    original: &NativeSimulationRequestV1,
    full_source: &NativeSimulationRequestV1,
    full_document: &serde_json::Value,
) {
    let destination = directory.to_path_buf();
    std::fs::create_dir_all(&destination).unwrap();
    let mut origin_request = original.clone();
    origin_request.target_points[0].targets[0].weight = money("0.01");
    origin_request.target_points[0].cash_weight = money("0.99");
    let origin =
        job::simulation::simulate_explicit(catalog, &origin_request, Some(dataset)).unwrap();
    let initial = original_initial_state(&origin);
    let mut current_request = full_source.clone();
    current_request.selection = serde_json::from_value(
        full_document["fittedSource"]["heldoutData"]["request"]["forecast"]["selection"].clone(),
    )
    .unwrap();
    current_request.target_points = origin_request.target_points.clone();
    let current =
        job::simulation::simulate_explicit(catalog, &current_request, Some(dataset)).unwrap();
    let current_initial = original_initial_state(&current);
    assert!(current_initial.snapshot.ts_event > initial.snapshot.ts_event);
    assert_eq!(origin.summary["orders.open"], "0");
    assert_eq!(current.summary["orders.open"], "0");
    let study: NativePortfolioStudyRequestV1 =
        serde_json::from_value(full_document["study"].clone()).unwrap();
    let mut selection = full_source.selection.clone();
    selection.decision_cutoff_ns = count(current_initial.open_orders_available_ns.get() + 1);
    selection.event_end_ns = selection.decision_cutoff_ns;
    let forward_request = NativeObservedSpotBuildRequestV1 {
        schema_version: SchemaV1,
        dataset_revision_id: dataset,
        initial_state: current_initial.clone(),
        selection,
        mandate: study.mandate,
        execution_settings: study.execution_settings,
        rolling_liquidity: study.rolling_liquidity,
        assets: study.assets,
        members: study.members,
    };
    let objects_directory =
        std::path::Path::new(full_document["objectsDirectory"].as_str().unwrap());
    let forward = job::observed_spot::build(catalog, &forward_request, |id| {
        Ok(std::fs::read(objects_directory.join(id.to_string()))?)
    })
    .unwrap();
    assert!(
        forward.target.is_some(),
        "{:?}",
        forward.target_block_reason
    );
    let target = forward.target.as_ref().unwrap();
    for (trade, target) in forward.trade_estimates.iter().zip(&target.targets) {
        assert_eq!(
            trade
                .target_notional
                .as_str()
                .parse::<bigdecimal::BigDecimal>()
                .unwrap(),
            target.weight.as_decimal() * forward.valuation.total.as_decimal()
        );
    }
    let mut document = full_document.clone();
    document["initial_state"] = serde_json::to_value(&initial).unwrap();
    document["current_initial_state"] = serde_json::to_value(&current_initial).unwrap();
    document["forwardSourceSelection"] = serde_json::to_value(&full_source.selection).unwrap();
    document["accountFixtureMethod"] =
        serde_json::json!("ORIGINAL_NATIVE_ONE_PERCENT_ENTRY_THEN_HOLD_NOT_SELECTED_POLICY");
    let mut valuation_selection: NativeBarSelectionV1 =
        serde_json::from_value(document["valuationSelection"].clone()).unwrap();
    valuation_selection.event_end_ns = initial.snapshot.ts_event;
    valuation_selection.decision_cutoff_ns = initial.snapshot.ts_event;
    document["valuationSelection"] = serde_json::to_value(&valuation_selection).unwrap();
    let value = job::observed_spot::value(
        catalog,
        &NativeObservedSpotValuationRequestV1 {
            schema_version: SchemaV1,
            dataset_revision_id: dataset,
            initial_state: initial,
            selection: valuation_selection,
            settings: origin_request.settings.clone(),
        },
    )
    .unwrap();
    for (name, value) in [
        (
            "origin-request.json",
            serde_json::to_value(origin_request).unwrap(),
        ),
        ("origin.json", serde_json::to_value(origin).unwrap()),
        (
            "current-origin-request.json",
            serde_json::to_value(current_request).unwrap(),
        ),
        (
            "current-origin.json",
            serde_json::to_value(current).unwrap(),
        ),
        ("value.json", serde_json::to_value(value).unwrap()),
        (
            "forward-current-request.json",
            serde_json::to_value(forward_request).unwrap(),
        ),
        (
            "forward-current-result.json",
            serde_json::to_value(forward).unwrap(),
        ),
        ("source.json", document),
    ] {
        std::fs::write(destination.join(name), serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
