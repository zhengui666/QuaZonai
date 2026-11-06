//! Controlled software points matching perf-qa/make_fixtures.py, not market evidence.
use contracts::{
    factors::{NativeFactorExclusionReasonV1, NativeFactorPartitionV1, NativeFactorPointV1},
    review_science::NativeReviewFactorScopeV1,
    science::{ForecastMissingReason, NativeForecastPointV1},
    DbCounter,
};

pub struct ReviewCase {
    pub name: String,
    pub points: Vec<NativeFactorPointV1>,
    pub scope: NativeReviewFactorScopeV1,
}

fn fixture(groups: usize, assets: usize, mode: &str) -> Vec<NativeFactorPointV1> {
    let mut points = Vec::new();
    for group in 0..groups {
        let count = if mode == "irregular" {
            3 + (group * 7) % (assets.saturating_sub(2)).max(1)
        } else {
            assets
        };
        for asset in 0..count {
            let mut score = ((group * 7 + asset * 11) % 23) as f64 / 8.0 - 11.0 / 8.0;
            let label = ((group * 13 + asset * 5) % 29) as f64 / 1024.0 - 14.0 / 1024.0;
            if mode == "constant" {
                score = 1.0;
            }
            if mode == "ties_signed_zero" {
                score = [-0.0, 0.0, 1.0, 1.0, -2.0][(group + asset) % 5];
            }
            let event = ((group + 1) * 10) as u64;
            let overlap = if mode == "overlap" { 3 } else { 1 };
            let mut point = NativeFactorPointV1 {
                observation: NativeForecastPointV1 {
                    instrument_id: format!("ASSET{asset}.SIM"),
                    ordinal: group.try_into().unwrap(),
                    event_ns: DbCounter::new(event).unwrap(),
                    available_ns: DbCounter::new(event).unwrap(),
                    forecast: Some(score),
                    forecast_reason: None,
                    label_return: Some(label),
                    label_available_ns: Some(DbCounter::new(event + overlap * 10).unwrap()),
                    label_reason: None,
                },
                partition: NativeFactorPartitionV1::Validation,
                label_event_ns: Some(DbCounter::new(event + overlap * 10).unwrap()),
                exclusion_reason: None,
            };
            if mode == "excluded" && (group + asset) % 7 == 0 {
                point.observation.forecast = None;
                point.observation.forecast_reason = Some(ForecastMissingReason::IndicatorWarmup);
                point.exclusion_reason = Some(NativeFactorExclusionReasonV1::IndicatorWarmup);
            }
            if mode == "partitioned" && group % 3 == 0 {
                point.partition = NativeFactorPartitionV1::Heldout;
            }
            points.push(point);
        }
    }
    if mode == "duplicate" {
        points.insert(2, points[0].clone());
    }
    if mode == "missing" {
        points[0].observation.forecast = None;
    }
    points
}

pub fn cases() -> Vec<ReviewCase> {
    let mut cases = Vec::new();
    for (scope_name, scope) in [
        (
            "pooled_time_series",
            NativeReviewFactorScopeV1::PooledTimeSeries,
        ),
        ("cross_sectional", NativeReviewFactorScopeV1::CrossSectional),
    ] {
        for mode in [
            "regular",
            "irregular",
            "ties_signed_zero",
            "constant",
            "excluded",
            "partitioned",
            "overlap",
            "duplicate",
            "missing",
        ] {
            cases.push(ReviewCase {
                name: format!("review-{scope_name}-{mode}"),
                points: fixture(96, 12, mode),
                scope,
            });
        }
        for count in [0, 8, 59, 60] {
            cases.push(ReviewCase {
                name: format!("review-{scope_name}-groups-{count}"),
                points: fixture(count, 12, "regular"),
                scope,
            });
        }
    }
    cases
}
