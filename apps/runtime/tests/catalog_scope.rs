//! Real SQLite parameters with explicit synthetic source metadata. No network/OCI claim.
#[path = "../../../tests/support/catalog_metadata.rs"]
mod catalog_fixture;
#[path = "../../job/tests/support/polymarket.rs"]
mod prediction;
use portfolio_config::execution_models;
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
        5 | 8 => {
            let input = serde_json::from_str(include_str!(
                "../../../tests/contracts/allocation-input.json"
            ))
            .unwrap();
            let mut request = portfolio_config::request(&input);
            request.assets.truncate(selection.bar_types.len());
            request
                .execution_settings
                .fee_rates
                .truncate(request.assets.len());
            request.selection = selection;
            request
                .current_weights
                .weights
                .truncate(request.assets.len());
            // The allocation fixture's default asset IDs are not the registered
            // catalog's IDs. Bind all three views to the actual native selection;
            // unregistered selections still fail the independent catalog checks.
            for ((asset, fee), name) in request
                .assets
                .iter_mut()
                .zip(&mut request.execution_settings.fee_rates)
                .zip(&request.selection.bar_types)
            {
                let id = name
                    .parse::<nautilus_model::data::BarType>()
                    .unwrap()
                    .instrument_id()
                    .to_string();
                asset.instrument_id = id.clone();
                fee.instrument_id = id;
            }
            for (weight, asset) in request
                .current_weights
                .weights
                .iter_mut()
                .zip(&request.assets)
            {
                weight.instrument_id = asset.instrument_id.clone();
            }
            let one: contracts::DecimalValue = "1".parse().unwrap();
            let cash = request
                .assets
                .iter()
                .fold(one.as_decimal().clone(), |cash, a| {
                    cash - a.current_weight.as_decimal()
                });
            request.current_weights.cash_weight = cash.to_plain_string().parse().unwrap();
            request.current_weights.asof_ns = request.selection.decision_cutoff_ns;
            request.current_weights.available_ns = request.selection.decision_cutoff_ns;
            request.current_weights.valid_until_ns =
                count(request.selection.decision_cutoff_ns.get() + 1);
            for member in &mut request.members {
                member.model_artifact_id = model;
            }
            if kind == 8 {
                request.mandate.rebalance_schedule.kind =
                    contracts::portfolio::RebalanceKind::FixedInterval;
                request.mandate.rebalance_schedule.interval_seconds = Some(60);
                request.mandate.rebalance_schedule.target_ttl_seconds = 120;
                for asset in &mut request.assets {
                    asset.current_weight = "0".parse().unwrap();
                }
                return NativeTaskParametersV1::StudyPortfolio {
                    schema_version: SchemaV1,
                    dataset_revision_id: dataset,
                    request: Box::new(NativePortfolioStudyRequestV1 {
                        settlements: Vec::new(),
                        schema_version: SchemaV1,
                        source_selection: request.selection,
                        evaluation_start_ns: count(120_000_000_000),
                        manual_cutoffs_ns: None,
                        calendar: None,
                        rolling_liquidity: None,
                        research_available_through_ns: count(119_000_000_000),
                        mandate: request.mandate,
                        execution_settings: request.execution_settings,
                        assets: request.assets,
                        members: request.members,
                    }),
                };
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
                settlements: Vec::new(),
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
            request: Box::new(NativeSimulationRequestV1 {
                settlements: Vec::new(),
                schema_version: SchemaV1,
                selection,
                settings: NativeSimulationSettingsV1 {
                    schema_version: SchemaV1,
                    base_currency: "USD".into(),
                    starting_capital: "1000".parse().unwrap(),
                    account_kind: NativeAccountKind::Margin,
                    leverage: "1".parse().unwrap(),
                    fill_model: execution_models::fill(),
                    fee_model: execution_models::fee(),
                    latency_model: execution_models::latency(1),
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
            }),
        },
    }
}

async fn accepts(
    kind: u8,
    metadata: RuntimeCatalogMetadataV1,
    selection: NativeBarSelectionV1,
) -> bool {
    accepts_with_settlements(kind, metadata, selection, Vec::new()).await
}

