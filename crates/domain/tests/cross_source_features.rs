use contracts::{research::DataPartition, science::*, DbCounter, SchemaV1};
use domain::execution::features::*;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn definition() -> FeatureDefinitionV1 {
    FeatureDefinitionV1 {
        feature_key: "inventory".into(),
        source_ref: "frozen:warehouse-sensor/revision-7".into(),
        source_key: "aisle-3".into(),
        availability: FeatureAvailabilityV1::Observed,
        max_age_ns: Some(count(5)),
    }
}
fn row(event: u64, available: u64, sequence: u64, value: Option<f64>) -> FeatureObservationV1 {
    FeatureObservationV1 {
        feature_index: 0,
        event_ns: count(event),
        observed_available_ns: Some(count(available)),
        sequence: count(sequence),
        value,
        missing_reason: value.is_none().then(|| "SOURCE_GAP".into()),
    }
}
fn part(rows: Vec<FeatureObservationV1>) -> FeatureObservationsV1 {
    FeatureObservationsV1 {
        schema_version: SchemaV1,
        partition: DataPartition::Validation,
        feature_schema: vec![definition()],
        observations: rows,
    }
}

#[test]
fn arrivals_are_causal_and_zero_missing_expired_are_distinct() {
    let mut state = FeatureState::new(&[definition()]).unwrap();
    let empty = state.values(count(9)).unwrap();
    assert_eq!(
        empty[0].missing_reason,
        Some(FeatureMissingReasonV1::NotYetAvailable)
    );
    assert_eq!(empty[0].event_ns, None);
    state.observe(&row(10, 12, 1, Some(0.0))).unwrap();
    assert!(
        state.values(count(11)).is_err(),
        "an arrived future value must never be exposed"
    );
    let zero = state.values(count(12)).unwrap();
    assert_eq!(zero[0].value, Some(0.0));
    assert_eq!(zero[0].missing_reason, None);
    assert_eq!(state.values(count(15)).unwrap()[0].value, Some(0.0));
    assert_eq!(
        state.values(count(16)).unwrap()[0].missing_reason,
        Some(FeatureMissingReasonV1::Expired)
    );
    state.observe(&row(17, 18, 2, None)).unwrap();
    assert_eq!(
        state.values(count(18)).unwrap()[0].missing_reason,
        Some(FeatureMissingReasonV1::SourceMissing)
    );
}

#[test]
fn availability_orders_delivery_event_time_ranks_only_eligible_arrivals() {
    let mut state = FeatureState::new(&[definition()]).unwrap();
    state.observe(&row(10, 12, 1, Some(1.0))).unwrap();
    state.observe(&row(11, 12, 2, Some(2.0))).unwrap();
    state.observe(&row(11, 12, 3, Some(3.0))).unwrap();
    assert_eq!(state.values(count(12)).unwrap()[0].value, Some(3.0));
    state.observe(&row(9, 13, 4, Some(4.0))).unwrap();
    assert_eq!(
        state.values(count(13)).unwrap()[0].value,
        Some(3.0),
        "late older event does not overwrite latest event"
    );
    assert!(state.observe(&row(12, 12, 5, Some(5.0))).is_err());
}

#[test]
fn modeled_lag_preserves_unknown_observed_clock_and_rejects_overflow() {
    let mut schema = definition();
    schema.availability = FeatureAvailabilityV1::ModeledLag { lag_ns: count(3) };
    let mut event = row(10, 12, 1, Some(7.0));
    event.observed_available_ns = None;
    assert_eq!(effective_available_ns(&schema, &event).unwrap(), count(13));
    let mut state = FeatureState::new(&[schema.clone()]).unwrap();
    state.observe(&event).unwrap();
    let result = state.values(count(13)).unwrap();
    assert_eq!(result[0].observed_available_ns, None);
    assert_eq!(result[0].effective_available_ns, Some(count(13)));
    schema.availability = FeatureAvailabilityV1::Observed;
    assert!(effective_available_ns(&schema, &event).is_err());
    schema.availability = FeatureAvailabilityV1::ModeledLag {
        lag_ns: count(i64::MAX as u64),
    };
    assert!(effective_available_ns(&schema, &event).is_err());
}

#[test]
fn fragments_require_identical_schema_partition_and_global_unique_rows() {
    let first = part(vec![row(10, 12, 1, Some(0.0))]);
    let second = part(vec![row(12, 14, 2, Some(1.0))]);
    let schema = first.feature_schema.clone();
    assert!(bind_observations(
        &[first.clone(), second.clone()],
        &schema,
        DataPartition::Validation
    )
    .is_ok());
    assert!(bind_observations(
        &[first.clone(), first.clone()],
        &schema,
        DataPartition::Validation
    )
    .is_err());
    let mut wrong = second.clone();
    wrong.partition = DataPartition::Discovery;
    assert!(
        bind_observations(&[first.clone(), wrong], &schema, DataPartition::Validation).is_err()
    );
    let mut wrong = second;
    wrong.feature_schema[0].source_ref = "other-source".into();
    assert!(bind_observations(&[first, wrong], &schema, DataPartition::Validation).is_err());
}

#[test]
fn future_value_changes_cannot_change_prefix_and_fold_state_is_fresh() {
    fn prefix(future: f64) -> Vec<FeatureValueV1> {
        let input = part(vec![
            row(10, 12, 1, Some(0.0)),
            row(20, 22, 2, Some(future)),
        ]);
        observations(&input).unwrap();
        let mut state = FeatureState::new(&input.feature_schema).unwrap();
        for row in input
            .observations
            .iter()
            .filter(|r| r.observed_available_ns.unwrap() <= count(12))
        {
            state.observe(row).unwrap();
        }
        state.values(count(12)).unwrap()
    }
    assert_eq!(prefix(-100.0), prefix(100.0));
    let fresh = FeatureState::new(&[definition()])
        .unwrap()
        .values(count(12))
        .unwrap();
    assert_eq!(
        fresh[0].missing_reason,
        Some(FeatureMissingReasonV1::NotYetAvailable)
    );
}

#[test]
fn invalid_values_clocks_and_duplicate_schema_fail_closed() {
    for invalid in [row(10, 9, 1, Some(0.0)), row(10, 12, 1, Some(f64::NAN))] {
        assert!(observations(&part(vec![invalid])).is_err());
    }
    let mut invalid = row(10, 12, 1, None);
    invalid.missing_reason = None;
    assert!(observations(&part(vec![invalid])).is_err());
    assert!(schema(&[definition(), definition()]).is_err());
    let mut invalid = part(vec![row(10, 12, 1, Some(0.0))]);
    invalid.partition = DataPartition::Sealed;
    assert!(observations(&invalid).is_err());
}
