//! Controlled protocol fixture only. Labels/IDs are structural counterexamples,
//! not a real claim, initialization consumption, credential or market evidence.
#![allow(dead_code)]
#[path = "portfolio.rs"]
mod portfolio_fixture;
use chrono::{DateTime, Utc};
use contracts::{
    catalogs::DataRevisionPolicy,
    control::CommandResult,
    delivery::*,
    forward::ForwardEnvironmentV1,
    portfolio::{AllocationInputV1, CandidateWeightsSourceV1},
    research::{DataOrigin, DataPartition, PitStatus},
    runtime::RuntimeDataKind,
    science::{PaperInitializationRefV1, PortfolioWeightsSourceV1},
    strategy_portfolio::{
        FreshPaperCashV1, HandoffClaimViewV2, TargetPackageEnvelopeV2, TargetPackageVersionV2,
    },
    DbCounter, Id, Revision, SchemaV1,
};
use serde_json::json;

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap()
}

pub fn example() -> (
    PaperInitialExecutionConsumeV1,
    HandoffClaimViewV2,
    CommandResult<PaperInitialExecutionViewV1>,
    DateTime<Utc>,
) {
    let input: AllocationInputV1 =
        serde_json::from_str(include_str!("../contracts/allocation-input.json")).unwrap();
    let mut frozen = portfolio_fixture::request(&input);
    let root = PaperInitializationRefV1 {
        artifact_id: frozen.current_weights_artifact_id,
        downstream_id: Id::new(),
        trader_id: "TRADER-001".into(),
        account_id: "SIM-001".into(),
    };
    frozen.current_weights.paper_initialization = Some(root.clone());
    frozen.current_weights.source = PortfolioWeightsSourceV1::PaperInitialCapital {
        account_start: FreshPaperCashV1 {
            downstream_id: root.downstream_id,
            trader_id: root.trader_id.clone(),
            account_id: root.account_id.clone(),
            base_currency: frozen.mandate.base_currency.clone(),
            starting_capital: frozen.mandate.capital_assumption.clone(),
            execution_assumptions_id: frozen.mandate.execution_assumptions_id,
        },
    };
    frozen.current_weights.asof_ns = DbCounter::new(90_000_000_000).unwrap();
    frozen.current_weights.available_ns = frozen.current_weights.asof_ns;
    frozen.current_weights.valid_until_ns = DbCounter::new(390_000_000_000).unwrap();
    frozen.current_weights.cash_weight = "1".parse().unwrap();
    for w in &mut frozen.current_weights.weights {
        w.weight = "0".parse().unwrap();
    }
    frozen.selection.decision_cutoff_ns = frozen.current_weights.asof_ns;
    frozen.selection.event_end_ns = frozen.current_weights.asof_ns;
    let dataset = Id::new();
    let metadata = Id::new();
    let source = ForecastEvaluationSourceV2 {
        build_run_id: Id::new(),
        build_accepted_attempt_id: Id::new(),
        build_parameters_artifact_id: Id::new(),
        build_report_artifact_id: Id::new(),
        build_input_set_id: Id::new(),
        build_environment: ForwardEnvironmentV1::Paper,
        forward_dataset_revision_id: dataset,
        forward_metadata_artifact_id: metadata,
        current_weights_artifact_id: root.artifact_id,
    };
    let provenance = vec![
        source.build_parameters_artifact_id,
        source.build_report_artifact_id,
        root.artifact_id,
        metadata,
    ];
    let package=ForecastTargetPackageV2{release_id:Id::new(),package_schema_version:TargetPackageVersionV2::V2,source_kind:ForecastReleaseSourceV2::ForecastEvaluation,source,
        forward_dataset:FrozenForwardDatasetV2 {dataset_revision_id:dataset,native_metadata_artifact_id:metadata,storage_version:"controlled".into(),data_kind:RuntimeDataKind::Bar,partition:DataPartition::Forward,origin:DataOrigin::Real,pit_status:PitStatus::Verified,revision_policy:DataRevisionPolicy::AsKnownThen,event_start:at(0),event_end:at(90),available_through:at(90),row_count:DbCounter::new(1).unwrap(),selection:frozen.selection.clone(),instrument_definitions:input.assets.iter().map(|a|json!({"CurrencyPair":{"id":a.instrument_id,"ts_event":0,"ts_init":0,"price_increment":"0.01","fixture_only":true}})).collect()},
        environment_origin:PackageOriginV1::Synthetic,project_id:Id::new(),candidate_id:Id::new(),mandate_id:Id::new(),qualification_refs:vec![Id::new(),Id::new()],evaluation_refs:vec![Id::new()],input_revision_refs:vec![dataset],engine_versions:Default::default(),asof:at(90),valid_from:at(95),valid_until:at(390),base_currency:frozen.mandate.base_currency.clone(),capital_assumption:frozen.mandate.capital_assumption.clone(),current_weights_source:CandidateWeightsSourceV1::PaperInitialCapital,current_weights:frozen.current_weights,execution_settings:frozen.execution_settings,
        targets:input.assets.iter().map(|a|PackageTargetV1{instrument_id:a.instrument_id.clone(),target_weight:a.current_weight.clone(),currency:a.currency.clone()}).collect(),cash_weight:input.current_cash_weight,constraints_summary:frozen.mandate.constraints,exposure_tolerance:frozen.mandate.exposure_tolerance,cost_assumption_ref:frozen.mandate.execution_assumptions_id,compatible_market_capabilities:vec!["controlled/1".into()],limitations:vec!["Controlled structural fixture only".into()],provenance_artifact_refs:provenance};
    let claim = HandoffClaimViewV2 {
        handoff: HandoffViewV1 {
            id: Id::new(),
            project_id: package.project_id,
            candidate_id: package.candidate_id,
            mandate_id: package.mandate_id,
            release_id: package.release_id,
            approval_id: Id::new(),
            downstream_id: root.downstream_id,
            environment: ForwardEnvironmentV1::Paper,
            delivery_sequence: DbCounter::new(1).unwrap(),
            revision: Revision::INITIAL,
            state: HandoffStateV1::Claimed,
            supersedes_handoff_id: None,
            offered_at: at(95),
            expires_at: at(390),
            claimed_at: Some(at(98)),
            external_claim_id: Some("controlled-original-claim".into()),
            acknowledged_at: None,
        },
        package: TargetPackageEnvelopeV2::Forecast(Box::new(package)),
    };
    let request = PaperInitialExecutionConsumeV1 {
        schema_version: SchemaV1,
        paper_initialization: root.clone(),
        release_id: claim.handoff.release_id,
        external_claim_id: claim.handoff.external_claim_id.clone().unwrap(),
        owner_instance_id: Id::new(),
    };
    let result = CommandResult {
        schema_version: SchemaV1,
        replayed: false,
        resource: PaperInitialExecutionViewV1 {
            schema_version: SchemaV1,
            state: PaperInitialExecutionStateV1::Consumed,
            paper_initialization: root,
            package_artifact_id: Id::new(),
            owner_instance_id: request.owner_instance_id,
            consuming_credential_id: Id::new(),
            consumed_at: at(99),
            claim: Box::new(claim.clone()),
        },
    };
    (request, claim, result, at(100))
}