async fn accepts_with_settlements(
    kind: u8,
    metadata: RuntimeCatalogMetadataV1,
    selection: NativeBarSelectionV1,
    settlements: Vec<contracts::settlement::NativeSettlementGroupV1>,
) -> bool {
    let root = tempfile::tempdir().unwrap();
    let journal = Journal::open(&root.path().join("journal.sqlite"), 64 * 1024 * 1024, 4)
        .await
        .unwrap();
    let dataset = Id::new();
    let model = Id::new();
    let mut parameters = if matches!(kind, 6 | 7) {
        let mut actual = metadata.quality.datasets[0].selection.clone();
        actual.decision_cutoff_ns = actual.event_end_ns;
        let NativeTaskParametersV1::SimulatePortfolio { mut request, .. } =
            operation(2, dataset, model, actual)
        else {
            unreachable!()
        };
        if kind == 6 {
            NativeTaskParametersV1::SimulateCandidate {
                schema_version: SchemaV1,
                candidate_id: Id::new(),
                candidate_available_ns: request.target_points[0].asof_ns,
                dataset_revision_id: dataset,
                source_selection: selection,
                target_artifact_id: model,
                settings_artifact_id: Id::new(),
                request,
            }
        } else {
            let mut later = request.target_points[0].clone();
            later.asof_ns = count(later.asof_ns.get() + 1);
            request.target_points.push(later);
            NativeTaskParametersV1::SimulatePortfolioSequence {
                schema_version: SchemaV1,
                dataset_revision_id: dataset,
                source_selection: selection,
                sources: request
                    .target_points
                    .iter()
                    .map(|point| NativePortfolioTargetSourceV1 {
                        candidate_id: Id::new(),
                        candidate_available_ns: point.asof_ns,
                        target_artifact_id: Id::new(),
                    })
                    .collect(),
                settings_artifact_id: Id::new(),
                request,
            }
        }
    } else {
        operation(kind, dataset, model, selection)
    };
    match &mut parameters {
        NativeTaskParametersV1::ValidateData { selections, .. } => {
            for selected in selections {
                selected.settlements = settlements.clone();
            }
        }
        NativeTaskParametersV1::SimulatePortfolio { request, .. }
        | NativeTaskParametersV1::SimulateCandidate { request, .. }
        | NativeTaskParametersV1::SimulatePortfolioSequence { request, .. } => {
            request.settlements = settlements
        }
        NativeTaskParametersV1::StudyPortfolio { request, .. } => request.settlements = settlements,
        _ => {}
    }
    let parameter = Id::new();
    let encoded = serde_json::to_vec(&parameters).unwrap();
    journal.put_object(parameter, "1", &encoded).await.unwrap();
    let mut inputs = vec![RuntimeInputV1::Dataset {
        revision_id: dataset,
        registered_ref: metadata.registered_ref.clone(),
        storage_version: metadata.storage_version.clone(),
        role: metadata.partition,
    }];
    let source_objects = match &parameters {
        NativeTaskParametersV1::SimulateCandidate {
            target_artifact_id,
            settings_artifact_id,
            ..
        } => vec![
            (*target_artifact_id, ArtifactInputRole::Report),
            (*settings_artifact_id, ArtifactInputRole::Parameters),
        ],
        NativeTaskParametersV1::SimulatePortfolioSequence {
            sources,
            settings_artifact_id,
            ..
        } => sources
            .iter()
            .map(|s| (s.target_artifact_id, ArtifactInputRole::Report))
            .chain(std::iter::once((
                *settings_artifact_id,
                ArtifactInputRole::Parameters,
            )))
            .collect(),
        _ => Vec::new(),
    };
    for (id, role) in source_objects {
        journal.put_object(id, "1", b"{}").await.unwrap();
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: count(2),
            role,
        });
    }
    if let NativeTaskParametersV1::BuildPortfolio { request, .. } = &parameters {
        let bytes = serde_json::to_vec(&request.current_weights).unwrap();
        journal
            .put_object(request.current_weights_artifact_id, "1", &bytes)
            .await
            .unwrap();
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: request.current_weights_artifact_id,
            storage_version: "1".into(),
            byte_count: count(bytes.len() as u64),
            role: ArtifactInputRole::Report,
        });
        let bytes = serde_json::to_vec(&request.execution_settings).unwrap();
        let id = request.mandate.constraints.transaction_costs_ref;
        journal.put_object(id, "1", &bytes).await.unwrap();
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: count(bytes.len() as u64),
            role: ArtifactInputRole::Parameters,
        });
    }
    if let NativeTaskParametersV1::StudyPortfolio { request, .. } = &parameters {
        let bytes = serde_json::to_vec(&request.execution_settings).unwrap();
        let id = request.mandate.constraints.transaction_costs_ref;
        journal.put_object(id, "1", &bytes).await.unwrap();
        inputs.push(RuntimeInputV1::Artifact {
            artifact_id: id,
            storage_version: "1".into(),
            byte_count: count(bytes.len() as u64),
            role: ArtifactInputRole::Parameters,
        });
    }
    if matches!(kind, 1 | 3 | 4 | 5 | 8) {
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
async fn all_data_operations_cannot_widen_the_registered_visibility_cutoff() {
    for kind in 0..9 {
        let mut metadata = catalog_fixture::metadata();
        if matches!(kind, 3 | 8) {
            metadata.partition = DataPartition::Validation;
        }
        if kind == 4 {
            metadata.partition = DataPartition::Sealed;
        }
        if (5..8).contains(&kind) {
            metadata.partition = DataPartition::Forward;
        }
        let selected = metadata.quality.datasets[0].selection.clone();
        assert!(
            accepts(kind, metadata.clone(), selected.clone()).await,
            "kind {kind}"
        );
        let mut narrower = selected.clone();
        narrower.decision_cutoff_ns = narrower.event_end_ns;
        assert!(accepts(kind, metadata.clone(), narrower).await);
        let mut later = selected;
        later.decision_cutoff_ns = count(later.decision_cutoff_ns.get() + 1);
        assert!(!accepts(kind, metadata, later).await);
    }
}

#[tokio::test]
async fn all_data_operations_reject_unregistered_types_instruments_and_event_ranges() {
    for kind in 0..9 {
        let mut metadata = catalog_fixture::metadata();
        if matches!(kind, 3 | 8) {
            metadata.partition = DataPartition::Validation;
        }
        if kind == 4 {
            metadata.partition = DataPartition::Sealed;
        }
        if (5..8).contains(&kind) {
            metadata.partition = DataPartition::Forward;
        }
        domain::catalogs::metadata(&metadata, now()).unwrap();
        let selection = metadata.quality.datasets[0].selection.clone();
        assert!(
            accepts(kind, metadata.clone(), selection.clone()).await,
            "kind {kind}"
        );
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

#[tokio::test]
async fn frozen_settlement_vectors_cannot_be_replaced_under_the_same_registered_snapshot() {
    let mut metadata = catalog_fixture::metadata();
    let id = prediction::IDS[0];
    metadata.universe.membership[0].instrument_id = id.into();
    metadata.universe.instrument_definitions = prediction::instruments("0", 200_000_000_000)
        .iter()
        .map(|instrument| serde_json::to_value(instrument).unwrap())
        .collect();
    let quality = &mut metadata.quality.datasets[0];
    quality.instrument_ids = vec![id.into()];
    quality.selection.bar_types = vec![format!("{id}-1-MINUTE-LAST-EXTERNAL")];
    quality.settlements =
        prediction::settlement_groups(200_000_000_000, 230_000_000_000, ["1", "0"]);
    let selected = quality.selection.clone();
    let frozen = quality.settlements.clone();
    domain::catalogs::metadata(&metadata, now()).unwrap();
    assert!(accepts_with_settlements(0, metadata.clone(), selected.clone(), frozen.clone()).await);
    assert!(!accepts(0, metadata.clone(), selected.clone()).await);
    for mutation in 0..3 {
        let mut replaced = frozen.clone();
        match mutation {
            0 => {
                for o in &mut replaced[0].outcomes {
                    o.close_price = "0.5".parse().unwrap();
                }
            }
            1 => {
                for o in &mut replaced[0].outcomes {
                    o.ts_init = count(229_000_000_000);
                }
            }
            _ => {
                replaced[0].outcomes.pop();
            }
        }
        assert!(!accepts_with_settlements(0, metadata.clone(), selected.clone(), replaced).await);
    }
    let mut restarted = metadata.clone();
    for outcome in &mut restarted.quality.datasets[0].settlements[0].outcomes {
        outcome.close_price = "0.5".parse().unwrap();
    }
    domain::catalogs::metadata(&restarted, now()).unwrap();
    assert!(!accepts_with_settlements(0, restarted, selected.clone(), frozen).await);
    // No outcome vector is introduced into the Alpha forecast request.
    assert!(accepts(1, metadata, selected).await);
}
