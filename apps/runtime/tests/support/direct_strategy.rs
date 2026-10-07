//! Native producer continuation of direct_research, using only public Store commands.
//! Optional file output contains the original SQL claim for native_paper_target;
//! writing that file alone is not evidence that the Paper consumer has executed.
use super::*;
#[path = "../../../../tests/support/strategy_composition.rs"]
mod strategy_fixture;
use contracts::{
    control::{AssignablePrincipalKind, CredentialIssue, ListQuery, MachineScope, PrincipalCreate},
    delivery::*,
    forward::ForwardEnvironmentV1,
    research::{InputItemV1, InputPurpose, InputSetCreate},
    settings::{
        DownstreamConfigurationV1, DownstreamCreate, DownstreamEnvironments, PackageSchemaVersion,
    },
    strategy_portfolio::*,
};
use nautilus_model::{
    data::BarType,
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use std::{future::Future, io::Write, pin::Pin, str::FromStr};
use store::{authority::Actor, StoreError};

pub(super) struct ClaimedStrategy {
    pub claim: HandoffClaimViewV2,
    pub execution_assumptions: contracts::execution_assumptions::ExecutionAssumptionsViewV1,
    pub principal_id: Id,
}

pub(super) fn paper_market(
    market: &mut job::catalog::NativeMarketData,
    request: &mut NativeExperimentEvaluationRequestV1,
) {
    assert_eq!(market.series.len(), 1);
    let series = &mut market.series[0];
    let id = InstrumentId::from("BTCUSDT.BINANCE");
    // Same public Binance fee basis and quantity precision as the existing
    // synthetic Paper feed, set before native compilation/evaluation freezes it.
    series.instrument = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(id)
            .raw_symbol(Symbol::from("BTCUSDT"))
            .base_currency(Currency::from_str("BTC").unwrap())
            .quote_currency(Currency::USDT())
            .price_precision(2)
            .size_precision(6)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.000001"))
            .maker_fee(rust_decimal::Decimal::from_str("0.001").unwrap())
            .taker_fee(rust_decimal::Decimal::from_str("0.001").unwrap())
            .margin_init(rust_decimal::Decimal::ONE)
            .margin_maint(rust_decimal::Decimal::ONE)
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    assert!(series.instrument_updates.is_empty());
    series.bar_type = BarType::from_str("BTCUSDT.BINANCE-1-SECOND-LAST-EXTERNAL").unwrap();
    for bar in &mut series.bars {
        bar.bar_type = series.bar_type;
        bar.volume = Quantity::from("1000000.000000");
    }
    request.instrument_id = id.to_string();
    request.selection.bar_types = vec![series.bar_type.to_string()];
    request.settings.starting_capital = "10000".parse().unwrap();
    request.settings.exposure_tolerance = "0.00001".parse().unwrap();
    request.settings.fee_rates[0].instrument_id = id.to_string();
    request.settings.fee_rates[0].maker = "0.001".parse().unwrap();
    // Bounded engineering window includes the actual compiler and OCI runs,
    // plus the existing Paper consumer. No accepted clocks are refreshed later.
    request.target_ttl_ns = oci::count(600 * fixture::SECOND);
}

// These fixture continuations keep large async state on the heap. A separate
// constructor avoids embedding their construction scratch space in caller polls.
#[inline(never)]
fn compose<'a>(
    pool: &'a PgPool,
    prepared: &'a scientific_catalog::Prepared,
    remote: &'a mut oci::Fixture,
    worker: &'a Worker,
    shutdown: tokio::sync::watch::Receiver<bool>,
    build: &'a StrategyPortfolioBuildV1,
    key: &'a str,
) -> Pin<
    Box<
        impl Future<
                Output = (
                    StrategyPortfolioCandidateV1,
                    NativeStrategyCompositionResultV1,
                ),
            > + 'a,
    >,
