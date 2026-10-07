//! Independent frozen target-policy replay. Features are native CustomData;
//! generated targets use the existing Nautilus account, fills, fees and equity.
//! No supervised fitting, future labels, source-specific adapters or live orders.
use crate::{catalog::load_catalog, feature_model::FeatureModel, signals::SignalModule};
use anyhow::{ensure, Result};
use bigdecimal::BigDecimal;
use contracts::{research::DataPartition, science::*, DbCounter, DecimalValue, Id, SchemaV1};
use domain::execution::features::{effective_available_ns, FeatureState};
use nautilus_backtest::{config::BacktestEngineConfig, engine::BacktestEngine};
use nautilus_common::{
    actor::{data_actor::DataActorConfig, DataActor, DataActorCore},
    logging::logger::LoggerConfig,
    nautilus_actor,
};
use nautilus_core::UnixNanos;
use nautilus_model::{
    data::{CustomData, CustomDataTrait, Data, DataType, HasTsInit},
    identifiers::ActorId,
    instruments::{Instrument, InstrumentAny},
};
use serde::Serialize;
use std::{
    any::Any, cell::RefCell, collections::BTreeMap, fmt, path::Path, rc::Rc, str::FromStr,
    sync::Arc,
};

const EVENT_TYPE: &str = "QzFeatureEventV1";

fn count(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
enum FeaturePayload {
    Observation(FeatureObservationV1),
    Decision { ordinal: u32 },
}

/// Only the adapter-owned envelope is custom. Nautilus owns event scheduling,
/// clock advancement, subscriptions and delivery, including equal-time order.
#[derive(Clone, Debug, PartialEq, Serialize)]
struct FeatureEvent {
    event_ns: u64,
    available_ns: u64,
    payload: FeaturePayload,
}

impl HasTsInit for FeatureEvent {
    fn ts_init(&self) -> UnixNanos {
        self.available_ns.into()
    }
}

impl CustomDataTrait for FeatureEvent {
    fn type_name(&self) -> &'static str {
        EVENT_TYPE
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn ts_event(&self) -> UnixNanos {
        self.event_ns.into()
    }
    fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }
    fn clone_arc(&self) -> Arc<dyn CustomDataTrait> {
        Arc::new(self.clone())
    }
    fn eq_arc(&self, other: &dyn CustomDataTrait) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}

#[derive(Default)]
struct FeatureReplayStatus {
    decisions: Vec<NativeExperimentDecisionV1>,
    remaining_fuel: u64,
    failure: bool,
}

struct FeatureReplay {
    core: DataActorCore,
    state: FeatureState,
    model: FeatureModel,
    status: Rc<RefCell<FeatureReplayStatus>>,
}
nautilus_actor!(FeatureReplay);
impl fmt::Debug for FeatureReplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeatureReplay").finish_non_exhaustive()
    }
}

impl FeatureReplay {
    fn apply(&mut self, data: &CustomData) -> Result<()> {
        ensure!(
            !self.status.borrow().failure,
            "FEATURE_REPLAY_ALREADY_FAILED"
        );
        let event = data
            .data
            .as_any()
            .downcast_ref::<FeatureEvent>()
            .ok_or_else(|| anyhow::anyhow!("FEATURE_REPLAY_TYPE"))?;
        ensure!(
            self.core.timestamp_ns().as_u64() == event.available_ns,
            "FEATURE_REPLAY_CLOCK"
        );
        match &event.payload {
            FeaturePayload::Observation(row) => self.state.observe(row)?,
            FeaturePayload::Decision { ordinal } => {
                let features = self.state.values(count(event.available_ns)?)?;
                let value =
                    self.model
                        .predict(event.available_ns, event.event_ns, *ordinal, &features)?;
                ensure!(
                    (0.0..=1.0).contains(&value),
                    "EXPERIMENT_TARGET_WEIGHT_RANGE"
                );
                // Preserve the finite number's JSON decimal. Unsupported precision
                // is rejected by DecimalValue rather than silently rounded.
                let weight: DecimalValue = BigDecimal::from_str(&serde_json::to_string(&value)?)?
                    .to_plain_string()
                    .parse()
                    .map_err(anyhow::Error::msg)?;
                let mut status = self.status.borrow_mut();
                status.remaining_fuel = self.model.remaining_fuel();
                status.decisions.push(NativeExperimentDecisionV1 {
                    ordinal: *ordinal,
                    event_ns: count(event.event_ns)?,
                    decision_ns: count(event.available_ns)?,
                    label_end_ns: None,
                    label_available_ns: None,
                    label_return: None,
                    features,
                    target_weight: weight,
                });
            }
        }
        Ok(())
    }
}

