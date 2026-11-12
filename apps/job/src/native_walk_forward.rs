//! Thin chronological selection over the existing Nautilus strategy runner.
//! Every train candidate and every subsequent test owns a fresh native account.
use anyhow::{Result, ensure};
use contracts::{
    DbCounter, SchemaV1, native_strategy::NativeStrategyBacktestRequestV1, native_walk_forward::*,
};
use nautilus_model::data::HasTsInit;
use std::path::Path;
fn count(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}
fn window(
    base: &NativeStrategyBacktestRequestV1,
    start: u64,
    end: u64,
) -> Result<NativeStrategyBacktestRequestV1> {
    let mut request = base.clone();
    request.available_start_ns = Some(count(start)?);
    request.available_end_ns = count(end)?;
    // Keep original event bounds: delayed historical observations remain eligible.
    // Native strategy, account and all indicator state cold-start in this window.
    Ok(request)
}
fn metric(
    result: &contracts::native_strategy::NativeStrategyBacktestResultV1,
    objective: &NativeWalkForwardObjectiveV1,
) -> Result<f64> {
    let statistics = &result.canonical_result["statistics"];
    let mut values = Vec::new();
    if objective.currency.is_none() {
        for group in ["general", "returns"] {
            if let Some(value) = statistics[group].get(&objective.native_key) {
                values.push(value);
            }
        }
    }
    if let Some(currencies) = statistics["pnls"].as_object() {
        for (currency, metrics) in currencies {
            if objective
                .currency
                .as_ref()
                .is_none_or(|wanted| wanted == currency)
            {
                if let Some(value) = metrics.get(&objective.native_key) {
                    values.push(value);
                }
            }
        }
    }
    ensure!(
        values.len() == 1,
        "WALK_FORWARD_METRIC_MISSING_OR_AMBIGUOUS"
    );
    // Upstream canonical statistics encode finite IEEE754 values as 16 hex digits.
    let encoded = values[0]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("WALK_FORWARD_NATIVE_METRIC_ENCODING"))?;
    ensure!(encoded.len() == 16, "WALK_FORWARD_METRIC_UNAVAILABLE");
    let value = f64::from_bits(u64::from_str_radix(encoded, 16)?);
    ensure!(value.is_finite(), "WALK_FORWARD_METRIC_UNAVAILABLE");
    Ok(value)
}
pub fn run(
    root: &Path,
    request: NativeStrategyWalkForwardRequestV1,
) -> Result<NativeStrategyWalkForwardResultV1> {
    let first = request
        .candidates
        .first()
        .ok_or_else(|| anyhow::anyhow!("WALK_FORWARD_CANDIDATES_REQUIRED"))?;
    ensure!(
        !request.objective.native_key.is_empty(),
        "WALK_FORWARD_METRIC_REQUIRED"
    );
    let mut shared = serde_json::to_value(first)?;
    shared.as_object_mut().unwrap().remove("strategy");
    for candidate in &request.candidates {
        let mut comparison = serde_json::to_value(candidate)?;
        comparison.as_object_mut().unwrap().remove("strategy");
        ensure!(
            comparison == shared,
            "WALK_FORWARD_SHARED_DATA_AND_ASSUMPTIONS_REQUIRED"
        );
    }
    let catalog = crate::catalog::open_catalog(root)?;
    let mut clocks = std::collections::BTreeSet::new();
    for selector in &first.selectors {
        let query =
            nautilus_persistence::catalog::types::CatalogQuery::new(selector.data_type.parse()?)
                .with_identifiers(Some(vec![selector.identifier.clone()]))
                .with_range(
                    Some(
                        first
                            .available_start_ns
                            .unwrap_or(first.event_start_ns)
                            .get()
                            .into(),
                    ),
                    Some(first.available_end_ns.get().into()),
                );
        let mut session = crate::catalog::file_batch_session(
            &catalog,
            query,
            Some(first.event_start_ns.get()..first.event_end_ns.get()),
            4096,
        )?;
        while let Some(batch) = session.next_batch()? {
            for index in 0..batch.len() {
                clocks.insert(batch.get(index).unwrap().ts_init().as_u64());
            }
        }
    }
    let clocks = clocks.into_iter().collect::<Vec<_>>();
    let splits =
        domain::execution::validation::native_walk_forward_folds(&request.split, clocks.len())?;
    let mut folds = Vec::new();
    for (index, split) in splits.into_iter().enumerate() {
        let train_start = clocks[split.train[0]];
        let train_end = clocks[*split.train.last().unwrap()];
        let test_start = clocks[split.test[0]];
        let test_end = clocks[*split.test.last().unwrap()];
        let mut trials = Vec::new();
        let mut winner: Option<(usize, f64)> = None;
        for (candidate_index, base) in request.candidates.iter().enumerate() {
            let candidate = window(base, train_start, train_end)?;
            match crate::native_strategy::run(root, candidate) {
                Ok(result) => {
                    let score = metric(&result, &request.objective);
                    if let Ok(value) = score.as_ref() {
                        let better =
                            winner.is_none_or(|(_, old)| match request.objective.direction {
                                NativeObjectiveDirectionV1::Maximize => *value > old,
                                NativeObjectiveDirectionV1::Minimize => *value < old,
                            });
                        if better {
                            winner = Some((candidate_index, *value));
                        }
                    }
                    trials.push(NativeWalkForwardTrialV1 {
                        candidate_index,
                        metric_value: score.as_ref().ok().copied(),
                        error: score.err().map(|e| e.to_string()),
                        result: Some(result),
                    });
                }
                Err(error) => trials.push(NativeWalkForwardTrialV1 {
                    candidate_index,
                    metric_value: None,
                    error: Some(format!("{error:#}")),
                    result: None,
                }),
            }
        }
        // No test values exist until the winner is selected exclusively from past runs.
        let (test_result, status, reason) = if let Some((selected, _)) = winner {
            let test_request = window(&request.candidates[selected], test_start, test_end)?;
            match crate::native_strategy::run(root, test_request) {
                Ok(result) => (Some(result), "COMPLETED", None),
                Err(error) => (None, "TEST_FAILED", Some(format!("{error:#}"))),
            }
        } else {
            (
                None,
                "NOT_EVALUATED",
                Some("NO_COMPARABLE_TRAINING_CANDIDATE".into()),
            )
        };
        folds.push(NativeWalkForwardFoldV1 {
            index,
            train_start_ns: count(train_start)?,
            train_end_ns: count(train_end)?,
            test_start_ns: count(test_start)?,
            test_end_ns: count(test_end)?,
            trials,
            selected_candidate_index: winner.map(|v| v.0),
            test_result,
            status: status.into(),
            reason,
        });
    }
    let metadata_note = if matches!(
        first.instrument_definition,
        Some(contracts::native_strategy::NativeInstrumentDefinitionV1::StaticScenario { .. })
    ) {
        "HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED: every train/test result is conditional on the same explicitly selected original definition; chronological separation does not verify historical specifications."
    } else {
        "Instrument definitions are selected by catalog availability at each window start; this is not an independent source-history audit."
    };
    Ok(NativeStrategyWalkForwardResultV1 { schema_version: SchemaV1, request, native_version: crate::NAUTILUS_VERSION.into(), splitter_version: "solow-cv 0.7.3 TimeSeriesSplit".into(), performance_scope: "INDEPENDENT_COLD_START_TEST_WINDOW_NATIVE_STATISTICS".into(), notes: vec![metadata_note.into(), "Each train/test cold-starts the unchanged compiled upstream strategy, account and indicators. No market data or positions carry across windows; indicators initialize using only rows available within the window.".into(), "Original source event bounds are preserved; inclusive availability bounds select complete same-ts_init batches. An availability gap is not an inferred label/position purge horizon.".into(), "Native PnL and return statistic definitions are unchanged, including native realized-position return semantics and unavailable statistics. These are not newly synthesized daily equity returns or an automatic OOS qualification score.".into(), "Only finite unambiguous native training metrics select candidates. Exact ties use the earlier declared candidate. Failed or unavailable trials remain recorded; test results never re-rank candidates. Multiple trials and repeated inspections can still overfit.".into()], folds })
}
