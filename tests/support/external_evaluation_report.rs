//! Controlled protocol evidence for adoption/HTTP/CLI tests only. No Wasmi,
//! Nautilus, market replay, statistical performance or qualification is claimed.
//! The minimal account/snapshot envelope follows domain/tests/native_portfolio_outputs.
#![allow(dead_code)]
use contracts::{
    DbCounter, Id, Revision, SchemaV1,
    evidence::MetricStatus,
    execution::NativeTaskParametersV1,
    portfolio::AllocationTargetV1,
    runtime_jobs::{RuntimeOutputV1, native_output_contract},
    science::*,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).expect("controlled fixture counter")
}

fn snapshot(settings: &NativeSimulationSettingsV1, time: u64) -> Value {
    json!({
        "account_id":"SIM-001",
        "account_type":match settings.account_kind { NativeAccountKind::Cash => "CASH", NativeAccountKind::Margin => "MARGIN" },
        "base_currency":settings.base_currency,
        "total_equity":[format!("{} {}", settings.starting_capital.as_decimal().to_plain_string(), settings.base_currency)],
        "ts_event":time.to_string(),
    })
}

fn simulation(request: &NativeSimulationRequestV1, row_count: u64) -> NativeSimulationResultV1 {
    let settings = &request.settings;
    let start = request.target_points[0].asof_ns.get();
    let end = request.selection.decision_cutoff_ns.get();
    let (variant, kind) = match settings.account_kind {
        NativeAccountKind::Cash => ("Cash", "CASH"),
        NativeAccountKind::Margin => ("Margin", "MARGIN"),
    };
    let summary = BTreeMap::from([
        ("venues.total".into(), "1".into()),
        ("orders.open".into(), "0".into()),
        ("orders.inflight".into(), "0".into()),
    ]);
    let balance = format!(
        "{} {}",
        settings.starting_capital.as_decimal().to_plain_string(),
        settings.base_currency
    );
    let base = json!({"id":"SIM-001", "account_type":kind, "base_currency":settings.base_currency,
        "balances_starting":BTreeMap::from([(settings.base_currency.clone(), balance)])});
    let account = BTreeMap::from([(variant, json!({"base":base}))]);
    NativeSimulationResultV1 {
        spot_cash_report: None,
        schema_version: SchemaV1,
        native_version: "0.63.0".into(),
        iterations: count(row_count),
        events: DbCounter::ZERO,
        orders: DbCounter::ZERO,
        positions: DbCounter::ZERO,
        consumed_target_points: count(request.target_points.len() as u64),
        summary: summary.clone(),
        statistics: Vec::new(),
        returns_kind: NativeReturnsKind::PortfolioDaily,
        returns_status: MetricStatus::InsufficientData,
        returns_reason: Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE".into()),
        returns: Vec::new(),
        canonical_result: json!({
            "schema":"nautilus-backtest-result/v1",
            "run":{"outcome":"completed", "iterations":row_count.to_string(),
                "total_events":"0", "total_orders":"0", "total_positions":"0",
                "backtest_start_ns":start.to_string(), "backtest_end_ns":end.to_string()},
            "summary":summary, "accounts":[account],
            "portfolio_snapshots":[snapshot(settings,end)],
        }),
    }
}