impl DataActor for FeatureReplay {
    fn on_start(&mut self) -> Result<()> {
        self.subscribe_data(DataType::new(EVENT_TYPE, None, None), None, None);
        Ok(())
    }
    fn on_data(&mut self, data: &CustomData) -> Result<()> {
        if let Err(error) = self.apply(data) {
            // Official callbacks may log an error and continue. The owning
            // adapter checks this latch and the exact decision count after run.
            self.status.borrow_mut().failure = true;
            return Err(error);
        }
        Ok(())
    }
}

pub(crate) fn replay_features(
    schema: &[FeatureDefinitionV1],
    observations: &[FeatureObservationV1],
    decisions: &[(u32, u64, u64)],
    module: &SignalModule,
    fuel: &mut u64,
) -> Result<Vec<NativeExperimentDecisionV1>> {
    ensure!(!decisions.is_empty(), "EXPERIMENT_EMPTY_FOLD");
    let last_cutoff = decisions.last().unwrap().2;
    let mut events = Vec::with_capacity(observations.len() + decisions.len());
    for row in observations {
        let available_ns =
            effective_available_ns(&schema[usize::from(row.feature_index)], row)?.get();
        if available_ns <= last_cutoff {
            events.push(FeatureEvent {
                event_ns: row.event_ns.get(),
                available_ns,
                payload: FeaturePayload::Observation(row.clone()),
            });
        }
    }
    for &(ordinal, event_ns, available_ns) in decisions {
        events.push(FeatureEvent {
            event_ns,
            available_ns,
            payload: FeaturePayload::Decision { ordinal },
        });
    }
    // One stream, stable source-before-decision ties. Source sequence is part of
    // the declared immutable data identity, never upload/listing order.
    events.sort_by_key(|event| match &event.payload {
        FeaturePayload::Observation(row) => (
            event.available_ns,
            0,
            event.event_ns,
            row.feature_index,
            row.sequence.get(),
        ),
        FeaturePayload::Decision { ordinal } => (
            event.available_ns,
            1,
            event.event_ns,
            0,
            u64::from(*ordinal),
        ),
    });
    let status = Rc::new(RefCell::new(FeatureReplayStatus::default()));
    let actor = FeatureReplay {
        core: DataActorCore::new(DataActorConfig {
            actor_id: Some(ActorId::from("QZ-FEATURE-REPLAY")),
            ..DataActorConfig::default()
        }),
        state: FeatureState::new(schema)?,
        model: FeatureModel::new(module, u32::try_from(decisions.len())?, *fuel)?,
        status: status.clone(),
    };
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .is_colored(false)
            .build()?,
        shutdown_on_error: true,
        bypass_logging: true,
        run_analysis: false,
        ..BacktestEngineConfig::default()
    })?;
    let replay = (|| -> Result<()> {
        engine.add_actor(actor)?;
        engine.add_data(
            events
                .into_iter()
                .map(|event| {
                    Data::Custom(CustomData::new(
                        Arc::new(event),
                        DataType::new(EVENT_TYPE, None, None),
                    ))
                })
                .collect(),
            None,
            true,
            true,
        )?;
        engine.run(None, None, None, false)?;
        let observed = status.borrow();
        ensure!(!observed.failure, "FEATURE_REPLAY_CALLBACK_FAILED");
        ensure!(
            observed.decisions.len() == decisions.len(),
            "FEATURE_REPLAY_INCOMPLETE"
        );
        Ok(())
    })();
    engine.dispose();
    replay?;
    let mut observed = status.borrow_mut();
    *fuel = observed.remaining_fuel;
    Ok(std::mem::take(&mut observed.decisions))
}

