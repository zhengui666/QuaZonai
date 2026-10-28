//! Bounded-memory evaluation of the explicit derived cohort representation.
use super::*;
use std::collections::VecDeque;

#[derive(Default)]
struct Pearson {
    count: u64,
    first: Option<(f64, f64)>,
    varying_score: bool,
    varying_label: bool,
    nonfinite: bool,
    mean_score: f64,
    mean_label: f64,
    score_m2: f64,
    label_m2: f64,
    covariance: f64,
}

impl Pearson {
    fn push(&mut self, score: f64, label: f64) -> Result<()> {
        self.count = self
            .count
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("TICK_COUNT_RANGE"))?;
        if let Some((first_score, first_label)) = self.first {
            self.varying_score |= score != first_score;
            self.varying_label |= label != first_label;
        } else {
            self.first = Some((score, label));
        }
        self.nonfinite |= !score.is_finite() || !label.is_finite();
        let delta_score = score - self.mean_score;
        let delta_label = label - self.mean_label;
        self.mean_score += delta_score / self.count as f64;
        self.mean_label += delta_label / self.count as f64;
        self.score_m2 += delta_score * (score - self.mean_score);
        self.label_m2 += delta_label * (label - self.mean_label);
        self.covariance += delta_score * (label - self.mean_label);
        Ok(())
    }

    fn finish(&self) -> NativeFactorIcV1 {
        use NativeFactorIcStatusV1::*;
        let absent = |status| NativeFactorIcV1 {
            value: None,
            status,
        };
        if self.nonfinite {
            return absent(NonfiniteData);
        }
        if self.count < 2 {
            return absent(InsufficientSamples);
        }
        if !self.varying_score {
            return absent(ConstantScore);
        }
        if !self.varying_label {
            return absent(ConstantLabel);
        }
        let value = self.covariance / self.score_m2.sqrt() / self.label_m2.sqrt();
        if !self.score_m2.is_finite()
            || !self.label_m2.is_finite()
            || !self.covariance.is_finite()
            || !value.is_finite()
        {
            return absent(NonfiniteResult);
        }
        NativeFactorIcV1 {
            value: Some(value),
            status: Available,
        }
    }
}

#[derive(Default)]
struct Metric {
    observations: u64,
    warmup: u64,
    incomplete: u64,
    crossing: u64,
    pairs: Pearson,
}

impl Metric {
    fn push(
        &mut self,
        prediction: Option<f64>,
        label: Option<f64>,
        reason: Option<NativeFactorExclusionReasonV1>,
    ) -> Result<()> {
        self.observations = self
            .observations
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("TICK_COUNT_RANGE"))?;
        match reason {
            Some(NativeFactorExclusionReasonV1::IndicatorWarmup) => self.warmup += 1,
            Some(NativeFactorExclusionReasonV1::LabelNotComplete) => self.incomplete += 1,
            Some(NativeFactorExclusionReasonV1::LabelCrossesPartition) => self.crossing += 1,
            Some(NativeFactorExclusionReasonV1::PredictionOutsidePartition) => {
                anyhow::bail!("TICK_EVENT_OUTSIDE_SELECTION")
            }
            None => self.pairs.push(
                prediction.ok_or_else(|| anyhow::anyhow!("TICK_PREDICTION_MISSING"))?,
                label.ok_or_else(|| anyhow::anyhow!("TICK_LABEL_MISSING"))?,
            )?,
        }
        Ok(())
    }

    fn finish(&self) -> Result<NativeFactorMetricV1> {
        Ok(NativeFactorMetricV1 {
            counts: NativeFactorCountsV1 {
                observations: counter(self.observations)?,
                included: counter(self.pairs.count)?,
                excluded: counter(self.observations - self.pairs.count)?,
                indicator_warmup: counter(self.warmup)?,
                incomplete_labels: counter(self.incomplete)?,
                cross_boundary_labels: counter(self.crossing)?,
                prediction_outside_partition: counter(0)?,
            },
            pearson_ic: self.pairs.finish(),
        })
    }
}

const PARTITIONS: [NativeFactorPartitionV1; 3] = [
    NativeFactorPartitionV1::Train,
    NativeFactorPartitionV1::Validation,
    NativeFactorPartitionV1::Heldout,
];

