//! Test-only relational/HTTP seam. Fabricated native reports here are not a Sandbox/E2E pass.
#![allow(dead_code)]
#[path = "../../../../tests/support/research.rs"]
mod research;
use contracts::{
    DbCounter, Id, SchemaV1, account_observation::*, control::*, forward::ForwardEnvironmentV1,
    settings::*,
};
use sqlx::PgPool;
use store::{Store, authority::Actor};

pub async fn account_setup(pool: &PgPool) -> (Store, Actor, Actor, AccountObservationSubmitV1) {
    account_setup_named(pool, "capital-fixture").await
}
pub async fn account_setup_named(
    pool: &PgPool,
    name: &str,
) -> (Store, Actor, Actor, AccountObservationSubmitV1) {
    let (store, operator) = research::operator(pool).await;
    let project = store
        .create_project(
            &operator,
            &format!("{name}-project"),
            &ProjectCreate {
                schema_version: SchemaV1,
                name: format!("{name} Paper downstream weights"),
                description: "Controlled observations, not qualification".into(),
                fork_from_project_id: None,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let downstream = store
        .create_downstream(
            &operator,
            &format!("{name}-downstream"),
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: format!("{name} Paper fixture"),
                    endpoint: "https://downstream.example".into(),
                    accepted_package_versions: vec![PackageSchemaVersion::V2],
                    environments: DownstreamEnvironments::Paper,
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
            &operator,
            &format!("{name}-principal"),
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: format!("{name} bound downstream"),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(project),
                downstream_id: Some(downstream),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let verifier = Id::new();
    let store::control::CredentialPreparation::New(prepared) = store
        .prepare_credential_issuance(
            &operator,
            &format!("{name}-credential"),
            principal,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::ForwardSubmit],
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new fixture credential");
    };
    let credential = prepared
        .publish(Id::new(), verifier)
        .await
        .unwrap()
        .resource
        .id;
    let actor = Actor::Machine {
        credential_id: credential,
        verifier_ref: verifier,
        operator_grant: None,
    };
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let nanos = now.timestamp_nanos_opt().unwrap() as u64;
    let mut request: AccountObservationSubmitV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/native-account-paper-snapshot.json"
    ))
    .unwrap();
    request.binding.project_id = project;
    request.observed_at_ns = DbCounter::new(nanos - 10).unwrap();
    let snapshot = request.snapshot.as_mut().unwrap();
    snapshot.ts_event = DbCounter::new(nanos - 30).unwrap();
    snapshot.ts_init = DbCounter::new(nanos - 20).unwrap();
    request.snapshot.as_mut().unwrap().unrealized_pnls.clear();
    request
        .snapshot
        .as_mut()
        .unwrap()
        .total_equity
        .retain(|v| v.currency == "USD");
    (store, operator, actor, request)
}