/// Construct only a controlled legal report for a frozen EvaluateExperiment
/// task. `minimum_bytes` grows ordinary per-fold snapshots at the declared
/// snapshot interval; it never appends opaque strings or unknown schema fields.
/// Panics if the given bounded task cannot supply the requested fixture size.
pub fn report(
    task: &NativeTaskParametersV1,
    minimum_bytes: Option<usize>,
) -> NativeExperimentEvaluationResultV1 {
    let NativeTaskParametersV1::EvaluateExperiment {
        dataset_revision_id,
        model_artifact_id,
        feature_artifact_ids,
        request,
        ..
    } = task
    else {
        panic!("controlled report requires EvaluateExperiment");
    };
    domain::execution::experiment_request(request).expect("valid frozen fixture request");
    assert!(minimum_bytes.is_none_or(|size| size <= 64 * 1024 * 1024));
    let split = &request.split_policy;
    let horizon = u64::from(request.label_horizon_observations);
    let minimum_rows = split.train_size.get()
        + split.purge_observations.get()
        + split.embargo_observations.get()
        + split.test_size.get()
        + horizon;
    // This explicit fixture workload keeps the regression small; it is not a
    // production input, output or experiment budget.
    let maximum_folds = (10_000_u64 / split.test_size.get()).min(32);
    assert!(maximum_folds > 0);
    let maximum_rows = minimum_rows + (maximum_folds - 1) * split.step_size.unwrap().get();
    let rows = if minimum_bytes.is_some() {
        maximum_rows
    } else {
        minimum_rows
    }
    .min(u64::from(request.selection.maximum_rows));
    assert!(
        rows >= minimum_rows,
        "insufficient controlled source rows for a fold"
    );
    let start = request.selection.event_start_ns.get();
    let span = request.selection.event_end_ns.get() - start - 1;
    // Sparse original observations are legal. This controlled clock is at most
    // one minute apart; it never widens the frozen event or availability bounds.
    let spacing = (span / (rows - 1)).min(60_000_000_000);
    assert!(spacing > 0, "fixture requires distinct source event clocks");
    let event = |ordinal: usize| start + ordinal as u64 * spacing;
    let splits = domain::execution::validation::validation_folds(
        &request.split_policy,
        (rows - horizon) as usize,
    )
    .expect("controlled folds within native count limits");
    let mut folds = Vec::with_capacity(splits.len());
    for (fold_index, indices) in splits.into_iter().enumerate() {
        let decisions: Vec<_> = indices
            .test
            .iter()
            .map(|&ordinal| NativeExperimentDecisionV1 {
                ordinal: ordinal as u32,
                event_ns: count(event(ordinal)),
                decision_ns: count(event(ordinal) + 1),
                label_end_ns: Some(count(event(ordinal + horizon as usize))),
                label_available_ns: Some(count(event(ordinal + horizon as usize) + 1)),
                label_return: Some(0.0),
                features: request
                    .feature_schema
                    .iter()
                    .map(|_| FeatureValueV1 {
                        value: None,
                        missing_reason: Some(FeatureMissingReasonV1::NotYetAvailable),
                        event_ns: None,
                        observed_available_ns: None,
                        effective_available_ns: None,
                        sequence: None,
                    })
                    .collect(),
                target_weight: "0".parse().unwrap(),
            })
            .collect();
        let last = decisions.last().unwrap();
        let replay = NativeSimulationRequestV1 {
            schema_version: SchemaV1,
            settlements: Vec::new(),
            selection: NativeBarSelectionV1 {
                schema_version: SchemaV1,
                bar_types: request.selection.bar_types.clone(),
                event_start_ns: decisions[0].event_ns,
                event_end_ns: count(last.label_end_ns.unwrap().get() + 1),
                decision_cutoff_ns: last.label_available_ns.unwrap(),
                maximum_rows: request.selection.maximum_rows,
            },
            settings: request.settings.clone(),
            target_points: decisions
                .iter()
                .map(|point| NativeTargetPointV1 {
                    schema_version: SchemaV1,
                    asof_ns: point.decision_ns,
                    valid_until_ns: count(
                        point
                            .decision_ns
                            .get()
                            .checked_add(request.target_ttl_ns.get())
                            .expect("fixture target lifetime"),
                    ),
                    targets: vec![AllocationTargetV1 {
                        instrument_id: request.instrument_id.clone(),
                        currency: request.settings.base_currency.clone(),
                        weight: "0".parse().unwrap(),
                    }],
                    cash_weight: "1".parse().unwrap(),
                })
                .collect(),
        };
        let native = simulation(&replay, decisions.len() as u64 + horizon);
        folds.push(NativeExperimentFoldV1 {
            fold_index: fold_index as u16,
            training_end_available_ns: count(
                event(*indices.train.last().unwrap() + horizon as usize) + 1,
            ),
            training_ordinals: indices.train.into_iter().map(|n| n as u32).collect(),
            decisions,
            simulation_request: replay,
            simulation: native,
        });
    }
    let mut result = NativeExperimentEvaluationResultV1 {
        schema_version: SchemaV1,
        dataset_revision_id: *dataset_revision_id,
        model_artifact_id: *model_artifact_id,
        request: (**request).clone(),
        feature_artifact_ids: feature_artifact_ids.clone(),
        instrument_id: request.instrument_id.clone(),
        native_versions: BTreeMap::from([
            ("nautilus-backtest".into(), "0.63.0".into()),
            ("solow-cv".into(), "0.7.3".into()),
            ("wasmi".into(), "2.0.0".into()),
        ]),
        consumed_fuel: Some(count(1)),
        source_row_count: count(rows),
        feature_count: request.feature_schema.len() as u16,
        folds,
    };
    if let Some(minimum) = minimum_bytes {
        let mut size = serde_json::to_vec(&result).unwrap().len();
        let interval = u64::from(request.settings.snapshot_interval_ms) * 1_000_000;
        for index in 0..result.folds.len() {
            if size >= minimum {
                break;
            }
            let fold = &result.folds[index];
            let begin = fold.decisions[0].decision_ns.get();
            let end = fold.simulation_request.selection.decision_cutoff_ns.get();
            let count = (end - begin) / interval;
            assert!(count <= 1_000_000, "controlled snapshot count bound");
            for chunk in (1..=count).collect::<Vec<_>>().chunks(256) {
                for &offset in chunk {
                    let at = begin + offset * interval;
                    if at < end {
                        result.folds[index].simulation.canonical_result["portfolio_snapshots"]
                            .as_array_mut()
                            .unwrap()
                            .push(snapshot(&request.settings, at));
                    }
                }
                size = serde_json::to_vec(&result).unwrap().len();
                if size >= minimum {
                    break;
                }
            }
        }
        assert!(
            size >= minimum,
            "frozen fixture windows cannot produce requested report size"
        );
        for fold in &mut result.folds {
            fold.simulation.canonical_result["portfolio_snapshots"]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|row| row["ts_event"].as_str().unwrap().parse::<u64>().unwrap());
        }
    }
    // Exercise the real immutable-output validator before any test can adopt or
    // serve these bytes; names and all identities remain the task's exact ones.
    let bytes = serde_json::to_vec(&result).unwrap();
    let contract = native_output_contract("qz.experiment_evaluation", "1").unwrap();
    let descriptor = RuntimeOutputV1 {
        kind: contract.kind,
        schema: contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.experiment_evaluation".into(),
            version: "1".into(),
        },
        storage_ref: Id::new(),
        storage_version: Revision::INITIAL,
        byte_count: count(bytes.len() as u64),
        media_type: contract.media_type.into(),
    };
    let now = chrono::DateTime::from_timestamp(1_577_923_200, 0).unwrap();
    domain::execution::output_bindings(task, None, now, now, &[(descriptor, bytes)])
        .expect("controlled protocol report must pass exact native output binding");
    result
}
