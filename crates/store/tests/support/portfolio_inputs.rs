//! Actual input/mandate/weights commands; controlled runtime declarations only.
use super::*;
use chrono::{Duration, Utc};
use contracts::{
    catalogs::RuntimeCatalogMetadataV1, control::*, data::DatasetRegister, forward::*,
    portfolio::*, research::*, runtime::*, settings::*,
};
use store::{
    authority::Actor, data_registration::RegistrationPreparation, runtime::ProbePreparation,
};

fn nanos(time: chrono::DateTime<Utc>) -> DbCounter {
    DbCounter::new(time.timestamp_nanos_opt().unwrap().try_into().unwrap()).unwrap()
}

pub(super) async fn forward(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
) -> Id {
    let context = &f.freeze.execution_context;
    let (artifact,size): (uuid::Uuid,i64) = sqlx::query_as("SELECT e.native_metadata_artifact_id,a.byte_count FROM app.dataset_registration_evidence e JOIN app.artifacts a ON a.id=e.native_metadata_artifact_id WHERE e.dataset_revision_id=$1")
        .bind(f.data.discovery.as_uuid()).fetch_one(pool).await.unwrap();
    let mut metadata: RuntimeCatalogMetadataV1 = serde_json::from_slice(
        &f.read(
            artifact.to_string().try_into().unwrap(),
            DbCounter::new(size as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    let now: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    metadata.native_snapshot_ref = format!("controlled-forward/{}", Id::new());
    metadata.storage_version = Id::new().to_string();
    metadata.partition = DataPartition::Forward;
    metadata.event_start = now - Duration::minutes(1000);
    metadata.event_end = now;
    metadata.available_through = now;
    metadata.quality.checked_at = now;
    let quality = &mut metadata.quality.datasets[0];
    quality.first_event_ns = nanos(metadata.event_start);
    quality.last_event_ns = nanos(now - Duration::minutes(1));
    quality.available_through_ns = nanos(now - Duration::seconds(59));
    quality.selection.event_start_ns = nanos(metadata.event_start);
    quality.selection.event_end_ns = nanos(now);
    quality.selection.decision_cutoff_ns = nanos(now);
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM app.data_sources WHERE id=$1")
        .bind(f.data.source.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    let RegistrationPreparation::Execute(ticket) = store
        .prepare_dataset_registration(
            actor,
            &Id::new().to_string(),
            &DatasetRegister {
                schema_version: SchemaV1,
                source_id: f.data.source,
                grant_id: f.data.grant,
                expected_source_revision: revision.to_string().try_into().unwrap(),
                expected_runtime_revision: context.runtime_revision,
                native_storage_version: metadata.storage_version.clone(),
                existing_universe_version_id: Some(f.data.universe),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new Forward registration");
    };
    let dataset = store
        .complete_dataset_registration(
            *ticket,
            serde_json::to_vec(&metadata).unwrap(),
            |id, size| f.read(id, size),
            |objects| {
                std::future::ready(objects.into_iter().try_for_each(|object| {
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }))
            },
        )
        .await
        .unwrap()
        .resource;

    dataset.id
}

pub(super) async fn request(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    cycle: Id,
    environment: ForwardEnvironmentV1,
    interval: Option<u32>,
) -> PortfolioBuildRequestV1 {
    let context = &f.freeze.execution_context;
    let ProbePreparation::Pending(ticket) = store
        .prepare_runtime_probe(
            actor,
            "portfolio-probe",
            f.data.runtime,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: context.runtime_revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("new probe");
    };
    store
        .complete_runtime_probe(
            *ticket,
            RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(runtime_support::portfolio_capabilities(Utc::now())),
            },
            |id, bytes| {
                std::future::ready(f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity))
            },
        )
        .await
        .unwrap();

    let now: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let dataset = forward(pool, store, actor, f).await;
    let downstream = store
        .create_downstream(
            actor,
            "portfolio-downstream",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "Controlled weights source".into(),
                    endpoint: "https://downstream.example".into(),
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: if interval.is_some() {
                        DownstreamEnvironments::Both
                    } else {
                        match environment {
                            ForwardEnvironmentV1::Paper => DownstreamEnvironments::Paper,
                            ForwardEnvironmentV1::Live => DownstreamEnvironments::Live,
                        }
                    },
                    enabled: true,
                    development_http: false,
                },
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource
        .id;
    let principal = store
        .create_principal(
            actor,
            "portfolio-downstream-principal",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "Controlled downstream identity".into(),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(f.data.project),
                downstream_id: Some(downstream),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let store::control::CredentialPreparation::New(credential) = store
        .prepare_credential_issuance(
            actor,
            "portfolio-downstream-credential",
            principal,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::ForwardSubmit],
                expires_at: now + Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new controlled credential");
    };
    let verifier = Id::new();
    let credential = credential
        .publish(Id::new(), verifier)
        .await
        .unwrap()
        .resource
        .id;
    let weights = store
        .submit_downstream_weights(
            &Actor::Machine {
                credential_id: credential,
                verifier_ref: verifier,
                operator_grant: None,
            },
            &DownstreamWeightsSubmitV1 {
                schema_version: SchemaV1,
                project_id: f.data.project,
                environment,
                external_message_id: "original-portfolio-weights".into(),
                asof_ns: nanos(now),
                available_ns: nanos(now),
                valid_until_ns: nanos(now + Duration::hours(1)),
                base_currency: "USD".into(),
                cash_weight: "0".parse().unwrap(),
                weights: vec![AllocationTargetV1 {
                    instrument_id: "EUR/USD.SIM".into(),
                    currency: "USD".into(),
                    weight: "1".parse().unwrap(),
                }],
            },
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    let cutoff = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let input = store
        .create_input_set(
            actor,
            "portfolio-forward-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                purpose: InputPurpose::Forward,
                decision_cutoff: cutoff,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset,
                    role: DataPartition::Forward,
                }],
            },
        )
        .await
        .unwrap()
        .resource
        .header
        .id;
    let assumption = store
        .execution_assumption(actor, f.data.assumptions)
        .await
        .unwrap();
    let allocation: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mut constraints = allocation.constraints;
    constraints.transaction_costs_ref = assumption.fee_schedule_artifact_id;
    constraints.liquidity_ref = assumption
        .bar_liquidity
        .as_ref()
        .map(|s| s.report_artifact_id)
        .or(assumption.rolling_liquidity_artifact_id);
    constraints.max_participation = assumption
        .bar_liquidity
        .as_ref()
        .map(|s| s.participation_limit.clone())
        .or_else(|| {
            assumption
                .rolling_liquidity
                .as_ref()
                .map(|s| s.participation_limit.clone())
        });
    constraints.group_bounds = vec![GroupBoundV1 {
        group_id: "fixture-group".into(),
        min: "0.5".parse().unwrap(),
        max: "1".parse().unwrap(),
    }];
    if assumption.bar_liquidity.is_none() {
        constraints.group_bounds.clear();
    }
    let mandate = store
        .create_mandate(
            actor,
            "original-qualified-mandate",
            &MandateCreateV1 {
                schema_version: SchemaV1,
                project_id: f.data.project,
                runtime_id: f.data.runtime,
                expected_runtime_revision: context.runtime_revision,
                content: MandateContentV1 {
                    objective: allocation.objective,
                    risk_measure: allocation.risk,
                    base_currency: allocation.base_currency,
                    capital_assumption: assumption.settings.starting_capital,
                    universe_version_id: f.data.universe,
                    covariance_estimator: allocation.covariance_estimator,
                    alpha_ensemble: allocation.alpha_ensemble,
                    optimizer: allocation.optimizer,
                    constraints,
                    rebalance_schedule: RebalanceScheduleV1 {
                        schema_version: SchemaV1,
                        kind: if interval.is_some() {
                            RebalanceKind::FixedInterval
                        } else {
                            RebalanceKind::Manual
                        },
                        interval_seconds: interval,
                        calendar_ref: None,
                        timezone: "UTC".into(),
                        session_offset_seconds: None,
                        max_input_age_seconds: 60,
                        target_ttl_seconds: 300,
                    },
                    required_evaluation_policy_id: f.brief.content.evaluation_policy_id,
                    execution_assumptions_id: f.data.assumptions,
                    exposure_tolerance: allocation.exposure_tolerance,
                },
            },
        )
        .await
        .unwrap()
        .resource;
    let qualifications:Vec<uuid::Uuid>=sqlx::query_scalar("SELECT q.id FROM app.qualifications q JOIN app.alpha_versions v ON v.id=q.alpha_version_id WHERE v.project_id=$1 ORDER BY q.id")
        .bind(f.data.project.as_uuid()).fetch_all(pool).await.unwrap();
    let mut limits = super::limits();
    limits.experiments = 0;
    PortfolioBuildRequestV1 {
        schema_version: SchemaV1,
        cycle_id: cycle,
        mandate_id: mandate.id,
        input_set_id: input,
        runtime_id: f.data.runtime,
        expected_runtime_revision: context.runtime_revision,
        current_weights_source: PortfolioBuildWeightsV1::ForwardSnapshot {
            snapshot_id: weights.id,
        },
        environment,
        members: qualifications
            .into_iter()
            .map(|id| PortfolioMemberSelectionV1 {
                qualification_id: id.to_string().try_into().unwrap(),
                ensemble_weight: "0.5".parse().unwrap(),
            })
            .collect(),
        limits,
    }
}