fn partition_index(
    request: &NativeTradeTickFactorEvaluationRequestV1,
    event: u64,
) -> Result<usize> {
    let partition = partition_for_event(request, counter(event)?)
        .ok_or_else(|| anyhow::anyhow!("TICK_EVENT_OUTSIDE_SELECTION"))?;
    Ok(match partition {
        NativeFactorPartitionV1::Train => 0,
        NativeFactorPartitionV1::Validation => 1,
        NativeFactorPartitionV1::Heldout => 2,
    })
}

struct Pending {
    ordinal: u32,
    mark: f64,
    prediction: f64,
    partition: usize,
}

struct AssetSummary {
    metrics: [Metric; 3],
    predictions: u64,
    trades: u64,
}

struct Budget {
    fuel: Option<u64>,
    source_rows: u64,
}

fn record(
    metrics: &mut [Metric; 3],
    pooled: &mut [Metric; 3],
    partition: usize,
    prediction: Option<f64>,
    label: Option<f64>,
    reason: Option<NativeFactorExclusionReasonV1>,
) -> Result<()> {
    metrics[partition].push(prediction, label, reason)?;
    pooled[partition].push(prediction, label, reason)
}

fn evaluate_asset(
    request: &NativeTradeTickFactorEvaluationRequestV1,
    readers: (
        impl Iterator<Item = Result<Cohort>>,
        Option<&mut dyn Iterator<Item = Result<Cohort>>>,
    ),
    cohort_count: u64,
    module: &SignalModule,
    budget: &mut Budget,
    pooled: &mut [Metric; 3],
) -> Result<AssetSummary> {
    let (mut rows, mut label_rows) = readers;
    let mut summary = AssetSummary {
        metrics: std::array::from_fn(|_| Metric::default()),
        predictions: 0,
        trades: 0,
    };
    if cohort_count == 0 {
        if let Some(row) = rows.next() {
            row?;
            anyhow::bail!("TICK_CACHE_COHORT_COUNT_MISMATCH");
        }
        return Ok(summary);
    }
    ensure!(cohort_count <= i32::MAX as u64 + 1, "TICK_ORDINAL_LIMIT");
    let mut model = FeatureModel::new(module, Some(cohort_count), budget.fuel)?;
    let mut fast =
        ExponentialMovingAverage::new(request.parameters.fast_period_cohorts as usize, None);
    let mut slow =
        ExponentialMovingAverage::new(request.parameters.slow_period_cohorts as usize, None);
    let mut future: Option<Cohort> = None;
    let mut pending = VecDeque::<Pending>::new();
    let mut previous: Option<Cohort> = None;
    let mut seen = 0_u64;
    for current in rows {
        let current = current?;
        ensure!(seen < cohort_count, "TICK_CACHE_COHORT_COUNT_MISMATCH");
        ensure!(
            current.count > 0 && current.count <= request.sampling.maximum_cohort_rows as usize,
            "TICK_COHORT_ROW_LIMIT"
        );
        ensure!(
            current.vwap.is_finite()
                && current.vwap > 0.0
                && current.quantity.is_finite()
                && current.quantity > 0.0,
            "TICK_FEATURE_NONFINITE"
        );
        if let Some(previous) = &previous {
            ensure!(current.event > previous.event, "TICK_CACHE_EVENT_ORDER");
        }
        let trades = u64::try_from(current.count)?;
        ensure!(trades <= budget.source_rows, "TICK_SOURCE_ROW_LIMIT");
        budget.source_rows -= trades;
        summary.trades = summary
            .trades
            .checked_add(trades)
            .ok_or_else(|| anyhow::anyhow!("TICK_COUNT_RANGE"))?;
        let partition = partition_index(request, current.event)?;
        fast.update_raw(current.vwap);
        slow.update_raw(current.vwap);
        let ordinal = u32::try_from(seen)?;
        let prediction = if let Some(previous) = previous
            .as_ref()
            .filter(|_| fast.initialized() && slow.initialized())
        {
            let features = [
                feature(current.vwap, current.event, ordinal)?,
                feature(previous.vwap, previous.event, ordinal)?,
                feature(fast.value(), current.event, ordinal)?,
                feature(slow.value(), current.event, ordinal)?,
                feature(current.quantity, current.event, ordinal)?,
                feature(current.count as f64, current.event, ordinal)?,
                feature(
                    (current.event - previous.event) as f64,
                    current.event,
                    ordinal,
                )?,
            ];
            summary.predictions += 1;
            Some(model.predict(current.event, current.event, ordinal, &features)?)
        } else {
            record(
                &mut summary.metrics,
                pooled,
                partition,
                None,
                None,
                Some(NativeFactorExclusionReasonV1::IndicatorWarmup),
            )?;
            None
        };
        // The model returns before either reader supplies a future label.
        match request.target {
            NativeTickTargetV1::FixedTradeCohorts { horizon_cohorts } => {
                if pending.front().is_some_and(|point| {
                    u64::from(point.ordinal) + u64::from(horizon_cohorts) == seen
                }) {
                    let point = pending.pop_front().expect("front was present");
                    let label = current.vwap / point.mark - 1.0;
                    ensure!(label.is_finite(), "TICK_LABEL_NONFINITE");
                    let (_, end) = partition_bounds(request, PARTITIONS[point.partition]);
                    let reason = (current.event >= end.get())
                        .then_some(NativeFactorExclusionReasonV1::LabelCrossesPartition);
                    record(
                        &mut summary.metrics,
                        pooled,
                        point.partition,
                        Some(point.prediction),
                        Some(label),
                        reason,
                    )?;
                }
                if let Some(prediction) = prediction {
                    pending.push_back(Pending {
                        ordinal,
                        mark: current.vwap,
                        prediction,
                        partition,
                    });
                }
            }
            NativeTickTargetV1::FixedEventTime { horizon_ns } => {
                if let Some(prediction) = prediction {
                    let threshold = current
                        .event
                        .checked_add(horizon_ns.get())
                        .ok_or_else(|| anyhow::anyhow!("TICK_LABEL_TIME_RANGE"))?;
                    let reader = label_rows
                        .as_deref_mut()
                        .ok_or_else(|| anyhow::anyhow!("TICK_LABEL_READER_REQUIRED"))?;
                    while future.as_ref().is_none_or(|row| row.event < threshold) {
                        future = reader.next().transpose()?;
                        if future.is_none() {
                            break;
                        }
                    }
                    let label = future.as_ref().map(|row| row.vwap / current.vwap - 1.0);
                    ensure!(label.is_none_or(f64::is_finite), "TICK_LABEL_NONFINITE");
                    let (_, end) = partition_bounds(request, PARTITIONS[partition]);
                    let reason = match &future {
                        None => Some(NativeFactorExclusionReasonV1::LabelNotComplete),
                        Some(row) if row.event >= end.get() => {
                            Some(NativeFactorExclusionReasonV1::LabelCrossesPartition)
                        }
                        _ => None,
                    };
                    record(
                        &mut summary.metrics,
                        pooled,
                        partition,
                        Some(prediction),
                        label,
                        reason,
                    )?;
                }
            }
        }
        previous = Some(current);
        seen += 1;
    }
    ensure!(seen == cohort_count, "TICK_CACHE_COHORT_COUNT_MISMATCH");
    for point in pending {
        record(
            &mut summary.metrics,
            pooled,
            point.partition,
            Some(point.prediction),
            None,
            Some(NativeFactorExclusionReasonV1::LabelNotComplete),
        )?;
    }
    budget.fuel = model.remaining_fuel();
    Ok(summary)
}

