//! Controlled paired-report buffers for Store scientific-consumer regression.
//! Original buffers and native metadata retain fixture identity, not market or license attestation.
#![allow(dead_code)]
use contracts::{
    catalogs::{RecordedFeatureFragmentV1, RecordedFeatureInputsV1},
    research::DataPartition,
    science::{
        FeatureAvailabilityV1, FeatureDefinitionV1, FeatureObservationV1, FeatureObservationsV1,
    },
    DbCounter, SchemaV1,
};
fn n(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}

pub fn descriptor(parts: &[(String, String)]) -> RecordedFeatureInputsV1 {
    let mut fragments = Vec::new();
    let mut partition = None;
    for (key, raw) in parts {
        let content: FeatureObservationsV1 = serde_json::from_str(raw).unwrap();
        assert!(partition.is_none_or(|old| old == content.partition));
        partition = Some(content.partition);
        fragments.push(RecordedFeatureFragmentV1 {
            part_key: key.clone(),
            byte_count: n(raw.len() as u64),
            observations: n(content.observations.len() as u64),
            min_event_ns: content
                .observations
                .iter()
                .map(|r| r.event_ns)
                .min()
                .unwrap(),
            max_event_ns: content
                .observations
                .iter()
                .map(|r| r.event_ns)
                .max()
                .unwrap(),
            min_observed_available_ns: content
                .observations
                .iter()
                .filter_map(|r| r.observed_available_ns)
                .min()
                .unwrap(),
            max_observed_available_ns: content
                .observations
                .iter()
                .filter_map(|r| r.observed_available_ns)
                .max()
                .unwrap(),
        });
    }
    RecordedFeatureInputsV1 {
        schema_version: SchemaV1,
        source_selection_start_ns: fragments.iter().map(|f| f.min_event_ns).min().unwrap(),
        source_selection_end_ns: n(fragments
            .iter()
            .map(|f| f.max_event_ns.get())
            .max()
            .unwrap()
            + 1),
        partition: partition.unwrap(),
        fragments,
    }
}
pub fn parts() -> (RecordedFeatureInputsV1, Vec<(String, String)>) {
    let schema = vec![FeatureDefinitionV1 {
        feature_key: "signal".into(),
        source_ref: "fixture:immutable-part".into(),
        source_key: "source-market".into(),
        availability: FeatureAvailabilityV1::Observed,
        max_age_ns: Some(n(60_000_000_000)),
    }];
    let parts: Vec<_> = (0..2)
        .map(|sequence| {
            let event = n(1_577_923_199_000_000_000 + sequence);
            let content = FeatureObservationsV1 {
                schema_version: SchemaV1,
                partition: DataPartition::Validation,
                feature_schema: schema.clone(),
                observations: vec![FeatureObservationV1 {
                    feature_index: 0,
                    event_ns: event,
                    observed_available_ns: Some(event),
                    sequence: n(sequence),
                    value: Some(0.5),
                    missing_reason: None,
                }],
            };
            (
                format!("features-{sequence:04}"),
                format!(" \n{}\n", serde_json::to_string(&content).unwrap()),
            )
        })
        .collect();
    (descriptor(&parts), parts)
}