/// Each fold gets a fresh model, feature actor, replay engine, trading strategy
/// and native account. The frozen policy is not fitted or calibrated on labels.
pub fn evaluate(
    root: &Path,
    request: &NativeExperimentEvaluationRequestV1,
    dataset_revision_id: Id,
    model_artifact_id: Id,
    feature_artifact_ids: &[Id],
    parts: &[FeatureObservationsV1],
    wasm: &[u8],
) -> Result<NativeExperimentEvaluationResultV1> {
    domain::execution::experiment_request(request)?;
    domain::execution::features::artifact_ids(feature_artifact_ids)?;
    ensure!(
        feature_artifact_ids.len() == parts.len(),
        "EXPERIMENT_FEATURE_PART_COUNT"
    );
    domain::execution::features::bind_observations(
        parts,
        &request.feature_schema,
        DataPartition::Validation,
    )?;
    let observations = parts
        .iter()
        .flat_map(|part| part.observations.iter().cloned())
        .collect::<Vec<_>>();
    let market = load_catalog(root, &request.selection)?;
    ensure!(market.series.len() == 1, "EXPERIMENT_ONE_TRADED_INSTRUMENT");
    let series = &market.series[0];
    ensure!(
        series.instrument.id().to_string() == request.instrument_id,
        "EXPERIMENT_INSTRUMENT_IDENTITY"
    );
    ensure!(
        matches!(
            series.instrument,
            InstrumentAny::CurrencyPair(_) | InstrumentAny::Equity(_) | InstrumentAny::BinaryOption(_)
        ),
        "EXPERIMENT_TRADED_INSTRUMENT_UNSUPPORTED"
    );
    crate::prediction::bind_target_context(root, &market, &request.selection, request.binary_option.as_ref())?;
    crate::simulation::execution_market(&market, &request.settings)?;
    let horizon = request.label_horizon_observations as usize;
    let eligible = series
        .bars
        .len()
        .checked_sub(horizon)
        .ok_or_else(|| anyhow::anyhow!("EXPERIMENT_INSUFFICIENT_LABELS"))?;
    let splits = crate::validation::validation_folds(&request.split_policy, eligible)?;
    ensure!(
        splits.len() <= MAX_EXPERIMENT_FOLDS,
        "EXPERIMENT_FOLD_LIMIT"
    );
    ensure!(
        splits.iter().map(|fold| fold.test.len()).sum::<usize>() <= MAX_EXPERIMENT_DECISIONS,
        "EXPERIMENT_DECISION_LIMIT"
    );
    let module = SignalModule::new(wasm)?;
    let mut fuel = request.total_fuel.get();
    let mut folds = Vec::with_capacity(splits.len());
    for (fold_index, split) in splits.into_iter().enumerate() {
        ensure!(
            !split.test.is_empty()
                && !split.train.is_empty()
                && split.test.windows(2).all(|w| w[1] == w[0] + 1),
            "EXPERIMENT_FOLD_CHRONOLOGY"
        );
        let first = split.test[0];
        let last = *split.test.last().unwrap();
        let train_end = series.bars[*split.train.last().unwrap() + horizon]
            .ts_init
            .as_u64();
        ensure!(
            train_end < series.bars[first].ts_init.as_u64(),
            "EXPERIMENT_TRAIN_LABEL_FROM_FUTURE"
        );
        let clocks = split
            .test
            .iter()
            .map(|&index| {
                let bar = &series.bars[index];
                Ok((
                    u32::try_from(index)?,
                    bar.ts_event.as_u64(),
                    bar.ts_init.as_u64(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            clocks.windows(2).all(|w| w[0].2 < w[1].2),
            "EXPERIMENT_DECISION_CLOCK_TIE"
        );
        let mut decisions = replay_features(
            &request.feature_schema,
            &observations,
            &clocks,
            &module,
            &mut fuel,
        )?;
        // All model calls have finished before any label close is read. Fixed
        // bars means N observed rows even if their event intervals are irregular.
        for decision in &mut decisions {
            let index = decision.ordinal as usize;
            let future = &series.bars[index + horizon];
            let label = future.close.as_f64() / series.bars[index].close.as_f64() - 1.0;
            ensure!(label.is_finite(), "EXPERIMENT_LABEL_NONFINITE");
            decision.label_end_ns = Some(count(future.ts_event.as_u64())?);
            decision.label_available_ns = Some(count(future.ts_init.as_u64())?);
            decision.label_return = Some(label);
        }
        let end = &series.bars[last + horizon];
        let mut selection = request.selection.clone();
        selection.event_start_ns = count(series.bars[first].ts_event.as_u64())?;
        selection.event_end_ns = count(
            end.ts_event
                .as_u64()
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("EXPERIMENT_TIME_RANGE"))?,
        )?;
        selection.decision_cutoff_ns =
            count(end.ts_init.as_u64().max(selection.event_end_ns.get()))?;
        let targets = decisions
            .iter()
            .map(|decision| {
                Ok(NativeTargetPointV1 {
                    schema_version: SchemaV1,
                    asof_ns: decision.decision_ns,
                    valid_until_ns: count(
                        decision
                            .decision_ns
                            .get()
                            .checked_add(request.target_ttl_ns.get())
                            .ok_or_else(|| anyhow::anyhow!("EXPERIMENT_TIME_RANGE"))?,
                    )?,
                    targets: vec![contracts::portfolio::AllocationTargetV1 {
                        instrument_id: request.instrument_id.clone(),
                        currency: request.settings.base_currency.clone(),
                        weight: decision.target_weight.clone(),
                    }],
                    cash_weight: (BigDecimal::from(1) - decision.target_weight.as_decimal())
                        .to_plain_string()
                        .parse()
                        .map_err(anyhow::Error::msg)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        for target in &targets {
            domain::prediction::binary_option_target(request.binary_option.as_ref(), target)?;
        }
        let simulation_request = NativeSimulationRequestV1 {
            schema_version: SchemaV1,
            settlements: domain::prediction::binary_option_settlements(request.binary_option.as_ref(),
                &request.instrument_id, selection.decision_cutoff_ns),
            selection,
            settings: request.settings.clone(),
            target_points: targets,
        };
        let simulation = crate::simulation::simulate(root, &simulation_request)?;
        folds.push(NativeExperimentFoldV1 {
            fold_index: u16::try_from(fold_index)?,
            training_ordinals: split
                .train
                .into_iter()
                .map(u32::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            training_end_available_ns: count(train_end)?,
            decisions,
            simulation_request,
            simulation,
        });
    }
    Ok(NativeExperimentEvaluationResultV1 {
        schema_version: SchemaV1,
        dataset_revision_id,
        model_artifact_id,
        request: request.clone(),
        native_versions: BTreeMap::from([
            ("nautilus-backtest".into(), "0.63.0".into()),
            ("wasmi".into(), "2.0.0".into()),
            ("solow-cv".into(), "0.7.3".into()),
        ]),
        feature_artifact_ids: feature_artifact_ids.to_vec(),
        instrument_id: request.instrument_id.clone(),
        consumed_fuel: count(request.total_fuel.get() - fuel)?,
        source_row_count: count(series.bars.len() as u64)?,
        feature_count: u16::try_from(request.feature_schema.len())?,
        folds,
    })
}