pub(super) fn evaluate(
    root: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
    wasm: &[u8],
    summary_only: bool,
) -> Result<NativeTradeTickFactorEvaluationResultV1> {
    ensure!(summary_only, "TICK_CACHE_SUMMARY_ONLY_REQUIRED");
    let mut cache = super::cohort_cache::load(root, request)?;
    cache
        .assets
        .sort_by(|left, right| left.instrument_id.cmp(&right.instrument_id));
    let module = SignalModule::new(wasm, request.parameters.total_fuel.is_some())?;
    let mut budget = Budget {
        fuel: request.parameters.total_fuel.map(|value| value.get()),
        source_rows: request
            .input
            .source_row_limit()
            .ok_or_else(|| anyhow::anyhow!("TICK_RESOURCE_LIMIT"))?,
    };
    let mut pooled: [Metric; 3] = std::array::from_fn(|_| Metric::default());
    let mut assets = BTreeMap::new();
    let mut predictions = 0_u64;
    for asset in &cache.assets {
        let mut label_rows = match request.target {
            NativeTickTargetV1::FixedEventTime { .. } => Some(cache.cohorts(asset)?),
            NativeTickTargetV1::FixedTradeCohorts { .. } => None,
        };
        let summary = evaluate_asset(
            request,
            (
                cache.cohorts(asset)?,
                label_rows
                    .as_mut()
                    .map(|reader| reader as &mut dyn Iterator<Item = Result<Cohort>>),
            ),
            asset.cohort_count,
            &module,
            &mut budget,
            &mut pooled,
        )?;
        let expected = cache
            .coverage
            .instrument_coverage
            .iter()
            .find(|row| row.instrument_id == asset.instrument_id)
            .ok_or_else(|| anyhow::anyhow!("TICK_INSTRUMENT_SELECTION_MISSING"))?;
        ensure!(
            summary.trades == expected.trade_count.get(),
            "TICK_CACHE_TRADE_COUNT_MISMATCH"
        );
        predictions = predictions
            .checked_add(summary.predictions)
            .ok_or_else(|| anyhow::anyhow!("TICK_COUNT_RANGE"))?;
        ensure!(
            assets
                .insert(asset.instrument_id.clone(), summary.metrics)
                .is_none(),
            "TICK_INSTRUMENT_SELECTION_DUPLICATE"
        );
    }
    ensure!(predictions > 0, "TICK_NO_PREDICTIONS");
    let partitions = PARTITIONS
        .into_iter()
        .enumerate()
        .map(|(index, partition)| -> Result<_> {
            let (start_ns, end_ns) = partition_bounds(request, partition);
            let assets = request
                .input
                .instrument_ids
                .iter()
                .map(|id| -> Result<_> {
                    let metrics = assets
                        .get(id)
                        .ok_or_else(|| anyhow::anyhow!("TICK_INSTRUMENT_SELECTION_MISSING"))?;
                    Ok(NativeFactorAssetMetricV1 {
                        instrument_id: id.clone(),
                        metric: metrics[index].finish()?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(NativeFactorPartitionReportV1 {
                partition,
                start_ns,
                end_ns,
                metric: pooled[index].finish()?,
                assets,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NativeTradeTickFactorEvaluationResultV1 {
        schema_version: SchemaV1, request: request.clone(),
        native_versions: BTreeMap::from([("nautilus-model".into(), "0.63.0".into()), ("nautilus-persistence".into(), "0.63.0".into()), ("nautilus-indicators".into(), "0.63.0".into()), ("wasmi".into(), "2.0.0".into())]),
        methods: BTreeMap::from([
            ("input".into(), "EVENT_TIME_COHORT_CACHE_V1: derived complete-event cohorts; no BAR catalog or simulation".into()),
            ("prediction".into(), crate::feature_model::ABI.into()), ("features".into(), FEATURE_SCHEMA.into()),
            ("sampling".into(), "EVENT_TIME_COHORT_V1: complete current cohort visible; earlier selected cohorts only; unknown source sequence".into()),
            ("label".into(), request.target.label_method().into()),
            ("partitioning".into(), "shared half-open event cuts; cross-boundary labels excluded; warmup and state continue across source units".into()),
            ("qualification".into(), "UNQUALIFIED_EXPLORATORY; event time proxy is not original availability evidence".into()),
            ("ic".into(), "stable online covariance raw pooled Pearson correlation; not averaged asset/unit IC".into()),
        ]),
        consumed_fuel: request.parameters.total_fuel.zip(budget.fuel).map(|(total, left)| counter(total.get() - left)).transpose()?,
        coverage: cache.coverage, partitions, points: None,
    })
}

#[cfg(test)]
mod tests {
    use super::super::tests::{model, request, series};
    use super::*;

    fn same_metric(actual: &NativeFactorMetricV1, expected: &NativeFactorMetricV1) {
        assert_eq!(
            serde_json::to_value(&actual.counts).unwrap(),
            serde_json::to_value(&expected.counts).unwrap()
        );
        assert_eq!(actual.pearson_ic.status, expected.pearson_ic.status);
        match (actual.pearson_ic.value, expected.pearson_ic.value) {
            (Some(actual), Some(expected)) => {
                assert!((actual - expected).abs() <= 1e-12, "{actual} != {expected}")
            }
            (None, None) => {}
            _ => panic!("metric availability mismatch"),
        }
    }

    #[test]
    fn online_pearson_matches_original_calculation_and_unavailable_statuses() {
        for (scores, labels) in [
            (vec![], vec![]),
            (vec![1.0], vec![2.0]),
            (vec![1.0, 1.0], vec![2.0, 3.0]),
            (vec![1.0, 2.0], vec![3.0, 3.0]),
            (vec![1.0, f64::NAN], vec![2.0, 3.0]),
            (vec![1.0, 5.0, -7.0, 2.0], vec![9.0, -2.0, 4.0, 13.0]),
        ] {
            let expected = crate::factors::pearson_ic(&scores, &labels).unwrap();
            let mut online = Pearson::default();
            for (&score, &label) in scores.iter().zip(&labels) {
                online.push(score, label).unwrap();
            }
            let actual = online.finish();
            assert_eq!(actual.status, expected.status);
            if let (Some(actual), Some(expected)) = (actual.value, expected.value) {
                assert!((actual - expected).abs() <= 1e-12);
            }
        }
        let scores = (0..1000)
            .map(|index| (index as f64 * 0.17).sin() * 1e3 + 1e5)
            .collect::<Vec<_>>();
        let labels = (0..1000)
            .map(|index| (index as f64 * 0.13).cos())
            .collect::<Vec<_>>();
        let expected = crate::factors::pearson_ic(&scores, &labels).unwrap();
        let mut online = Pearson::default();
        for (score, label) in scores.into_iter().zip(labels) {
            online.push(score, label).unwrap();
        }
        assert!((online.finish().value.unwrap() - expected.value.unwrap()).abs() <= 1e-12);
    }

    fn compare(
        request: &NativeTradeTickFactorEvaluationRequestV1,
        data: BTreeMap<String, Vec<TradeTick>>,
        wasm: &[u8],
    ) {
        let expected = super::super::evaluate_series(request, data.clone(), wasm, true).unwrap();
        let module = SignalModule::new(wasm, request.parameters.total_fuel.is_some()).unwrap();
        let mut budget = Budget {
            fuel: request.parameters.total_fuel.map(|value| value.get()),
            source_rows: expected.coverage.trade_count.get(),
        };
        let mut pooled = std::array::from_fn(|_| Metric::default());
        let mut by_asset = BTreeMap::new();
        for (id, rows) in data {
            let cohorts =
                super::super::cohorts(&rows, request.sampling.maximum_cohort_rows).unwrap();
            let mut label_rows = cohorts.clone().into_iter().map(Ok);
            let summary = evaluate_asset(
                request,
                (cohorts.clone().into_iter().map(Ok), Some(&mut label_rows)),
                cohorts.len() as u64,
                &module,
                &mut budget,
                &mut pooled,
            )
            .unwrap();
            assert_eq!(summary.trades, rows.len() as u64);
            by_asset.insert(id, summary.metrics);
        }
        assert_eq!(budget.source_rows, 0);
        for (index, expected) in expected.partitions.iter().enumerate() {
            same_metric(&pooled[index].finish().unwrap(), &expected.metric);
            for asset in &expected.assets {
                same_metric(
                    &by_asset[&asset.instrument_id][index].finish().unwrap(),
                    &asset.metric,
                );
            }
        }
        assert_eq!(
            request
                .parameters
                .total_fuel
                .zip(budget.fuel)
                .map(|(total, left)| total.get() - left),
            expected.consumed_fuel.map(|value| value.get())
        );
    }

    #[test]
    fn streaming_keeps_all_seven_features_labels_partitions_and_global_fuel() {
        let mut data = series();
        let mut extra = data["AUD/USD.SIM"][1];
        extra.trade_id = nautilus_model::identifiers::TradeId::from("extra");
        extra.price = nautilus_model::types::Price::from("19");
        extra.size = nautilus_model::types::Quantity::from("3");
        data.get_mut("AUD/USD.SIM").unwrap().insert(2, extra);
        let last = data.get_mut("AUD/USD.SIM").unwrap().last_mut().unwrap();
        last.ts_event = 75_u64.into();
        last.ts_init = 75_u64.into();
        for horizon in [1, 2, 3] {
            let mut request = request();
            request.target = NativeTickTargetV1::FixedTradeCohorts {
                horizon_cohorts: horizon,
            };
            for slot in 0..7 {
                compare(&request, data.clone(), &model(slot));
            }
        }
        for horizon in [1, 9, 10, 20, 60] {
            let mut request = request();
            request.target = NativeTickTargetV1::FixedEventTime {
                horizon_ns: counter(horizon).unwrap(),
            };
            for slot in 0..7 {
                compare(&request, data.clone(), &model(slot));
            }
        }
        let mut request = request();
        request.parameters.total_fuel = Some(counter(1_000_000).unwrap());
        request.input.instrument_ids = vec![
            "ZZZ/USD.SIM".into(),
            "EMPTY.SIM".into(),
            "AUD/USD.SIM".into(),
        ];
        let mut data = series();
        let mut other = data["AUD/USD.SIM"].clone();
        for trade in &mut other {
            trade.instrument_id = nautilus_model::identifiers::InstrumentId::from("ZZZ/USD.SIM");
        }
        data.insert("ZZZ/USD.SIM".into(), other);
        data.insert("EMPTY.SIM".into(), vec![]);
        let stateful = wat::parse_str(
            r#"(module
            (global $calls (mut i32) (i32.const 0))
            (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64))
            (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
                global.get $calls i32.const 1 i32.add global.set $calls
                global.get $calls f64.convert_i32_s))"#,
        )
        .unwrap();
        compare(&request, data, &stateful);
    }

    #[test]
    fn streaming_enforces_actual_source_budget_count_and_event_order() {
        let request = request();
        let wasm = model(0);
        let module = SignalModule::new(&wasm, false).unwrap();
        let original = super::super::cohorts(&series()["AUD/USD.SIM"], 100).unwrap();
        for (mut rows, expected_count, source_rows, error) in [
            (original.clone(), 7, 6, "TICK_SOURCE_ROW_LIMIT"),
            (original.clone(), 8, 100, "TICK_CACHE_COHORT_COUNT_MISMATCH"),
            (original.clone(), 6, 100, "TICK_CACHE_COHORT_COUNT_MISMATCH"),
            (original.clone(), 7, 100, "TICK_CACHE_EVENT_ORDER"),
        ] {
            if error == "TICK_CACHE_EVENT_ORDER" {
                rows.swap(1, 2);
            }
            let mut budget = Budget {
                fuel: None,
                source_rows,
            };
            let result = evaluate_asset(
                &request,
                (rows.into_iter().map(Ok), None),
                expected_count,
                &module,
                &mut budget,
                &mut std::array::from_fn(|_| Metric::default()),
            );
            assert_eq!(result.err().unwrap().to_string(), error);
        }
    }
    #[test]
    fn duration_label_reader_failures_are_not_silently_censored() {
        let mut request = request();
        request.target = NativeTickTargetV1::FixedEventTime {
            horizon_ns: counter(10).unwrap(),
        };
        let wasm = model(0);
        let module = SignalModule::new(&wasm, false).unwrap();
        let rows = super::super::cohorts(&series()["AUD/USD.SIM"], 100).unwrap();
        let mut labels = std::iter::once(Err(anyhow::anyhow!("label read failed")));
        let mut budget = Budget {
            fuel: None,
            source_rows: 7,
        };
        let result = evaluate_asset(
            &request,
            (rows.into_iter().map(Ok), Some(&mut labels)),
            7,
            &module,
            &mut budget,
            &mut std::array::from_fn(|_| Metric::default()),
        );
        assert_eq!(result.err().unwrap().to_string(), "label read failed");
        assert_eq!(budget.source_rows, 5);
    }
}
