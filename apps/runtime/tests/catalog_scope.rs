//! Real SQLite parameters with explicit synthetic source metadata. No network/OCI claim.
#[path = "../../../tests/support/catalog_metadata.rs"]
mod catalog_fixture;
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_config;
use catalog_fixture::count;
use contracts::{
    catalogs::RuntimeCatalogMetadataV1,
    execution::{NativeDatasetSelectionV1, NativeTaskParametersV1},
    research::{ArtifactInputRole, DataPartition, SplitKind, SplitPolicyV1},
    runtime_jobs::*,
    science::*,
    Id, Revision, SchemaV1,
};
use runtime::{config::RegisteredCatalog, journal::Journal, materialize, now};

fn operation(
    kind: u8,
    dataset: Id,
    model: Id,
    selection: NativeBarSelectionV1,
) -> NativeTaskParametersV1 {
    match kind {
        5 => {
            let input = serde_json::from_str(include_str!(
                "../../../tests/contracts/allocation-input.json"
            ))
            .unwrap();
            let mut request = portfolio_config::request(&input);
            request.assets.truncate(selection.bar_types.len());
            request.selection = selection;
            for member in &mut request.members {
                member.model_artifact_id = model;
            }
            NativeTaskParametersV1::BuildPortfolio {
                schema_version: SchemaV1,
                dataset_revision_id: dataset,
                request: Box::new(request),
            }
        }
        0 => NativeTaskParametersV1::ValidateData {
            schema_version: SchemaV1,
            selections: vec![NativeDatasetSelectionV1 {
                dataset_revision_id: dataset,
                selection,
            }],
        },
        1 | 3 | 4 => {
            let forecast = NativeForecastRequestV1 {
                schema_version: SchemaV1,
                selection,
                parameters: NativeForecastParametersV1 {
                    schema_version: SchemaV1,
                    fast_period: 1,
                    slow_period: 2,
                    label_horizon_observations: 1,
                    total_fuel: count(1000),
                },
            };
            if kind == 1 {
                NativeTaskParametersV1::EvaluateAlpha {
                    schema_version: SchemaV1,
                    dataset_revision_id: dataset,
                    model_artifact_id: model,
                    request: forecast,
                }
            } else if kind == 4 {
                NativeTaskParametersV1::EvaluateSealedAlpha {
                    schema_version: SchemaV1,
                    dataset_revision_id: dataset,
                    model_artifact_id: model,
                    calibration_artifact_id: None,
                    request: Box::new(NativeAlphaSealedRequestV1 {
                        schema_version: SchemaV1,
                        forecast,
                        target_kind: contracts::brief::TargetKind::ExpectedReturn,
                        research_available_through_ns: count(0),
                    }),
                }
            } else {
                NativeTaskParametersV1::ValidateAlpha {
                    schema_version: SchemaV1,
                    dataset_revision_id: dataset,
                    model_artifact_id: model,
                    request: Box::new(NativeAlphaValidationRequestV1 {
                        schema_version: SchemaV1,
                        forecast,
                        split_policy: SplitPolicyV1 {
                            schema_version: SchemaV1,
                            kind: SplitKind::WalkForward,
                            train_size: count(3),
                            test_size: count(1),
                            step_size: Some(count(1)),
                            group_count: None,
                            test_group_count: None,
                            purge_observations: count(1),
                            embargo_observations: count(0),
                            label_horizon_observations: Some(count(1)),
                            interval_validation_required: true,
                            sealed_revision_id: Id::new(),
                        },
                        target_kind: contracts::brief::TargetKind::Score,
                    }),
                }
            }
        }
        _ => NativeTaskParametersV1::SimulatePortfolio {
            schema_version: SchemaV1,
            dataset_revision_id: dataset,
            request: NativeSimulationRequestV1 {
                schema_version: SchemaV1,
                selection,
                settings: NativeSimulationSettingsV1 {
                    schema_version: SchemaV1,
                    base_currency: "USD".into(),
                    starting_capital: "1000".parse().unwrap(),
                    account_kind: NativeAccountKind::Margin,
                    leverage: "1".parse().unwrap(),
                    insert_latency_ns: count(1),
                    snapshot_interval_ms: 1000,
                    exposure_tolerance: "0.00001".parse().unwrap(),
                    fee_rates: vec![NativeFeeRateV1 {
                        instrument_id: "EUR/USD.SIM".into(),
                        maker: "0".parse().unwrap(),
                        taker: "0".parse().unwrap(),
                    }],
                },
                target_points: vec![NativeTargetPointV1 {
                    schema_version: SchemaV1,
                    asof_ns: count(60_000_000_000),
                    valid_until_ns: count(240_000_000_000),
                    targets: vec![contracts::portfolio::AllocationTargetV1 {
                        instrument_id: "EUR/USD.SIM".into(),
                        currency: "USD".into(),
                        weight: "1".parse().unwrap(),
                    }],
                    cash_weight: "0".parse().unwrap(),
                }],
            },
        },
    }
}