use contracts::{Revision, account_observation::AccountMoneyV1, capital_exit::*};
pub struct Fixture {
    pub store: Store,
    pub operator: Actor,
    pub actor: Actor,
    pub observation: AccountObservationSubmitV1,
    pub source: Id,
    pub observation_id: Id,
    pub registration: store::capital_exit::CapitalExitOwnerRegistration,
}
pub fn money(amount: &str) -> AccountMoneyV1 {
    AccountMoneyV1 {
        amount: amount.parse().unwrap(),
        currency: "USD".into(),
    }
}
pub async fn setup(pool: &PgPool, register: bool) -> Fixture {
    setup_environment(pool, register, ForwardEnvironmentV1::Paper).await
}
pub async fn setup_environment(
    pool: &PgPool,
    register: bool,
    environment: ForwardEnvironmentV1,
) -> Fixture {
    let (store, operator, actor, mut observation) = account_setup(pool).await;
    if environment == ForwardEnvironmentV1::Live {
        let downstream = store
            .machine_session(&actor)
            .await
            .unwrap()
            .downstream_id
            .unwrap();
        // Synthetic relational permission scenario, not a real Live account connection.
        sqlx::query("UPDATE app.downstream_integrations SET environments='BOTH' WHERE id=$1")
            .bind(downstream.as_uuid())
            .execute(pool)
            .await
            .unwrap();
        observation.binding.environment = environment;
    }
    let receipt = store
        .submit_client_account_observation(
            &actor,
            &AccountObservationSubmitV2 {
                schema_version: NativeClientObservationSchemaV2,
                native_client_id: "CAPITAL-TEST-CLIENT".into(),
                observation: observation.clone(),
            },
        )
        .await
        .unwrap();
    let mut registration = store::capital_exit::CapitalExitOwnerRegistration {
        managed_account_key: format!("paper-native:{}", receipt.resource.source_id),
        owner_binding_ref: format!("nautilus-paper:{}", receipt.resource.source_id),
        downstream_id: receipt.resource.downstream_id,
        environment,
        venue: "SIM".into(),
        native_account_id: observation.binding.native_account_id.clone(),
        native_trader_id: observation.binding.native_trader_id.clone(),
        native_client_id: "CAPITAL-TEST-CLIENT".into(),
        collateral_currency: "USD".into(),
        instrument_id: "TEST.SIM".into(),
        controlled_strategy_ids: vec!["CAPITAL-001".into()],
    };
    if register {
        if environment == ForwardEnvironmentV1::Paper {
            registration = store
                .register_paper_capital_exit_owner(
                    &actor,
                    receipt.resource.source_id,
                    &observation.binding,
                    &registration,
                )
                .await
                .unwrap();
        } else {
            store
                .register_capital_exit_owner(&registration)
                .await
                .unwrap();
        }
    }
    Fixture {
        store,
        operator,
        actor,
        observation,
        source: receipt.resource.source_id,
        observation_id: receipt.resource.id,
        registration,
    }
}
impl Fixture {
    pub fn request(&self) -> CapitalExitPreviewRequestV1 {
        CapitalExitPreviewRequestV1 {
            schema_version: SchemaV1,
            account_source_id: self.source,
            expected_source_observation_id: self.observation_id,
            scope: CapitalExitScopeV1::Amount {
                amount: "100".parse().unwrap(),
                currency: "USD".into(),
            },
            policy: CapitalExitPolicyV1::CashOnly {},
        }
    }
    pub fn assessment(&self) -> CapitalExitOwnerAssessmentV1 {
        let now = chrono::Utc::now();
        CapitalExitOwnerAssessmentV1 {
            schema_version: SchemaV1,
            account_source_id: self.source,
            owner_binding_ref: self.registration.owner_binding_ref.clone(),
            expected_account_control_revision: Revision::INITIAL,
            external_message_id: "assessment-1".into(),
            sequence: DbCounter::new(1).unwrap(),
            request: self.request(),
            asof: now,
            valid_until: now + chrono::Duration::seconds(60),
            funds: CapitalExitPreviewFundsV1 {
                requested: money("100"),
                native_total_cash: Some(money("1000.25")),
                native_free_cash: Some(money("1000.25")),
                native_locked_cash: Some(money("0")),
                verified_idle_cash: Some(money("1000.25")),
                estimated_release: Some(money("100")),
                native_equity: Some(money("1000.25")),
                managed_capital_before: Some(money("1000.25")),
                remaining_managed_capital: Some(money("900.25")),
                estimated_execution_cost: None,
                existing_unrealized_pnl: None,
            },
            proposed_cancellations: vec![],
            retained_protective_orders: vec![],
            reduction_legs: vec![],
            evidence_refs: vec![self.observation_id],
            remaining_risk_evidence_id: None,
            native_report_ref: "controlled-fixture-only".into(),
            native_evidence: serde_json::json!({"fixture":"intake-boundary-only"}),
            capability: CapitalExitCapabilityV1::Supported,
            reason_codes: vec![],
        }
    }
    pub async fn preview(&self, key: &str) -> CapitalExitPreviewV1 {
        self.store
            .preview_capital_exit(
                &self.operator,
                self.observation.binding.project_id,
                key,
                &self.request(),
                |_| async { Ok(()) },
            )
            .await
            .unwrap()
            .resource
    }
    pub async fn start(&self) -> CapitalExitViewV1 {
        let assessment = self.assessment();
        self.store
            .submit_capital_exit_assessment(
                &self.actor,
                &assessment.external_message_id,
                &assessment,
                |_| async { Ok(()) },
            )
            .await
            .unwrap();
        let preview = self.preview("preview").await;
        self.store
            .start_capital_exit(
                &self.operator,
                self.observation.binding.project_id,
                "start",
                &CapitalExitStartV1 {
                    schema_version: SchemaV1,
                    preview_id: preview.id,
                    expected_account_control_revision: preview
                        .expected_account_control_revision
                        .unwrap(),
                    acknowledged_plan_artifact_id: preview.plan_artifact_id,
                    expected_source_observation_id: self.observation_id,
                },
            )
            .await
            .unwrap()
            .resource
    }
    pub async fn claim(&self, view: &CapitalExitViewV1, key: &str) -> CapitalExitViewV1 {
        self.store
            .claim_capital_exit(
                &self.actor,
                view.id,
                key,
                &CapitalExitClaimV1 {
                    schema_version: SchemaV1,
                    expected_revision: view.revision,
                    command_id: view.command_id,
                    account_control_epoch: view.account_control_epoch,
                    account_source_id: self.source,
                    owner_binding_ref: view.owner_binding_ref.clone(),
                    external_claim_id: key.into(),
                },
            )
            .await
            .unwrap()
            .resource
    }
    pub fn evidence(
        &self,
        view: &CapitalExitViewV1,
        message: &str,
        sequence: u64,
        evidence: CapitalExitEvidenceKindV1,
    ) -> CapitalExitOwnerEvidenceV1 {
        let now = chrono::Utc::now();
        CapitalExitOwnerEvidenceV1 {
            schema_version: SchemaV1,
            intent_id: view.id,
            command_id: view.command_id,
            account_source_id: self.source,
            owner_binding_ref: view.owner_binding_ref.clone(),
            native_session_id: self.observation.binding.native_session_id.clone(),
            native_account_id: self.observation.binding.native_account_id.clone(),
            account_control_epoch: view.account_control_epoch,
            external_claim_id: view.external_claim_id.clone().unwrap(),
            external_message_id: message.into(),
            sequence: DbCounter::new(sequence).unwrap(),
            source_observation_id: self.observation_id,
            asof: now,
            valid_until: now + chrono::Duration::seconds(60),
            original_event_refs: vec!["fixture-native-event".into()],
            native_evidence: serde_json::json!({"fixture":"boundary-only-not-native-acceptance"}),
            evidence,
        }
    }
    pub async fn fence(&self, view: &CapitalExitViewV1, sequence: u64) -> CapitalExitViewV1 {
        let message = format!("fence-{sequence}");
        let evidence = self.evidence(
            view,
            &message,
            sequence,
            CapitalExitEvidenceKindV1::FenceApplied {
                native_gate_report_ref: "fixture-all-callback-gates".into(),
                controlled_strategy_ids: self.registration.controlled_strategy_ids.clone(),
                invalidated_target_claim_refs: vec!["old-claimed-target".into()],
                invalidated_child_timer_refs: vec!["old-timer".into()],
                resolved_inflight_report_ref: "fixture-native-cache-report".into(),
                remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
                remaining_target_authority_ref: None,
            },
        );
        self.store
            .submit_capital_exit_evidence(&self.actor, view.id, &message, &evidence, |_| async {
                Ok(())
            })
            .await
            .unwrap()
            .resource
    }
}