> {
    Box::pin(async move {
        let store = &prepared.store;
        let actor = &prepared.actor;
        let data = &prepared.data;
        let admitted = store
            .start_strategy_portfolio_build(
                actor,
                key,
                build,
                |id, size| data.read(id, size),
                |object| async move {
                    data.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                },
            )
            .await
            .unwrap();
        let replay = store
            .start_strategy_portfolio_build(
                actor,
                key,
                build,
                |id, size| data.read(id, size),
                |_| async { panic!("idempotent admission must not publish another task") },
            )
            .await
            .unwrap();
        assert!(!admitted.replayed && replay.replayed);
        assert_eq!(admitted.resource.id, replay.resource.id);
        assert_eq!(
            admitted.resource.kind,
            contracts::runs::RunKind::PortfolioBuild
        );
        let (finished, spec, message) = execute(
            pool,
            prepared,
            remote,
            worker,
            admitted.resource.id,
            shutdown.clone(),
        )
        .await;
        let task_bytes = data
            .objects
            .read(
                spec.parameters_artifact_id,
                spec.inputs
                    .iter()
                    .find_map(|input| match input {
                        RuntimeInputV1::Artifact {
                            artifact_id,
                            byte_count,
                            ..
                        } if *artifact_id == spec.parameters_artifact_id => Some(*byte_count),
                        _ => None,
                    })
                    .unwrap(),
            )
            .unwrap();
        let NativeTaskParametersV1::ComposeStrategyTargets {
            dataset_revision_id,
            request,
            ..
        } = serde_json::from_slice(&task_bytes).unwrap()
        else {
            panic!("original dispatch must be ComposeStrategyTargets");
        };
        assert_eq!(spec.requested_output_schemas.len(), 1);
        assert_eq!(
            spec.requested_output_schemas[0].name,
            "qz.strategy_portfolio"
        );
        let artifact = published(
            pool,
            &data.objects,
            finished.id,
            finished.active_attempt_id.unwrap(),
            "qz.strategy_portfolio",
        )
        .await;
        assert_eq!(
            remote_bytes(remote, &spec, artifact.remote_storage_ref).await,
            artifact.bytes
        );
        let report: NativeStrategyCompositionResultV1 =
            serde_json::from_slice(&artifact.bytes).unwrap();
        assert_eq!(
            serde_json::to_value(&report.request).unwrap(),
            serde_json::to_value(&request).unwrap()
        );
        assert_eq!(
            dataset_revision_id,
            report.request.input_provenance.dataset_revision_id
        );
        domain::execution::strategy::result(&request, &report).unwrap();
        for input in &spec.inputs {
            if let RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } = input
            {
                let original = data.read(*artifact_id, *byte_count).await.unwrap();
                assert_existing_input(remote, &spec, *artifact_id, &original).await;
            }
        }
        assert_eq!(report.native_versions["strategy-composition"], "1");
        assert_eq!(report.native_versions["nautilus-backtest"], "0.63.0");
        // The production Worker must already have published the candidate before
        // acknowledge_run succeeds; this reads that projection, never accepts a report.
        let candidates = store
            .candidates_envelope(
                actor,
                data.data.project,
                &ListQuery {
                    cursor: None,
                    limit: 100,
                },
            )
            .await
            .unwrap();
        let candidate = candidates
            .items
            .into_iter()
            .find_map(|candidate| match candidate {
                PortfolioCandidateListEnvelopeV2::Strategy(candidate)
                    if candidate.run_id == finished.id =>
                {
                    Some(candidate)
                }
                _ => None,
            })
            .expect("production Worker publishes the original composition candidate");
        let PortfolioCandidateEnvelopeV2::Strategy(readback) =
            store.candidate_envelope(actor, candidate.id).await.unwrap()
        else {
            panic!("strategy candidate readback")
        };
        assert_eq!(
            serde_json::to_value(&candidate).unwrap(),
            serde_json::to_value(&readback).unwrap()
        );
        assert_eq!(
            candidate.accepted_attempt_id,
            finished.active_attempt_id.unwrap()
        );
        assert_eq!(candidate.report_artifact_id, artifact.artifact_id);
        assert_eq!(candidate.members.len(), 1);
        assert_eq!(
            candidate.members[0].alpha_version_id,
            build.members[0].alpha_version_id
        );
        assert_eq!(candidate.members[0].ensemble_weight, "1".parse().unwrap());
        assert_eq!(
            serde_json::to_value(&candidate.purpose).unwrap(),
            serde_json::to_value(&build.purpose).unwrap()
        );
        assert_eq!(
            candidate.input_provenance.market_data_origin,
            DataOrigin::Fixture
        );
        assert_eq!(candidate.input_provenance.pit_status, PitStatus::Unverified);
        assert!(candidate
            .input_provenance
            .feature_artifact_origins
            .values()
            .all(|origin| *origin == DataOrigin::Synthetic));
        let before: Value =
            sqlx::query_scalar("SELECT to_jsonb(c) FROM app.research_cycles c WHERE id=$1")
                .bind(build.cycle_id.as_uuid())
                .fetch_one(pool)
                .await
                .unwrap();
        worker
            .process_message(message.clone(), "native-strategy-redelivery", shutdown)
            .await
            .unwrap();
        let after: Value =
            sqlx::query_scalar("SELECT to_jsonb(c) FROM app.research_cycles c WHERE id=$1")
                .bind(build.cycle_id.as_uuid())
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(
            before, after,
            "duplicate native messages must not charge twice"
        );
        let counts: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.run_attempts WHERE run_id=$1),(SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1),(SELECT count(*) FROM app.portfolio_candidates WHERE run_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE msg_id=$2),(SELECT count(*) FROM pgmq.a_runs WHERE msg_id=$2)").bind(finished.id.as_uuid()).bind(message.message_id).fetch_one(pool).await.unwrap();
        assert_eq!(counts, (1, 1, 1, 0, 1));
        let repeated = published(
            pool,
            &data.objects,
            finished.id,
            finished.active_attempt_id.unwrap(),
            "qz.strategy_portfolio",
        )
        .await;
        assert_eq!(repeated.artifact_id, artifact.artifact_id);
        assert_eq!(repeated.bytes, artifact.bytes);
        (candidate, report)
    })
}