async fn accepts(
    kind: u8,
    metadata: RuntimeCatalogMetadataV1,
    selection: NativeBarSelectionV1,
) -> bool {
    let root = tempfile::tempdir().unwrap();
    let journal = Journal::open(&root.path().join("journal.sqlite"), 64 * 1024 * 1024, 4)
        .await
        .unwrap();
    let dataset = Id::new();
    let model = Id::new();
    let parameters = operation(kind, dataset, model, selection);
    let parameter = Id::new();
    let encoded = serde_json::to_vec(&parameters).unwrap();
    journal.put_object(parameter, "1", &encoded).await.unwrap();
    let mut inputs = vec![RuntimeInputV1::Dataset {
        revision_id: dataset,
        registered_ref: metadata.registered_ref.clone(),
        storage_version: metadata.storage_version.clone(),
        role: metadata.partition,
    }];
    if matches!(kind, 1 | 3 | 4 | 5) {
        journal
            .put_object(model, "1", b"controlled-model-fixture")
            .await
            .unwrap();
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: model,
            storage_version: "1".into(),
            byte_count: count(24),
            role: ArtifactInputRole::Model,
        });
    }
    let run = Id::new();
    let spec = JobSpecV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: domain::runtime_jobs::external_id(run, 1).unwrap(),
        job_kind: parameters.job_kind(),
        image_ref: format!("sha256:{}", "a".repeat(64)),
        input_set_id: Id::new(),
        inputs,
        parameters_artifact_id: parameter,
        limits: RuntimeJobLimitsV1 {
            cpu: 1,
            cpu_seconds: count(1),
            memory_mib: 64,
            wall_seconds: 30,
            output_bytes: count(4096),
        },
        deadline_at: now() + chrono::Duration::seconds(60),
        requested_output_schemas: parameters.output_schemas(),
    };
    let registered = RegisteredCatalog {
        root: root.path().to_owned(),
        raw_metadata: serde_json::to_vec(&metadata).unwrap(),
        metadata,
    };
    let result = materialize::parameters(&journal, &spec, &[registered])
        .await
        .is_ok();
    assert!(journal.scheduling().await.unwrap().is_empty());
    assert_eq!(
        journal
            .materialization_bytes(&spec.external_job_id)
            .await
            .unwrap(),
        None
    );
    journal.close().await;
    result
}

#[tokio::test]
async fn all_six_data_operations_cannot_widen_the_registered_visibility_cutoff() {
    for kind in 0..6 {
        let mut metadata = catalog_fixture::metadata();
        if kind == 3 {
            metadata.partition = DataPartition::Validation;
        }
        if kind == 4 {
            metadata.partition = DataPartition::Sealed;
        }
        if kind == 5 {
            metadata.partition = DataPartition::Forward;
        }
        let selected = metadata.quality.datasets[0].selection.clone();
        assert!(accepts(kind, metadata.clone(), selected.clone()).await);
        let mut narrower = selected.clone();
        narrower.decision_cutoff_ns = narrower.event_end_ns;
        assert!(accepts(kind, metadata.clone(), narrower).await);
        let mut later = selected;
        later.decision_cutoff_ns = count(later.decision_cutoff_ns.get() + 1);
        assert!(!accepts(kind, metadata, later).await);
    }
}

#[tokio::test]
async fn all_six_data_operations_reject_unregistered_types_instruments_and_event_ranges() {
    for kind in 0..6 {
        let mut metadata = catalog_fixture::metadata();
        if kind == 3 {
            metadata.partition = DataPartition::Validation;
        }
        if kind == 4 {
            metadata.partition = DataPartition::Sealed;
        }
        if kind == 5 {
            metadata.partition = DataPartition::Forward;
        }
        domain::catalogs::metadata(&metadata, now()).unwrap();
        let selection = metadata.quality.datasets[0].selection.clone();
        assert!(accepts(kind, metadata.clone(), selection.clone()).await);
        let mut foreign = selection.clone();
        foreign.bar_types = vec!["GBP/USD.SIM-1-MINUTE-LAST-EXTERNAL".into()];
        assert!(!accepts(kind, metadata.clone(), foreign).await);
        let mut aggregation = selection.clone();
        aggregation.bar_types = vec!["EUR/USD.SIM-5-MINUTE-LAST-EXTERNAL".into()];
        assert!(!accepts(kind, metadata.clone(), aggregation).await);
        let mut widened = selection.clone();
        widened.event_start_ns = count(0);
        assert!(!accepts(kind, metadata.clone(), widened).await);
        let mut widened = selection.clone();
        widened.event_end_ns = count(260_000_000_000);
        assert!(!accepts(kind, metadata.clone(), widened).await);
        let mut wrong_identity = metadata.clone();
        wrong_identity.quality.datasets[0].instrument_ids = vec!["GBP/USD.SIM".into()];
        assert!(!accepts(kind, wrong_identity, selection.clone()).await);
        let mut missing_member = metadata;
        missing_member.universe.membership.clear();
        assert!(!accepts(kind, missing_member, selection).await);
    }
}
