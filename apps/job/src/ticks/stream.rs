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

const REGIMES: [NativeTickRegimeV1; 4] = [
    NativeTickRegimeV1::Bull,
    NativeTickRegimeV1::Bear,
    NativeTickRegimeV1::Range,
    NativeTickRegimeV1::Unknown,
];
const SPAN_CUTS: [u64; 6] = [
    0,
    60_000_000_000,
    61_000_000_000,
    120_000_000_000,
    300_000_000_000,
    3_600_000_000_000,
];
type GroupMetrics = [[Metric; 4]; 3];
fn group_metrics() -> GroupMetrics {
    std::array::from_fn(|_| std::array::from_fn(|_| Metric::default()))
}

#[derive(Default)]
struct Spans {
    count: u64,
    minimum: Option<u64>,
    maximum: Option<u64>,
    buckets: [u64; 6],
}
impl Spans {
    fn push(&mut self, span: u64) {
        self.count += 1;
        self.minimum = Some(self.minimum.map_or(span, |value| value.min(span)));
        self.maximum = Some(self.maximum.map_or(span, |value| value.max(span)));
        self.buckets[SPAN_CUTS.partition_point(|lower| *lower <= span) - 1] += 1;
    }
    fn finish(&self) -> Result<NativeTickLabelSpansV1> {
        Ok(NativeTickLabelSpansV1 {
            count: counter(self.count)?,
            min_ns: self.minimum.map(counter).transpose()?,
            max_ns: self.maximum.map(counter).transpose()?,
            buckets: SPAN_CUTS
                .iter()
                .enumerate()
                .map(|(index, lower)| {
                    Ok(NativeTickLabelSpanBucketV1 {
                        lower_ns: counter(*lower)?,
                        upper_ns: SPAN_CUTS.get(index + 1).copied().map(counter).transpose()?,
                        count: counter(self.buckets[index])?,
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
}

struct Groups {
    start: u64,
    states: Vec<NativeTickRegimeV1>,
    current: GroupMetrics,
    pooled: GroupMetrics,
    spans: [[Spans; 4]; 3],
    assets: BTreeMap<String, GroupMetrics>,
}
impl Groups {
    fn new(request: &NativeTradeTickFactorEvaluationRequestV1) -> Result<Self> {
        Ok(Self {
            start: request.input.event_start_ns.get(),
            states: domain::ticks::evaluation_regimes(request)?,
            current: group_metrics(),
            pooled: group_metrics(),
            spans: std::array::from_fn(|_| std::array::from_fn(|_| Spans::default())),
            assets: BTreeMap::new(),
        })
    }
    fn record(
        &mut self,
        event: u64,
        partition: usize,
        prediction: Option<f64>,
        label: Option<f64>,
        reason: Option<NativeFactorExclusionReasonV1>,
        span: Option<u64>,
    ) -> Result<()> {
        let day = usize::try_from((event - self.start) / domain::ticks::UTC_DAY_NS)?;
        let state = *self
            .states
            .get(day)
            .ok_or_else(|| anyhow::anyhow!("TICK_REGIME_EVENT_OUTSIDE_SELECTION"))?;
        let index = REGIMES
            .iter()
            .position(|candidate| *candidate == state)
            .unwrap();
        self.current[partition][index].push(prediction, label, reason)?;
        self.pooled[partition][index].push(prediction, label, reason)?;
        if reason.is_none() {
            self.spans[partition][index]
                .push(span.ok_or_else(|| anyhow::anyhow!("TICK_LABEL_SPAN_MISSING"))?);
        }
        Ok(())
    }
    fn finish_asset(&mut self, id: &str) {
        self.assets.insert(
            id.to_owned(),
            std::mem::replace(&mut self.current, group_metrics()),
        );
    }
    fn finish(
        self,
        request: &NativeTradeTickFactorEvaluationRequestV1,
    ) -> Result<Vec<NativeTickRegimePartitionV1>> {
        PARTITIONS
            .iter()
            .enumerate()
            .map(|(partition_index, partition)| {
                let (start_ns, end_ns) = partition_bounds(request, *partition);
                let regimes = REGIMES
                    .iter()
                    .enumerate()
                    .map(|(index, regime)| {
                        let assets = request
                            .input
                            .instrument_ids
                            .iter()
                            .map(|id| {
                                Ok(NativeFactorAssetMetricV1 {
                                    instrument_id: id.clone(),
                                    metric: self.assets[id][partition_index][index].finish()?,
                                })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        let days = self
                            .states
                            .iter()
                            .enumerate()
                            .filter(|(day, state)| {
                                let event = self.start + *day as u64 * domain::ticks::UTC_DAY_NS;
                                **state == *regime
                                    && event >= start_ns.get()
                                    && event < end_ns.get()
                            })
                            .count();
                        Ok(NativeTickRegimeMetricV1 {
                            regime: *regime,
                            days: counter(days as u64)?,
                            metric: self.pooled[partition_index][index].finish()?,
                            observed_assets: counter(
                                assets
                                    .iter()
                                    .filter(|asset| asset.metric.counts.observations.get() > 0)
                                    .count() as u64,
                            )?,
                            included_assets: counter(
                                assets
                                    .iter()
                                    .filter(|asset| asset.metric.counts.included.get() > 0)
                                    .count() as u64,
                            )?,
                            assets,
                            label_spans: self.spans[partition_index][index].finish()?,
                        })
                    })
                    .collect::<Result<_>>()?;
                Ok(NativeTickRegimePartitionV1 {
                    partition: *partition,
                    start_ns,
                    end_ns,
                    regimes,
                })
            })
            .collect()
    }
}

struct Pending {
    ordinal: u32,
    event: u64,
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
    groups: &mut Option<&mut Groups>,
    event: u64,
    span: Option<u64>,
) -> Result<()> {
    metrics[partition].push(prediction, label, reason)?;
    pooled[partition].push(prediction, label, reason)?;
    if let Some(groups) = groups {
        groups.record(event, partition, prediction, label, reason, span)?;
    }
    Ok(())
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
    mut groups: Option<&mut Groups>,
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
        let signed_quantity = if request.input.is_flow_cache() {
            let value = current
                .signed_quantity
                .ok_or_else(|| anyhow::anyhow!("TICK_FLOW_QUANTITY_MISSING"))?;
            ensure!(
                value.is_finite() && value.abs() <= current.quantity,
                "TICK_FLOW_QUANTITY_INVALID"
            );
            Some(value)
        } else {
            None
        };
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
            Some(if let Some(signed) = signed_quantity {
                let [a, b, c, d, e, f, g] = features;
                let flow_features = [
                    a,
                    b,
                    c,
                    d,
                    e,
                    f,
                    g,
                    feature(signed, current.event, ordinal)?,
                ];
                model.predict(current.event, current.event, ordinal, &flow_features)?
            } else {
                model.predict(current.event, current.event, ordinal, &features)?
            })
        } else {
            record(
                &mut summary.metrics,
                pooled,
                partition,
                None,
                Some(NativeFactorExclusionReasonV1::IndicatorWarmup),
                &mut groups,
                current.event,
                None,
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
                        &mut groups,
                        point.event,
                        Some(current.event - point.event),
                    )?;
                }
                if let Some(prediction) = prediction {
                    pending.push_back(Pending {
                        ordinal,
                        event: current.event,
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
                        &mut groups,
                        current.event,
                        future.as_ref().map(|row| row.event - current.event),
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
            &mut groups,
            point.event,
            None,
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
    let mut groups = request
        .evaluation_regimes
        .as_ref()
        .map(|_| Groups::new(request))
        .transpose()?;
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
            groups.as_mut(),
        )?;
        if let Some(groups) = &mut groups {
            groups.finish_asset(&asset.instrument_id);
        }
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
    let regime_partitions = groups.map(|groups| groups.finish(request)).transpose()?;
    let mut result = NativeTradeTickFactorEvaluationResultV1 {
        schema_version: SchemaV1, request: request.clone(),
        native_versions: BTreeMap::from([("nautilus-model".into(), "0.63.0".into()), ("nautilus-persistence".into(), "0.63.0".into()), ("nautilus-indicators".into(), "0.63.0".into()), ("wasmi".into(), "2.0.0".into())]),
        methods: BTreeMap::from([
            ("input".into(), format!("{}: derived complete-event cohorts; no BAR catalog or simulation", request.input.cache_representation().unwrap())),
            ("prediction".into(), crate::feature_model::ABI.into()), ("features".into(), if request.input.is_flow_cache() { FLOW_FEATURE_SCHEMA } else { FEATURE_SCHEMA }.into()),
            ("sampling".into(), "EVENT_TIME_COHORT_V1: complete current cohort visible; earlier selected cohorts only; unknown source sequence".into()),
            ("label".into(), request.target.label_method().into()),
            ("partitioning".into(), "shared half-open event cuts; cross-boundary labels excluded; warmup and state continue across source units".into()),
            ("qualification".into(), "UNQUALIFIED_EXPLORATORY; event time proxy is not original availability evidence".into()),
            ("ic".into(), "stable online covariance raw pooled Pearson correlation; not averaged asset/unit IC".into()),
        ]),
        consumed_fuel: request.parameters.total_fuel.zip(budget.fuel).map(|(total, left)| counter(total.get() - left)).transpose()?,
        coverage: cache.coverage, partitions, regime_partitions, points: None,
    };
    if result.regime_partitions.is_some() {
        result.methods.insert("regimes".into(), "BTC_COMPLETED_30_DAY_RETURN_V1: decision UTC day; exact completed D-1 / D-31 endpoints with strict +/-5% cuts; missing endpoint UNKNOWN; evaluation only; label state does not add exclusions. Label spans describe included pairs only.".into());
    }
    Ok(result)
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

    #[test]
    fn flow_stream_refuses_missing_or_invalid_flow_even_during_warmup() {
        let mut request = request();
        request.input.representation = Some(NativeTickRepresentationV1::EventTimeCohortFlowCacheV1);
        request.input.maximum_rows = None;
        request.input.maximum_source_rows = Some(counter(7).unwrap());
        let original = super::super::cohorts(&series()["AUD/USD.SIM"], 100).unwrap();
        let module = SignalModule::new(&model(7), false).unwrap();
        for (signed, expected) in [
            (None, "TICK_FLOW_QUANTITY_MISSING"),
            (Some(f64::NAN), "TICK_FLOW_QUANTITY_INVALID"),
            (Some(2.0), "TICK_FLOW_QUANTITY_INVALID"),
        ] {
            let mut rows = original.clone();
            rows[0].signed_quantity = signed;
            let mut budget = Budget {
                fuel: None,
                source_rows: 7,
            };
            let mut pooled = std::array::from_fn(|_| Metric::default());
            let failure = evaluate_asset(
                &request,
                (rows.into_iter().map(Ok), None),
                7,
                &module,
                &mut budget,
                &mut pooled,
                None,
            )
            .err()
            .unwrap();
            assert_eq!(failure.to_string(), expected);
        }
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
                None,
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
                None,
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
            None,
        );
        assert_eq!(result.err().unwrap().to_string(), "label read failed");
        assert_eq!(budget.source_rows, 5);
    }
    #[test]
    fn regime_metrics_match_explicit_pairs_without_changing_states_fuel_or_labels() {
        let day = domain::ticks::UTC_DAY_NS;
        let mut request = request();
        request.input.event_start_ns = counter(32 * day).unwrap();
        request.input.event_end_ns = counter(36 * day).unwrap();
        request.input.decision_cutoff_ns = request.input.event_end_ns;
        request.train_end_ns = counter(33 * day).unwrap();
        request.validation_end_ns = counter(35 * day).unwrap();
        request.input.representation = Some(NativeTickRepresentationV1::EventTimeCohortCacheV1);
        request.input.maximum_rows = None;
        request.input.maximum_source_rows = Some(counter(100).unwrap());
        request.parameters.total_fuel = Some(counter(1_000_000).unwrap());
        request.input.instrument_ids = vec![
            "AUD/USD.SIM".into(),
            "EMPTY.SIM".into(),
            "ZZZ/USD.SIM".into(),
        ];
        request.evaluation_regimes = Some(NativeTickRegimeEvaluationV1 {
            kind: NativeTickRegimeMethodV1::BtcCompleted30DayReturnV1,
            days: [Some("106"), Some("94"), Some("100"), None]
                .into_iter()
                .enumerate()
                .map(|(index, price)| {
                    let start = (32 + index as u64) * day;
                    NativeTickRegimeDayV1 {
                        day_start_ns: counter(start).unwrap(),
                        previous: price.map(|price| NativeTickRegimeMarkV1 {
                            event_ns: counter(start - day + 1).unwrap(),
                            price: price.into(),
                        }),
                        lookback: Some(NativeTickRegimeMarkV1 {
                            event_ns: counter(start - 31 * day + 1).unwrap(),
                            price: "100".into(),
                        }),
                    }
                })
                .collect(),
        });
        domain::ticks::evaluation_request(&request).unwrap();
        let times = [
            32 * day + 1,
            32 * day + 2,
            33 * day - 20_000_000_000,
            33 * day + 10_000_000_000,
            34 * day - 20_000_000_000,
            34 * day + 60_000_000_000,
            35 * day - 20_000_000_000,
            35 * day + 60_000_000_000,
            36 * day - 1,
        ];
        let mut data = BTreeMap::new();
        for (id, scale) in [("AUD/USD.SIM", 1), ("ZZZ/USD.SIM", 3)] {
            let rows = times
                .iter()
                .enumerate()
                .map(|(index, event)| {
                    let mut row = series()["AUD/USD.SIM"][0];
                    row.instrument_id = nautilus_model::identifiers::InstrumentId::from(id);
                    row.trade_id =
                        nautilus_model::identifiers::TradeId::from(index.to_string().as_str());
                    row.ts_event = (*event).into();
                    row.ts_init = (*event).into();
                    row.price = nautilus_model::types::Price::from(
                        ((10 + index) * scale).to_string().as_str(),
                    );
                    row
                })
                .collect::<Vec<_>>();
            data.insert(id.to_owned(), rows);
        }
        data.insert("EMPTY.SIM".into(), vec![]);
        for target in [
            NativeTickTargetV1::FixedTradeCohorts { horizon_cohorts: 1 },
            NativeTickTargetV1::FixedEventTime {
                horizon_ns: counter(60_000_000_000).unwrap(),
            },
        ] {
            request.target = target;
            let wasm = model(0);
            let expected =
                super::super::evaluate_series(&request, data.clone(), &wasm, false).unwrap();
            let module = SignalModule::new(&wasm, true).unwrap();
            let mut budget = Budget {
                fuel: Some(1_000_000),
                source_rows: 100,
            };
            let mut pooled = std::array::from_fn(|_| Metric::default());
            let mut groups = Groups::new(&request).unwrap();
            for (id, trades) in &data {
                let rows = super::super::cohorts(trades, 100).unwrap();
                let mut labels = rows.clone().into_iter().map(Ok);
                evaluate_asset(
                    &request,
                    (rows.clone().into_iter().map(Ok), Some(&mut labels)),
                    rows.len() as u64,
                    &module,
                    &mut budget,
                    &mut pooled,
                    Some(&mut groups),
                )
                .unwrap();
                groups.finish_asset(id);
            }
            assert_eq!(
                expected.consumed_fuel.unwrap().get(),
                1_000_000 - budget.fuel.unwrap()
            );
            let reports = groups.finish(&request).unwrap();
            let states = domain::ticks::evaluation_regimes(&request).unwrap();
            for (index, report) in reports.iter().enumerate() {
                same_metric(
                    &pooled[index].finish().unwrap(),
                    &expected.partitions[index].metric,
                );
                assert_eq!(
                    report
                        .regimes
                        .iter()
                        .map(|group| group.metric.counts.observations.get())
                        .sum::<u64>(),
                    expected.partitions[index].metric.counts.observations.get()
                );
                for group in &report.regimes {
                    let metric = crate::factors::metric(
                        expected.points.as_ref().unwrap().iter().filter(|point| {
                            point.partition == report.partition
                                && states
                                    [((point.observation.event_ns.get() - 32 * day) / day) as usize]
                                    == group.regime
                        }),
                    )
                    .unwrap();
                    same_metric(&group.metric, &metric);
                    assert_eq!(group.assets.len(), 3);
                    assert_eq!(group.assets[1].metric.counts.observations.get(), 0);
                    assert_eq!(group.label_spans.count, group.metric.counts.included);
                    assert_eq!(
                        group
                            .label_spans
                            .buckets
                            .iter()
                            .map(|bucket| bucket.count.get())
                            .sum::<u64>(),
                        group.label_spans.count.get()
                    );
                    if matches!(target, NativeTickTargetV1::FixedEventTime { .. }) {
                        assert!(group
                            .label_spans
                            .min_ns
                            .is_none_or(|span| span.get() >= 60_000_000_000));
                    }
                }
            }
            // A BEAR decision at day 34 minus 20 seconds labels a RANGE observation,
            // without introducing a regime-boundary exclusion inside VALIDATION.
            assert!(reports[1].regimes[1].metric.counts.included.get() > 0);
        }
    }
}
