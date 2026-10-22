//! A genuinely new native market revision for the automatic PM lifecycle test.
//! Synthetic evidence proves the pipeline, never investment performance.
#[path = "support/market.rs"]
mod market;
#[path = "support/refresh_market.rs"]
mod refresh_market;
use contracts::{ensemble_fit::*, factors::*, SchemaV1};
use market::{count, INTERVAL_NS};

#[test]
fn export_disjoint_native_pm_revision_when_requested() {
    let Some(destination) = std::env::var_os("QZ_PM_EXPORT_FIXTURE_DIR") else {
        return;
    };
    let root = std::path::PathBuf::from(destination);
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("source.json")).unwrap()).unwrap();
    let offset = 10 * 24 * 60 * INTERVAL_NS;
    let prior_end: u64 = original["heldoutData"]["request"]["forecast"]["selection"]
        ["event_end_ns"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        offset > prior_end,
        "No prior sealed row may enter the new training protocol."
    );
    let (catalog, simulation) = refresh_market::market_direction_with_size(
        "0",
        14_000,
        1.0,
        "10000000.0000",
        false,
        [
            market::execution_models::fill(),
            market::execution_models::fee(),
            market::execution_models::latency(1_000_000),
        ],
        4,
        "0.0001",
        offset,
    );
    let heldout_request = NativeFactorEvaluationRequestV1 {
        schema_version: SchemaV1,
        forecast: market::forecast_request(&simulation),
        train_end_ns: count(offset + 4320 * INTERVAL_NS),
        validation_end_ns: count(offset + 8640 * INTERVAL_NS),
    };
    let mut factor_request = heldout_request.clone();
    factor_request.forecast.selection.event_end_ns = factor_request.validation_end_ns;
    factor_request.forecast.selection.decision_cutoff_ns = factor_request.validation_end_ns;
    let catalog_prefix =
        job::catalog_prefix::digest(catalog.path(), &factor_request.forecast.selection).unwrap();
    let bodies = ["local.get 0", "f64.const 1 local.get 0 f64.div"];
    let modules: Vec<_> = bodies.iter().map(|body| market::module(body)).collect();
    let fit_request = NativeEnsembleFitRequestV1 {
        schema_version: SchemaV1,
        fit_end_ns: factor_request.train_end_ns,
        folds: [(1080, 2160), (2160, 3240), (3240, 4320)]
            .into_iter()
            .map(|(start, end)| NativeEnsembleFoldV1 {
                train_end_ns: count(offset + start * INTERVAL_NS),
                validation_start_ns: count(offset + start * INTERVAL_NS),
                validation_end_ns: count(offset + end * INTERVAL_NS),
            })
            .collect(),
        members: modules
            .iter()
            .enumerate()
            .map(|(index, module)| NativeEnsembleFitMemberV1 {
                factor_version_id: format!("next-multi-day-factor-{index}"),
                source_evidence_ref: format!("next-native-multi-day-report-{index}"),
                report: job::factors::evaluate_factor(catalog.path(), &factor_request, module)
                    .unwrap(),
            })
            .collect(),
    };
    let fit = job::ensemble_fit::fit(&fit_request).unwrap();
    assert_eq!(
        serde_json::to_value(&catalog_prefix).unwrap(),
        serde_json::to_value(
            job::catalog_prefix::digest(catalog.path(), &factor_request.forecast.selection)
                .unwrap()
        )
        .unwrap(),
        "The source prefix must remain unchanged throughout native report generation and fitting."
    );
    assert!(fit.fit_end_ns.get() > prior_end);
    let first_calibrations: Vec<_> = original["fit"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|member| member["calibrations"].as_array().unwrap().iter().map(|calibration| serde_json::json!({"intercept":calibration["intercept"], "slope":calibration["slope"]})))
        .collect();
    let next_calibrations: Vec<_> = fit
        .members
        .iter()
        .flat_map(|member| member.calibrations.iter().map(|calibration| serde_json::json!({"intercept":calibration.intercept, "slope":calibration.slope})))
        .collect();
    assert_ne!(next_calibrations, first_calibrations,
        "The changed original prices must produce fresh native calibration evidence, independent of member IDs.");
    fn copy_tree(source: &std::path::Path, destination: &std::path::Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let target = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let catalog_path = root.join("next-multi-day-micro-lot-sealed-catalog");
    copy_tree(catalog.path(), &catalog_path);
    // Same clocks/specifications as revision one, but changed original marks.
    // This is the precise counterexample to accepting metadata-only equality.
    let (counterexample, _) = refresh_market::market_direction_with_size(
        "0",
        14_000,
        1.0,
        "10000000.0000",
        false,
        [
            market::execution_models::fill(),
            market::execution_models::fee(),
            market::execution_models::latency(1_000_000),
        ],
        4,
        "0.0001",
        0,
    );
    let counterexample_path = root.join("changed-original-prefix-catalog");
    copy_tree(counterexample.path(), &counterexample_path);
    let original_request: NativeFactorEvaluationRequestV1 =
        serde_json::from_value(original["request"].clone()).unwrap();
    let expected_original = job::catalog_prefix::digest(
        std::path::Path::new(original["catalog"].as_str().unwrap()),
        &original_request.forecast.selection,
    )
    .unwrap();
    let changed_original =
        job::catalog_prefix::digest(counterexample.path(), &original_request.forecast.selection)
            .unwrap();
    assert_eq!(changed_original.row_count, expected_original.row_count);
    assert_eq!(
        changed_original.series_count,
        expected_original.series_count
    );
    assert_ne!(
        changed_original.content_sha256,
        expected_original.content_sha256
    );
    let members: Vec<_> = bodies.iter().enumerate().map(|(index, body)| serde_json::json!({
        "factorVersionId":fit_request.members[index].factor_version_id,"sourceTrialId":index+1,
        "model":format!("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) {body}))")
    })).collect();
    let source = serde_json::json!({"catalogPrefix":catalog_prefix,"counterexampleCatalog":counterexample_path,"fit":fit,"fitRequest":fit_request,"members":members,"request":factor_request,
        "policy":original["policy"],"data":{"catalog":catalog_path,"request":factor_request},
        "heldoutData":{"catalog":catalog_path,"request":heldout_request},"catalog":catalog_path,
        "fixtureProvenance":"SYNTHETIC_NEW_ORIGINAL_PARQUET_FUTURE_SUFFIX_NATIVE_REFIT_NOT_SHIFTED_REPORT"});
    std::fs::write(
        root.join("source-next.pending.json"),
        serde_json::to_vec(&source).unwrap(),
    )
    .unwrap();
    // The second registered policy also passes the existing official consumer.
    // Reports above are actual native computations, not hand-authored metrics.
    let output = std::process::Command::new("node")
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../research"))
        .args(["--experimental-strip-types", "--input-type=module", "-e", r#"
import assert from 'node:assert/strict';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { canonical } from './src/documents.ts';
import { iterationRequest } from './plugins/native-wasm.ts';
import { replayFittedEnsemble } from './plugins/native-fitted-portfolio.ts';
const root = process.env.QZ_PM_EXPORT_FIXTURE_DIR;
const source = JSON.parse(await readFile(join(root, 'source-next.pending.json'), 'utf8'));
assert.equal(canonical(iterationRequest(source.heldoutData, true)), canonical(source.request));
const replay = await replayFittedEnsemble({ data: source.data, members: source.members, fit: source.fit,
  catalogPrefix: source.catalogPrefix, policy: source.policy, partition: 'VALIDATION', binary: process.env.QZ_JOB_BINARY }, BACKGROUND_CONTEXT);
const stats = replay.evidence.simulation.statistics;
assert.equal(stats.find(value => value.native_key === 'Calendar Coverage Ratio').value, 1);
assert.ok(stats.find(value => value.native_key === 'Calendar Expected Days').value >= 2);
assert.ok(replay.evidence.simulationRequest.target_points.length > 4000);
await writeFile(join(root, 'registered-next-policy-replay.json'), JSON.stringify(replay));
"#])
        .env("QZ_JOB_BINARY", env!("CARGO_BIN_EXE_job"))
        .output().expect("Node and installed research dependencies are required for actual native evidence");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::rename(
        root.join("source-next.pending.json"),
        root.join("source-next.json"),
    )
    .unwrap();
}
