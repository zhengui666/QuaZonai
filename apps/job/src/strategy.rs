//! Reuse FeatureReplay and one native target account. Historical members supply
//! their original targets; a current decision executes the frozen MODEL from its
//! original fold boundary, preserving WASM state and ordinal on every call.
use anyhow::{ensure, Result};
use bigdecimal::BigDecimal;
use contracts::{
    research::DataPartition, science::*, strategy_portfolio::*, DbCounter, Id, SchemaV1,
};
use nautilus_model::instruments::Instrument;
use std::{collections::BTreeMap, path::Path};

fn count(n: u64) -> Result<DbCounter> {
    DbCounter::new(n).map_err(anyhow::Error::msg)
}

pub fn compose(
    root: &Path,
    request: &NativeStrategyCompositionRequestV1,
    mut read: impl FnMut(Id) -> Result<Vec<u8>>,
) -> Result<NativeStrategyCompositionResultV1> {
    domain::execution::strategy_composition_request(request)?;
    let mut reports = Vec::with_capacity(request.members.len());
    for member in &request.members {
        let report: NativeExperimentEvaluationResultV1 =
            serde_json::from_slice(&read(member.policy.source.report_artifact_id)?)?;
        domain::execution::strategy::source(&member.policy, &report)?;
        reports.push(report);
    }
    let mut fuel = request.total_fuel.get();
    let outcome = match &request.purpose {
        StrategyPortfolioPurposeV1::HistoricalReplay {} => {
            let folds = request
                .members
                .iter()
                .zip(&reports)
                .map(|(member, report)| domain::execution::strategy::source(&member.policy, report))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let first = folds[0];
            // The execution catalog, account and fees are explicit. Source folds
            // retain all original decision/event clocks, without resampling.
            for fold in &folds {
                ensure!(
                    fold.decisions.len() == first.decisions.len()
                        && fold
                            .decisions
                            .iter()
                            .zip(&first.decisions)
                            .all(
                                |(a, b)| a.event_ns == b.event_ns && a.decision_ns == b.decision_ns
                            ),
                    "STRATEGY_HISTORICAL_ALIGNMENT"
                );
                ensure!(
                    fold.simulation_request.selection.event_start_ns
                        == request.selection.event_start_ns
                        && fold.simulation_request.selection.event_end_ns
                            == request.selection.event_end_ns
                        && fold.simulation_request.selection.decision_cutoff_ns
                            == request.selection.decision_cutoff_ns
                        && fold.simulation_request.selection.bar_types
                            == request.selection.bar_types,
                    "STRATEGY_HISTORICAL_EXECUTION_WINDOW"
                );
            }
            let targets = (0..first.decisions.len())
                .map(|index| {
                    domain::execution::strategy::blend(
                        &request.members,
                        &folds
                            .iter()
                            .map(|f| &f.simulation_request.target_points[index])
                            .collect::<Vec<_>>(),
                        request.mandate.target_ttl_seconds,
                    )
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let simulation_request = NativeSimulationRequestV1 {
                schema_version: SchemaV1,
                settlements: Vec::new(),
                selection: request.selection.clone(),
                settings: request.settings.clone(),
                target_points: targets,
            };
            let simulation =
                crate::simulation::simulate_strategy(root, &simulation_request, &request.mandate)?;
            StrategyCompositionOutcomeV1::HistoricalReplay {
                simulation_request: Box::new(simulation_request),
                simulation: Box::new(simulation),
            }
        }
        StrategyPortfolioPurposeV1::CurrentDecision { account_start, .. } => {
            let market = crate::catalog::load_catalog(root, &request.selection)?;
            ensure!(
                market.series.len() == 1,
                "STRATEGY_CURRENT_INSTRUMENT_COUNT"
            );
            let series = &market.series[0];
            ensure!(
                series.instrument.id().to_string() == request.members[0].policy.instrument_id,
                "STRATEGY_CURRENT_INSTRUMENT"
            );
            crate::simulation::execution_market(&market, &request.settings)?;
            let mut predictions_per_member = BTreeMap::new();
            let mut targets = Vec::with_capacity(request.members.len());
            for (member, report) in request.members.iter().zip(&reports) {
                let fold = domain::execution::strategy::source(&member.policy, report)?;
                let first = &member.policy.initialization;
                let start = series
                    .bars
                    .iter()
                    .position(|bar| {
                        bar.ts_event.as_u64() == first.first_event_ns.get()
                            && bar.ts_init.as_u64() == first.first_decision_ns.get()
                    })
                    .ok_or_else(|| anyhow::anyhow!("STRATEGY_WARMUP_START_MISSING"))?;
                let bars = &series.bars[start..];
                ensure!(
                    bars.len() > fold.decisions.len() && bars.len() <= MAX_EXPERIMENT_DECISIONS,
                    "STRATEGY_CURRENT_CONTINUATION_REQUIRED"
                );
                let clocks = bars
                    .iter()
                    .enumerate()
                    .map(|(offset, bar)| {
                        Ok((
                            first
                                .first_ordinal
                                .checked_add(u32::try_from(offset)?)
                                .ok_or_else(|| anyhow::anyhow!("STRATEGY_ORDINAL_OVERFLOW"))?,
                            bar.ts_event.as_u64(),
                            bar.ts_init.as_u64(),
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                ensure!(
                    clocks
                        .windows(2)
                        .all(|pair| pair[0].1 < pair[1].1 && pair[0].2 < pair[1].2),
                    "STRATEGY_CURRENT_CLOCK_ORDER"
                );
                ensure!(
                    fold.decisions.iter().zip(&clocks).all(
                        |(point, (ordinal, event, decision))| point.ordinal == *ordinal
                            && point.event_ns.get() == *event
                            && point.decision_ns.get() == *decision
                    ),
                    "STRATEGY_WARMUP_SOURCE_CLOCK_MISMATCH"
                );
                let parts = member
                    .feature_artifact_ids
                    .iter()
                    .map(|id| {
                        Ok(serde_json::from_slice::<FeatureObservationsV1>(&read(
                            *id,
                        )?)?)
                    })
                    .collect::<Result<Vec<_>>>()?;
                let original = member.policy.feature_artifact_ids.len();
                domain::execution::features::bind_observations(
                    &parts[..original],
                    &member.policy.feature_schema,
                    DataPartition::Validation,
                )?;
                domain::execution::features::bind_observations(
                    &parts[original..],
                    &member.policy.feature_schema,
                    DataPartition::Forward,
                )?;
                let last_original = fold.decisions.last().unwrap().decision_ns;
                let final_decision = count(clocks.last().unwrap().2)?;
                for row in parts[original..].iter().flat_map(|part| &part.observations) {
                    let available = domain::execution::features::effective_available_ns(
                        &member.policy.feature_schema[usize::from(row.feature_index)],
                        row,
                    )?;
                    ensure!(
                        available > last_original && available <= final_decision,
                        "STRATEGY_NEW_OBSERVATION_WINDOW"
                    );
                }
                let mut ids = std::collections::BTreeSet::new();
                let observations = parts
                    .iter()
                    .flat_map(|part| part.observations.iter().cloned())
                    .collect::<Vec<_>>();
                ensure!(
                    observations.len() <= MAX_FEATURE_OBSERVATIONS
                        && observations
                            .iter()
                            .all(|row| ids.insert((row.feature_index, row.sequence))),
                    "STRATEGY_FEATURE_SEQUENCE_COLLISION"
                );
                let module =
                    crate::signals::SignalModule::new(&read(member.policy.model_artifact_id)?)?;
                let decisions = crate::experiment::replay_features(
                    &member.policy.feature_schema,
                    &observations,
                    &clocks,
                    &module,
                    &mut fuel,
                )?;
                // This detects both an altered warmup stream and a hidden model
                // reset. Labels are neither accessed nor needed for new calls.
                ensure!(
                    decisions
                        .iter()
                        .zip(&fold.decisions)
                        .all(|(fresh, old)| fresh.ordinal == old.ordinal
                            && fresh.event_ns == old.event_ns
                            && fresh.decision_ns == old.decision_ns
                            && fresh.features == old.features
                            && fresh.target_weight == old.target_weight),
                    "STRATEGY_WARMUP_REPLAY_MISMATCH"
                );
                let last = decisions.last().unwrap();
                predictions_per_member
                    .insert(member.alpha_version_id, count(decisions.len() as u64)?);
                targets.push(NativeTargetPointV1 {
                    schema_version: SchemaV1,
                    asof_ns: last.decision_ns,
                    valid_until_ns: count(
                        last.decision_ns
                            .get()
                            .checked_add(member.policy.target_ttl_ns.get())
                            .ok_or_else(|| anyhow::anyhow!("STRATEGY_TARGET_TIME_RANGE"))?,
                    )?,
                    targets: vec![contracts::portfolio::AllocationTargetV1 {
                        instrument_id: member.policy.instrument_id.clone(),
                        currency: member.policy.base_currency.clone(),
                        weight: last.target_weight.clone(),
                    }],
                    cash_weight: (BigDecimal::from(1) - last.target_weight.as_decimal())
                        .to_plain_string()
                        .parse()
                        .map_err(anyhow::Error::msg)?,
                });
            }
            let target = domain::execution::strategy::blend(
                &request.members,
                &targets.iter().collect::<Vec<_>>(),
                request.mandate.target_ttl_seconds,
            )?;
            domain::execution::strategy::constraints(
                &request.mandate,
                &target,
                &vec!["0".parse().map_err(anyhow::Error::msg)?; target.targets.len()],
            )?;
            StrategyCompositionOutcomeV1::CurrentDecision {
                account_start: Box::new(account_start.clone()),
                target,
                predictions_per_member,
            }
        }
    };
    let result = NativeStrategyCompositionResultV1 {
        schema_version: SchemaV1,
        request: request.clone(),
        native_versions: BTreeMap::from([
            ("nautilus-backtest".into(), "0.63.0".into()),
            ("wasmi".into(), "2.0.0".into()),
            ("strategy-composition".into(), "1".into()),
        ]),
        consumed_fuel: count(request.total_fuel.get() - fuel)?,
        outcome,
    };
    domain::execution::strategy_composition_result(request, &result)?;
    Ok(result)
}
