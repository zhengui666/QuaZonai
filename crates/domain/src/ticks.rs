//! Pure selection, resource and temporal rules for explicit tick factor research.
use crate::DomainError;
use contracts::{
    factors::NativeFactorPartitionV1,
    ticks::{NativeTickRegimeV1, NativeTickTargetV1, NativeTradeTickFactorEvaluationRequestV1},
    DbCounter,
};
use std::collections::BTreeSet;

pub const MAXIMUM_TICK_ROWS: u32 = 1_000_000;
pub const MAXIMUM_COHORT_ROWS: u32 = 100_000;
pub const UTC_DAY_NS: u64 = 86_400_000_000_000;

pub fn evaluation_request(
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<(), DomainError> {
    let input = &request.input;
    let fail = |code| Err(DomainError::Invalid(code));
    if input.instrument_ids.is_empty()
        || input.instrument_ids.len() > MAXIMUM_TICK_ROWS as usize
        || input.instrument_ids.iter().any(|id| {
            id.is_empty()
                || id.len() > 200
                || id.chars().any(char::is_whitespace)
                || id.chars().any(char::is_control)
        })
        || input.instrument_ids.iter().collect::<BTreeSet<_>>().len() != input.instrument_ids.len()
    {
        return fail("TICK_INSTRUMENT_SELECTION_INVALID");
    }
    if input.event_start_ns >= input.event_end_ns
        || input.decision_cutoff_ns < input.event_end_ns
        || request.train_end_ns <= input.event_start_ns
        || request.train_end_ns >= request.validation_end_ns
        || request.validation_end_ns > input.event_end_ns
    {
        return fail("TICK_PARTITION_BOUNDARIES");
    }
    let budget_valid = if input.is_cohort_cache() {
        input.maximum_rows.is_none()
            && input
                .maximum_source_rows
                .is_some_and(|value| value.get() > 0)
    } else {
        input.maximum_source_rows.is_none()
            && input
                .maximum_rows
                .is_some_and(|value| value > 0 && value <= MAXIMUM_TICK_ROWS)
    };
    if !budget_valid
        || request.sampling.maximum_cohort_rows == 0
        || request.sampling.maximum_cohort_rows > MAXIMUM_COHORT_ROWS
    {
        return fail("TICK_RESOURCE_LIMIT");
    }
    if input.source_revision.is_empty()
        || input.source_revision.len() > 512
        || input.source_revision.chars().any(char::is_control)
    {
        return fail("TICK_SOURCE_REVISION_REQUIRED");
    }
    let p = &request.parameters;
    if p.fast_period_cohorts == 0
        || p.slow_period_cohorts <= p.fast_period_cohorts
        || p.slow_period_cohorts > MAXIMUM_TICK_ROWS
        || p.total_fuel.is_some_and(|fuel| fuel.get() == 0)
    {
        return fail("TICK_PARAMETERS_INVALID");
    }
    let valid_target = match request.target {
        NativeTickTargetV1::FixedTradeCohorts { horizon_cohorts } => {
            horizon_cohorts > 0 && horizon_cohorts <= MAXIMUM_TICK_ROWS
        }
        NativeTickTargetV1::FixedEventTime { horizon_ns } => {
            horizon_ns.get() > 0
                && input
                    .event_end_ns
                    .get()
                    .checked_sub(1)
                    .and_then(|last| last.checked_add(horizon_ns.get()))
                    .is_some_and(|end| DbCounter::new(end).is_ok())
        }
    };
    if !valid_target {
        return fail("TICK_PARAMETERS_INVALID");
    }
    evaluation_regimes(request)?;
    Ok(())
}

/// Evaluation-only states use completed BTC endpoint days; no future marks enter features.
pub fn evaluation_regimes(
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<Vec<NativeTickRegimeV1>, DomainError> {
    use bigdecimal::BigDecimal;
    use std::str::FromStr;
    let Some(evaluation) = &request.evaluation_regimes else {
        return Ok(Vec::new());
    };
    let invalid = || DomainError::Invalid("TICK_REGIME_EVALUATION_INVALID");
    let start = request.input.event_start_ns.get();
    let end = request.input.event_end_ns.get();
    if !request.input.is_cohort_cache()
        || start >= end
        || [
            start,
            end,
            request.train_end_ns.get(),
            request.validation_end_ns.get(),
        ]
        .iter()
        .any(|value| !value.is_multiple_of(UTC_DAY_NS))
        || u64::try_from(evaluation.days.len()).map_err(|_| invalid())?
            != (end - start) / UTC_DAY_NS
    {
        return Err(invalid());
    }
    evaluation
        .days
        .iter()
        .enumerate()
        .map(|(index, day)| {
            let expected = start
                .checked_add(
                    u64::try_from(index)
                        .map_err(|_| invalid())?
                        .checked_mul(UTC_DAY_NS)
                        .ok_or_else(invalid)?,
                )
                .ok_or_else(invalid)?;
            if day.day_start_ns.get() != expected {
                return Err(invalid());
            }
            let mark = |mark: &Option<contracts::ticks::NativeTickRegimeMarkV1>,
                        offset: u64|
             -> Result<Option<BigDecimal>, DomainError> {
                let Some(mark) = mark else { return Ok(None) };
                let lower = expected
                    .checked_sub(offset * UTC_DAY_NS)
                    .ok_or_else(invalid)?;
                if mark.event_ns.get() < lower
                    || mark.event_ns.get() >= lower + UTC_DAY_NS
                    || mark.price.is_empty()
                    || mark.price.len() > 512
                    || mark
                        .price
                        .bytes()
                        .any(|byte| !byte.is_ascii_digit() && !matches!(byte, b'.' | b'+' | b'-'))
                {
                    return Err(invalid());
                }
                let value = BigDecimal::from_str(&mark.price).map_err(|_| invalid())?;
                if value <= BigDecimal::from(0) {
                    return Err(invalid());
                }
                Ok(Some(value))
            };
            let previous = mark(&day.previous, 1)?;
            let lookback = mark(&day.lookback, 31)?;
            Ok(match (previous, lookback) {
                (Some(previous), Some(lookback)) if &previous * 100 > &lookback * 105 => {
                    NativeTickRegimeV1::Bull
                }
                (Some(previous), Some(lookback)) if &previous * 100 < &lookback * 95 => {
                    NativeTickRegimeV1::Bear
                }
                (Some(_), Some(_)) => NativeTickRegimeV1::Range,
                _ => NativeTickRegimeV1::Unknown,
            })
        })
        .collect()
}

pub fn partition_bounds(
    request: &NativeTradeTickFactorEvaluationRequestV1,
    partition: NativeFactorPartitionV1,
) -> (DbCounter, DbCounter) {
    use NativeFactorPartitionV1::*;
    match partition {
        Train => (request.input.event_start_ns, request.train_end_ns),
        Validation => (request.train_end_ns, request.validation_end_ns),
        Heldout => (request.validation_end_ns, request.input.event_end_ns),
    }
}

pub fn partition_for_event(
    request: &NativeTradeTickFactorEvaluationRequestV1,
    event: DbCounter,
) -> Option<NativeFactorPartitionV1> {
    use NativeFactorPartitionV1::*;
    if event < request.input.event_start_ns || event >= request.input.event_end_ns {
        None
    } else if event < request.train_end_ns {
        Some(Train)
    } else if event < request.validation_end_ns {
        Some(Validation)
    } else {
        Some(Heldout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::ticks::NativeTickRepresentationV1;
    use serde_json::json;

    #[test]
    fn cache_representation_uses_a_separate_exact_source_budget() {
        let native: NativeTradeTickFactorEvaluationRequestV1 = serde_json::from_value(json!({
            "schema_version":1,"input":{"modality":"TRADE_TICK","instrument_ids":["AUD/USD.SIM"],
                "event_start_ns":"0","event_end_ns":"80","decision_cutoff_ns":"80","maximum_rows":100,
                "source_revision":"fixed-source","availability":"EVENT_TIME_PROXY_UNVERIFIED"},
            "sampling":{"kind":"EVENT_TIME_COHORT_V1","maximum_cohort_rows":100},
            "target":{"kind":"FIXED_TRADE_COHORTS","horizon_cohorts":1},
            "parameters":{"fast_period_cohorts":1,"slow_period_cohorts":2},
            "train_end_ns":"30","validation_end_ns":"50"
        })).unwrap();
        let legacy = serde_json::to_value(&native).unwrap();
        assert_eq!(legacy["input"]["maximum_rows"], 100);
        assert!(!legacy["input"]
            .as_object()
            .unwrap()
            .contains_key("representation"));
        assert!(!legacy["input"]
            .as_object()
            .unwrap()
            .contains_key("maximum_source_rows"));
        assert!(evaluation_request(&native).is_ok());
        let mut cache = native.clone();
        cache.input.representation = Some(NativeTickRepresentationV1::EventTimeCohortCacheV1);
        cache.input.maximum_rows = None;
        cache.input.maximum_source_rows = Some(DbCounter::new(4_500_000_000).unwrap());
        assert!(evaluation_request(&cache).is_ok());
        assert_eq!(cache.input.source_row_limit(), Some(4_500_000_000));
        let encoded = serde_json::to_value(&cache).unwrap();
        assert_eq!(encoded["input"]["maximum_source_rows"], "4500000000");
        assert!(!encoded["input"]
            .as_object()
            .unwrap()
            .contains_key("maximum_rows"));
        let mut flow = cache.clone();
        flow.input.representation = Some(NativeTickRepresentationV1::EventTimeCohortFlowCacheV1);
        assert!(evaluation_request(&flow).is_ok());
        assert!(flow.input.is_cohort_cache() && flow.input.is_flow_cache());
        assert_eq!(
            flow.input.cache_representation(),
            Some("EVENT_TIME_COHORT_FLOW_CACHE_V1")
        );
        assert_eq!(
            serde_json::to_value(&flow).unwrap()["input"]["representation"],
            "EVENT_TIME_COHORT_FLOW_CACHE_V1"
        );
        assert_eq!(flow.input.source_row_limit(), Some(4_500_000_000));
        cache.input.maximum_rows = Some(100);
        assert!(evaluation_request(&cache).is_err());
        cache.input.maximum_rows = None;
        cache.input.maximum_source_rows = None;
        assert!(evaluation_request(&cache).is_err());
        let mut invalid_native = native;
        invalid_native.input.maximum_source_rows = Some(DbCounter::new(1).unwrap());
        assert!(evaluation_request(&invalid_native).is_err());
        invalid_native.input.maximum_source_rows = None;
        invalid_native.input.maximum_rows = Some(MAXIMUM_TICK_ROWS + 1);
        assert!(evaluation_request(&invalid_native).is_err());
    }
    #[test]
    fn target_duration_is_exact_positive_and_cannot_mix_old_fields() {
        let old = serde_json::json!({"kind":"FIXED_TRADE_COHORTS","horizon_cohorts":1});
        let decoded: NativeTickTargetV1 = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), old);
        for invalid in [
            serde_json::json!({"kind":"FIXED_EVENT_TIME","horizon_ns":10}),
            serde_json::json!({"kind":"FIXED_EVENT_TIME","horizon_ns":"9223372036854775808"}),
            serde_json::json!({"kind":"FIXED_EVENT_TIME","horizon_ns":"10","horizon_cohorts":1}),
            serde_json::json!({"kind":"FIXED_TRADE_COHORTS","horizon_cohorts":1,"horizon_ns":"10"}),
        ] {
            assert!(serde_json::from_value::<NativeTickTargetV1>(invalid).is_err());
        }
        let mut req: NativeTradeTickFactorEvaluationRequestV1 = serde_json::from_value(serde_json::json!({
            "schema_version":1,"input":{"modality":"TRADE_TICK","instrument_ids":["A.SIM"],
                "event_start_ns":"0","event_end_ns":"80","decision_cutoff_ns":"80","maximum_rows":100,
                "source_revision":"source","availability":"EVENT_TIME_PROXY_UNVERIFIED"},
            "sampling":{"kind":"EVENT_TIME_COHORT_V1","maximum_cohort_rows":100},
            "target":{"kind":"FIXED_EVENT_TIME","horizon_ns":"9007199254740993"},
            "parameters":{"fast_period_cohorts":1,"slow_period_cohorts":2},
            "train_end_ns":"30","validation_end_ns":"50"
        })).unwrap();
        assert!(evaluation_request(&req).is_ok());
        for horizon in [0, i64::MAX as u64] {
            req.target = NativeTickTargetV1::FixedEventTime {
                horizon_ns: DbCounter::new(horizon).unwrap(),
            };
            assert!(evaluation_request(&req).is_err());
        }
        req.target = NativeTickTargetV1::FixedEventTime {
            horizon_ns: DbCounter::new(i64::MAX as u64 - 79).unwrap(),
        };
        assert!(evaluation_request(&req).is_ok());
    }
}

#[cfg(test)]
mod regime_tests {
    use super::*;
    use contracts::ticks::*;
    use serde_json::json;

    #[test]
    fn daily_states_use_exact_completed_endpoint_days_and_preserve_unknown() {
        let mut request: NativeTradeTickFactorEvaluationRequestV1 = serde_json::from_value(json!({
            "schema_version":1,"input":{"modality":"TRADE_TICK","representation":"EVENT_TIME_COHORT_FLOW_CACHE_V1",
                "instrument_ids":["BTC-USD-PERP.HYPERLIQUID"],"event_start_ns":(32*UTC_DAY_NS).to_string(),
                "event_end_ns":(36*UTC_DAY_NS).to_string(),"decision_cutoff_ns":(36*UTC_DAY_NS).to_string(),
                "maximum_source_rows":"100","source_revision":"source","availability":"EVENT_TIME_PROXY_UNVERIFIED"},
            "sampling":{"kind":"EVENT_TIME_COHORT_V1","maximum_cohort_rows":100},
            "target":{"kind":"FIXED_EVENT_TIME","horizon_ns":"60000000000"},
            "parameters":{"fast_period_cohorts":8,"slow_period_cohorts":32},
            "train_end_ns":(33*UTC_DAY_NS).to_string(),"validation_end_ns":(35*UTC_DAY_NS).to_string(),
            "evaluation_regimes":{"kind":"BTC_COMPLETED_30_DAY_RETURN_V1","days":[]}
        })).unwrap();
        request.evaluation_regimes.as_mut().unwrap().days = [
            Some("105.0000000000000000000000000001"),
            Some("94.9999999999999999999999999999"),
            Some("105"),
            None,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, price)| {
            let day = (32 + index as u64) * UTC_DAY_NS;
            NativeTickRegimeDayV1 {
                day_start_ns: DbCounter::new(day).unwrap(),
                previous: price.map(|price| NativeTickRegimeMarkV1 {
                    event_ns: DbCounter::new(day - UTC_DAY_NS + 1).unwrap(),
                    price: price.into(),
                }),
                lookback: Some(NativeTickRegimeMarkV1 {
                    event_ns: DbCounter::new(day - 31 * UTC_DAY_NS + 1).unwrap(),
                    price: "100".into(),
                }),
            }
        })
        .collect();
        assert!(evaluation_request(&request).is_ok());
        assert_eq!(
            evaluation_regimes(&request).unwrap(),
            [
                NativeTickRegimeV1::Bull,
                NativeTickRegimeV1::Bear,
                NativeTickRegimeV1::Range,
                NativeTickRegimeV1::Unknown
            ]
        );
        assert_eq!(
            serde_json::to_value(&request).unwrap()["evaluation_regimes"]["kind"],
            "BTC_COMPLETED_30_DAY_RETURN_V1"
        );
        let mut invalid = request.clone();
        invalid.evaluation_regimes.as_mut().unwrap().days[0]
            .previous
            .as_mut()
            .unwrap()
            .event_ns = request.input.event_start_ns;
        assert!(evaluation_request(&invalid).is_err());
        let mut invalid = request.clone();
        invalid.evaluation_regimes.as_mut().unwrap().days.swap(0, 1);
        assert!(evaluation_request(&invalid).is_err());
        let mut invalid = request.clone();
        invalid.evaluation_regimes.as_mut().unwrap().days[0]
            .previous
            .as_mut()
            .unwrap()
            .price = "1e2".into();
        assert!(evaluation_request(&invalid).is_err());
        request.clip_end(request.validation_end_ns);
        assert_eq!(request.evaluation_regimes.as_ref().unwrap().days.len(), 3);
        assert!(evaluation_request(&request).is_ok());
    }
}