// Keep test-stage inputs explicit without changing the boxed continuation boundary.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
pub(super) fn research_to_claim<'a>(
    pool: &'a PgPool,
    prepared: &'a scientific_catalog::Prepared,
    remote: &'a mut oci::Fixture,
    worker: &'a Worker,
    shutdown: tokio::sync::watch::Receiver<bool>,
    experiment: &'a ExperimentView,
    source: &'a NativeExperimentEvaluationResultV1,
    forward: FeatureObservationsV1,
) -> Pin<Box<impl Future<Output = ClaimedStrategy> + 'a>> {
    Box::pin(async move {
        let store = &prepared.store;
        let actor = &prepared.actor;
        let data = &prepared.data;
        let alpha = store
            .adopt_experiment_alpha(
                actor,
                "native-strategy-alpha",
                &StrategyAlphaAdoptIntentV1 {
                    schema_version: SchemaV1,
                    experiment_id: experiment.id,
                    request: StrategyAlphaAdoptV1 {
                        schema_version: SchemaV1,
                        expected_revision: experiment.revision,
                        name: "Original native target policy".into(),
                        source_fold_index: 0,
                    },
                },
                |id, size| data.read(id, size),
            )
            .await
            .unwrap()
            .resource;
        let evaluation = store
            .get_run(actor, experiment.run_id.unwrap())
            .await
            .unwrap();
        assert_eq!(alpha.policy.source.experiment_id, experiment.id);
        assert_eq!(alpha.policy.source.evaluation_run_id, evaluation.id);
        assert_eq!(
            alpha.policy.source.accepted_attempt_id,
            evaluation.active_attempt_id.unwrap()
        );
        assert_eq!(
            alpha.policy.source.report_artifact_id,
            experiment.conclusion_artifact_id.unwrap()
        );
        assert_eq!(alpha.policy.model_artifact_id, source.model_artifact_id);
        assert_eq!(alpha.policy.runtime_image_ref, oci::image());
        assert_eq!(
            alpha.policy.initialization.first_ordinal,
            source.folds[0].decisions[0].ordinal
        );
        let assumption = store
            .execution_assumption(actor, data.brief.content.execution_assumptions_id)
            .await
            .unwrap();
        let mut content = strategy_fixture::mandate(&source.request.settings);
        content.universe_version_id = data.data.universe;
        content.execution_assumptions_id = assumption.id;
        content.constraints.transaction_costs_ref = assumption.fee_schedule_artifact_id;
        content.max_input_age_seconds = 600;
        content.target_ttl_seconds = 600;
        let mandate = store
            .create_strategy_mandate(
                actor,
                "native-strategy-mandate",
                &StrategyMandateCreateV1 {
                    schema_version: SchemaV1,
                    project_id: data.data.project,
                    runtime_id: data.data.runtime,
                    expected_runtime_revision: data.freeze.execution_context.runtime_revision,
                    content,
                },
            )
            .await
            .unwrap()
            .resource;
        let historical_input = store
            .create_input_set(
                actor,
                "native-strategy-historical",
                &InputSetCreate {
                    schema_version: SchemaV1,
                    project_id: data.data.project,
                    purpose: InputPurpose::Portfolio,
                    decision_cutoff: instant(source.request.selection.decision_cutoff_ns.get()),
                    items: vec![InputItemV1::Dataset {
                        dataset_revision_id: data.data.validation,
                        role: DataPartition::Validation,
                    }],
                },
            )
            .await
            .unwrap()
            .resource;
        let mut limits = experiment_support::limits();
        limits.experiments = 0;
        limits.cpu_seconds = Some(oci::count(60));
        limits.output_bytes = Some(oci::count(4 * 1024 * 1024));
        let mut build = StrategyPortfolioBuildV1 {
            schema_version: SchemaV1,
            source_kind: StrategyPortfolioSourceV1::StrategyAlpha,
            cycle_id: experiment.cycle_id,
            mandate_id: mandate.id,
            input_set_id: historical_input.header.id,
            runtime_id: data.data.runtime,
            expected_runtime_revision: data.freeze.execution_context.runtime_revision,
            members: vec![StrategyMemberSelectionV1 {
                alpha_version_id: alpha.id,
                ensemble_weight: "1".parse().unwrap(),
            }],
            purpose: StrategyPortfolioPurposeV1::HistoricalReplay {},
            limits,
        };
        let (historical, report) = compose(
            pool,
            prepared,
            remote,
            worker,
            shutdown.clone(),
            &build,
            "native-strategy-replay",
        )
        .await;
        let StrategyCompositionOutcomeV1::HistoricalReplay {
            simulation_request,
            simulation,
        } = &report.outcome
        else {
            panic!("historical native dispatch")
        };
        assert_eq!(
            serde_json::to_value(&report.request.members[0].policy).unwrap(),
            serde_json::to_value(&alpha.policy).unwrap()
        );
        assert_eq!(report.consumed_fuel.get(), 0);
        assert_eq!(
            serde_json::to_value(&simulation_request.target_points).unwrap(),
            serde_json::to_value(&source.folds[0].simulation_request.target_points).unwrap()
        );
        assert!(simulation.orders.get() > 0);
        assert!(!simulation.canonical_result["fills"]
            .as_array()
            .unwrap()
            .is_empty());
        let historical_release = store
            .create_strategy_release(
                actor,
                "native-historical-release-rejected",
                &StrategyReleaseCreateV1 {
                    schema_version: SchemaV1,
                    source_kind: StrategyReleaseSourceV1::NativeTargetDecision,
                    candidate_id: historical.id,
                },
                |id, size| data.read(id, size),
                |_| async { panic!("historical targets cannot publish a current release") },
            )
            .await;
        assert!(matches!(
            historical_release,
            Err(StoreError::Invalid("strategy_current_decision_required"))
        ));

        let downstream = store
            .create_downstream(
                actor,
                "native-strategy-downstream",
                &DownstreamCreate {
                    schema_version: SchemaV1,
                    credential_ref: prepared
                        .vault
                        .put("DOWNSTREAM", b"synthetic-native-paper-only")
                        .unwrap(),
                    configuration: DownstreamConfigurationV1 {
                        name: "Native synthetic Paper consumer".into(),
                        endpoint: "http://127.0.0.1:19999".into(),
                        accepted_package_versions: vec![PackageSchemaVersion::V2],
                        environments: DownstreamEnvironments::Paper,
                        enabled: true,
                        development_http: true,
                    },
                },
                |_| async { Ok(()) },
            )
            .await
            .unwrap()
            .resource;
        let cutoff = forward.observations[0].observed_available_ns.unwrap();
        let current_input = store
            .create_input_set(
                actor,
                "native-strategy-current",
                &InputSetCreate {
                    schema_version: SchemaV1,
                    project_id: data.data.project,
                    purpose: InputPurpose::Forward,
                    decision_cutoff: instant(
                        (cutoff.get() / fixture::SECOND + 1) * fixture::SECOND,
                    ),
                    items: vec![InputItemV1::Dataset {
                        dataset_revision_id: data.data.forward,
                        role: DataPartition::Forward,
                    }],
                },
            )
            .await
            .unwrap()
            .resource;
        let feature = external_support::upload(
            store,
            actor,
            data,
            ResearchArtifactKind::Parameters,
            serde_json::to_string(&forward).unwrap(),
        )
        .await;
        build.input_set_id = current_input.header.id;
        build.purpose = StrategyPortfolioPurposeV1::CurrentDecision {
            account_start: FreshPaperCashV1 {
                downstream_id: downstream.id,
                trader_id: "QZ-PAPER01".into(),
                account_id: "BINANCE-PAPER01".into(),
                base_currency: assumption.settings.base_currency.clone(),
                starting_capital: assumption.settings.starting_capital.clone(),
                execution_assumptions_id: assumption.id,
            },
            member_inputs: vec![StrategyCurrentInputsV1 {
                alpha_version_id: alpha.id,
                feature_artifact_ids: vec![feature],
            }],
        };
        let (current, report) = compose(
            pool,
            prepared,
            remote,
            worker,
            shutdown,
            &build,
            "native-strategy-current-build",
        )
        .await;
        let StrategyCompositionOutcomeV1::CurrentDecision {
            target,
            predictions_per_member,
            ..
        } = &report.outcome
        else {
            panic!("current native dispatch")
        };
        assert_eq!(
            serde_json::to_value(&report.request.members[0].policy).unwrap(),
            serde_json::to_value(&alpha.policy).unwrap()
        );
        assert!(report.consumed_fuel.get() > 0);
        assert_eq!(
            predictions_per_member[&alpha.id].get(),
            30,
            "29 original warmup/continuation bars plus the new forward bar"
        );
        assert_eq!(target.asof_ns, cutoff);
        assert_eq!(target.targets[0].weight, "0.5".parse().unwrap());
        assert_eq!(target.cash_weight, "0.5".parse().unwrap());
        assert_eq!(
            target.valid_until_ns.get(),
            cutoff.get() + 600 * fixture::SECOND
        );
        assert_ne!(
            target.targets[0].weight,
            source.folds[0].decisions.last().unwrap().target_weight
        );

        // Capability observation is explicitly controlled in this producer test.
        // The original SQL claim is consumed through the existing separate native
        // Paper test when QZ_NATIVE_STRATEGY_CLAIM_FILE is supplied to both targets.
        let store::downstream::ProbePreparation::Pending(ticket) = store
            .prepare_downstream_probe(
                actor,
                "native-strategy-probe",
                downstream.id,
                &DownstreamProbeRequestV1 {
                    schema_version: SchemaV1,
                    expected_revision: downstream.revision,
                },
            )
            .await
            .unwrap()
        else {
            panic!("fresh probe")
        };
        store
            .complete_downstream_probe(
                *ticket,
                DownstreamProbeOutcomeV1::Available {
                    capabilities: DownstreamCapabilitiesV1 {
                        schema_version: SchemaV1,
                        delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
                        accepted_package_versions: vec![PackageSchemaVersion::V2],
                        environments: vec![ForwardEnvironmentV1::Paper],
                        market_capability_versions: vec![assumption.venue_capability_ref.clone()],
                        accepting_targets: true,
                        checked_at: runtime::now(),
                    },
                },
                |id, bytes| async move {
                    data.objects
                        .put(id, &bytes)
                        .map_err(|_| StoreError::Integrity)
                },
            )
            .await
            .unwrap();
        let release = store
            .create_strategy_release(
                actor,
                "native-strategy-release",
                &StrategyReleaseCreateV1 {
                    schema_version: SchemaV1,
                    source_kind: StrategyReleaseSourceV1::NativeTargetDecision,
                    candidate_id: current.id,
                },
                |id, size| data.read(id, size),
                |object| async move {
                    data.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                },
            )
            .await
            .unwrap()
            .resource;
        let approval = store
            .approve_release(
                actor,
                "native-strategy-approve",
                release.id,
                &ReleaseApproveV1 {
                    schema_version: SchemaV1,
                    downstream_id: downstream.id,
                    environment: ForwardEnvironmentV1::Paper,
                    expected_downstream_revision: downstream.revision,
                    expected_latest_decision_id: None,
                    valid_until: release.valid_until,
                },
                |id, size| data.read(id, size),
            )
            .await
            .unwrap()
            .resource;
        let offer = store
            .offer_handoff(
                actor,
                "native-strategy-offer",
                &HandoffOfferV1 {
                    schema_version: SchemaV1,
                    release_id: release.id,
                    approval_id: approval.id,
                    supersedes_handoff_id: None,
                    expires_at: release.valid_until,
                },
                |id, size| data.read(id, size),
            )
            .await
            .unwrap()
            .resource;
        let principal = store
            .create_principal(
                actor,
                "native-strategy-principal",
                &PrincipalCreate {
                    schema_version: SchemaV1,
                    name: "Native Paper test principal".into(),
                    kind: AssignablePrincipalKind::Downstream,
                    project_id: Some(data.data.project),
                    downstream_id: Some(downstream.id),
                    enabled: true,
                },
            )
            .await
            .unwrap()
            .resource;
        let store::control::CredentialPreparation::New(ticket) = store
            .prepare_credential_issuance(
                actor,
                "native-strategy-credential",
                principal.id,
                &CredentialIssue {
                    schema_version: SchemaV1,
                    scope_codes: vec![MachineScope::DownstreamClaim, MachineScope::DownstreamAck],
                    expires_at: runtime::now() + chrono::Duration::hours(1),
                },
            )
            .await
            .unwrap()
        else {
            panic!("new test principal credential")
        };
        let verifier = Id::new();
        let credential = ticket.publish(Id::new(), verifier).await.unwrap().resource;
        let machine = Actor::Machine {
            credential_id: credential.id,
            verifier_ref: verifier,
            operator_grant: None,
        };
        let request = HandoffClaimV1 {
            schema_version: SchemaV1,
            external_claim_id: "native-strategy-claim".into(),
            package_schema_version: PackageSchemaVersion::V2,
        };
        let claim = store
            .claim_handoff_envelope(
                &machine,
                "native-strategy-claim",
                offer.id,
                &request,
                |id, size| data.read(id, size),
            )
            .await
            .unwrap();
        let replay = store
            .claim_handoff_envelope(
                &machine,
                "native-strategy-claim",
                offer.id,
                &request,
                |_, _| async { panic!("claim replay uses its original stored receipt") },
            )
            .await
            .unwrap();
        assert!(!claim.replayed && replay.replayed);
        assert_eq!(
            serde_json::to_value(&claim.resource).unwrap(),
            serde_json::to_value(&replay.resource).unwrap()
        );
        let TargetPackageEnvelopeV2::TargetDecision(package) = &claim.resource.package else {
            panic!("SQL V2 target decision claim")
        };
        assert_eq!(claim.resource.handoff.state, HandoffStateV1::Claimed);
        assert_eq!(package.source.run_id, current.run_id);
        assert_eq!(
            package.source.accepted_attempt_id,
            current.accepted_attempt_id
        );
        assert_eq!(
            package.source.report_artifact_id,
            current.report_artifact_id
        );
        assert_eq!(package.source.alpha_version_ids, vec![alpha.id]);
        assert_eq!(
            package.source.input_provenance.market_data_origin,
            DataOrigin::Fixture
        );
        assert_eq!(
            package.source.input_provenance.pit_status,
            PitStatus::Unverified
        );
        domain::delivery::strategy_target_package(package, &report, &current).unwrap();
        let package_size: i64 =
            sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
                .bind(release.package_artifact_id.as_uuid())
                .fetch_one(pool)
                .await
                .unwrap();
        let package_bytes = data
            .read(release.package_artifact_id, oci::count(package_size as u64))
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&package_bytes).unwrap(),
            serde_json::to_value(package).unwrap(),
            "SQL claim preserves the original immutable release package"
        );
        assert!(package.valid_until > runtime::now());
        let counts: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs WHERE cycle_id=$1),(SELECT count(*) FROM app.run_attempts a JOIN app.runs r ON r.id=a.run_id WHERE r.cycle_id=$1),(SELECT count(*) FROM app.portfolio_candidates),(SELECT count(*) FROM app.run_missions),(SELECT count(*) FROM app.qualifications)").bind(experiment.cycle_id.as_uuid()).fetch_one(pool).await.unwrap();
        assert_eq!(counts, (4, 4, 2, 0, 0));
        if let Some(path) = std::env::var_os("QZ_NATIVE_STRATEGY_CLAIM_FILE") {
            let path = std::path::Path::new(&path);
            assert!(
                path.is_absolute(),
                "claim output must be an explicit absolute path"
            );
            // create_new prevents an old successful run's receipt being silently
            // replaced. The consumer refuses stale packages; no clock refresh exists.
            let bytes = serde_json::to_vec_pretty(
                &json!({"claim":claim.resource,"execution_assumptions":assumption}),
            )
            .unwrap();
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            file.write_all(&bytes).unwrap();
            file.sync_all().unwrap();
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        println!(
            "native strategy: original Evaluate → adopted alpha → actual historical/current OCI composition → REPORT/candidate → SQL release/approval/offer/claim; duplicate messages/claims preserved; Paper consumer execution is a separate required target"
        );
        ClaimedStrategy {
            claim: claim.resource,
            execution_assumptions: assumption,
            principal_id: principal.id,
        }
    })
}
