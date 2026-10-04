//! Small, shared control-plane invariants; persistence owns authorization/CAS.
use crate::DomainError;
use contracts::control::*;
use std::collections::BTreeSet;

pub fn name(value: &str) -> Result<(), DomainError> {
    text(value, 1, 120, false)
}
pub fn text(value: &str, min: usize, max: usize, multiline: bool) -> Result<(), DomainError> {
    let count = value.chars().count();
    if count < min
        || count > max
        || (min > 0 && value.trim().is_empty())
        || value
            .chars()
            .any(|c| c.is_control() && !(multiline && matches!(c, '\n' | '\t' | '\r')))
    {
        return Err(DomainError::Invalid("text"));
    }
    Ok(())
}
pub fn list(request: &ListQuery) -> Result<(), DomainError> {
    if !(1..=100).contains(&request.limit) {
        return Err(DomainError::Invalid("limit"));
    }
    Ok(())
}
pub fn principal(request: &PrincipalCreate) -> Result<(), DomainError> {
    name(&request.name)?;
    if (request.kind == AssignablePrincipalKind::Downstream) != request.downstream_id.is_some()
        || (request.kind == AssignablePrincipalKind::Downstream && request.project_id.is_none())
    {
        return Err(DomainError::Invalid("principal_identity"));
    }
    Ok(())
}
pub fn scopes(request: &CredentialIssue) -> Result<(), DomainError> {
    if request.scope_codes.is_empty()
        || request.scope_codes.len() > 10
        || request.scope_codes.iter().collect::<BTreeSet<_>>().len() != request.scope_codes.len()
        || (request.scope_codes.contains(&MachineScope::DoctorRead)
            && request.scope_codes.len() != 1)
    {
        return Err(DomainError::Invalid("scope_codes"));
    }
    Ok(())
}
pub fn command(request: &OperatorCommand) -> Result<(), DomainError> {
    match request {
        OperatorCommand::CodexProfileUpdate(r) => crate::codex::settings::profile_update(r),
        OperatorCommand::CodexProbe(_)
        | OperatorCommand::CodexLoginStart(_)
        | OperatorCommand::CodexLoginCancel(_)
        | OperatorCommand::CodexLogout(_) => Ok(()),
        OperatorCommand::BriefFreeze(_) => Ok(()),
        OperatorCommand::DataSourceCreate(r) => crate::data::source_create(r),
        OperatorCommand::DataSourceUpdate(r) => crate::data::source_update(r),
        OperatorCommand::DataGrantCreate(r) => crate::data::grant_create(r),
        OperatorCommand::DataGrantRevoke(r) => crate::data::grant_revoke(r),
        OperatorCommand::DatasetRegister(r) => crate::data::dataset_register(r),
        OperatorCommand::RecordedFeatureRegister(r) => crate::data::recorded_feature_intent(r),
        OperatorCommand::DataValidate(r) => crate::data::validate_request(r),
        OperatorCommand::AlphaEvaluate(r) => crate::data::bounded_native_limits(&r.limits),
        OperatorCommand::ExperimentEvaluate(r) => crate::experiments::evaluate(&r.request),
        OperatorCommand::ExperimentAdoptAlpha(r) => crate::experiments::adopt_alpha(&r.request),
        OperatorCommand::PortfolioBuild(r) => match r.as_ref() {
            contracts::strategy_portfolio::PortfolioBuildEnvelopeV2::Forecast(r) => {
                crate::portfolio::build_selection(r)
            }
            contracts::strategy_portfolio::PortfolioBuildEnvelopeV2::Strategy(r) => {
                crate::execution::strategy::build(r)
            }
        },
        OperatorCommand::PortfolioSimulate(r) => crate::data::bounded_native_limits(&r.limits),
        OperatorCommand::PortfolioStudy(r) => crate::data::bounded_native_limits(&r.limits),
        OperatorCommand::PolicyAuthorize(v) => crate::delivery::automation_policy(&v.content),
        OperatorCommand::PolicyRevoke(v) => text(&v.reason, 1, 2000, true),
        OperatorCommand::ApprovalRevoke(v) => {
            crate::delivery::decision_reason(&v.reason_code, &v.reason)
        }
        OperatorCommand::ReleaseCreate(_)
        | OperatorCommand::ReleaseApprove(_)
        | OperatorCommand::HandoffOffer(_) => Ok(()),
        OperatorCommand::ReleaseReject(r) => {
            crate::delivery::decision_reason(&r.reason_code, &r.reason)
        }
        OperatorCommand::ReleaseReopen(r) => {
            crate::delivery::decision_reason(&r.reason_code, &r.reason)
        }
        OperatorCommand::CycleStart(_)
        | OperatorCommand::CycleStartExternal(_)
        | OperatorCommand::CycleFinishExternal(_)
        | OperatorCommand::MigrationImport(_) => Ok(()),
        OperatorCommand::IntegrationSecretRegister(r) => crate::settings::secret_intent(r),
        OperatorCommand::RuntimeProbe(_) | OperatorCommand::DownstreamProbe(_) => Ok(()),
        OperatorCommand::RuntimeCreate(r) => crate::settings::runtime_create(r),
        OperatorCommand::RuntimeUpdate(r) => crate::settings::runtime_update(r),
        OperatorCommand::DownstreamCreate(r) => {
            crate::settings::downstream_configuration(&r.configuration)
        }
        OperatorCommand::DownstreamUpdate(r) => {
            crate::settings::downstream_configuration(&r.configuration)
        }
        OperatorCommand::BriefCreate(r) => {
            crate::brief::content(&r.request.content, &r.request.bindings)
        }
        OperatorCommand::MandateCreate(r) => match r.as_ref() {
            contracts::strategy_portfolio::MandateCreateEnvelopeV2::Forecast(r) => {
                crate::portfolio::mandate(&r.content)
            }
            contracts::strategy_portfolio::MandateCreateEnvelopeV2::Strategy(r) => {
                crate::execution::strategy::mandate(&r.content)
            }
        },
        OperatorCommand::ExecutionAssumptionsCreate(r) => {
            text(&r.settlement_rule_ref, 1, 200, false)?;
            crate::portfolio::simulation_settings(&r.settings)
        }
        OperatorCommand::BriefUpdate(r) => crate::brief::content(&r.content, &r.bindings),
        OperatorCommand::ProjectCreate(r) => {
            name(&r.name)?;
            text(&r.description, 0, 8000, true)
        }
        OperatorCommand::ProjectUpdate(r) => {
            name(&r.name)?;
            text(&r.description, 0, 8000, true)
        }
        OperatorCommand::PrincipalCreate(r) => principal(r),
        OperatorCommand::PrincipalUpdate(r) => name(&r.name),
        OperatorCommand::CredentialIssue(r) => scopes(&r.request),
        OperatorCommand::CredentialRevoke(r) => text(&r.reason, 1, 2000, true),
        OperatorCommand::InputSetCreate(r) => crate::research::input_set(r),
        OperatorCommand::EvaluationPolicyCreate(r) => crate::research::evaluation_policy(r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::{Id, SchemaV1};

    #[test]
    fn strategy_build_grant_validates_the_original_one_member_command() {
        let mut body = serde_json::json!({
            "operation":"PORTFOLIO_BUILD",
            "request": {
                "schema_version":1,"source_kind":"STRATEGY_ALPHA","cycle_id":Id::new(),
                "mandate_id":Id::new(),"input_set_id":Id::new(),"runtime_id":Id::new(),
                "expected_runtime_revision":"1","members":[{"alpha_version_id":Id::new(),"ensemble_weight":"1"}],
                "purpose":{"purpose":"HISTORICAL_REPLAY"},
                "limits":{"schema_version":1,"experiments":0,"cpu_seconds":"10","wall_seconds":60,"memory_mib":512,"output_bytes":"65536"}
            }
        });
        let valid: OperatorCommand = serde_json::from_value(body.clone()).unwrap();
        assert!(command(&valid).is_ok());
        assert_eq!(valid.normalized_request().unwrap(), body["request"]);
        body["request"]["members"][0]["ensemble_weight"] = "0.5".into();
        let invalid: OperatorCommand = serde_json::from_value(body).unwrap();
        assert!(command(&invalid).is_err());
    }

    #[test]
    fn downstream_requires_exact_project_and_downstream_bindings() {
        let mut request = PrincipalCreate {
            schema_version: SchemaV1,
            name: "delivery".into(),
            kind: AssignablePrincipalKind::Downstream,
            project_id: None,
            downstream_id: Some(Id::new()),
            enabled: true,
        };
        assert!(principal(&request).is_err());
        request.project_id = Some(Id::new());
        assert!(principal(&request).is_ok());
        request.downstream_id = None;
        assert!(principal(&request).is_err());
        for kind in [
            AssignablePrincipalKind::Cli,
            AssignablePrincipalKind::Automation,
        ] {
            request.kind = kind;
            request.project_id = None;
            assert!(
                principal(&request).is_ok(),
                "projectless doctor remains valid"
            );
            request.downstream_id = Some(Id::new());
            assert!(
                principal(&request).is_err(),
                "non-delivery identity cannot bind downstream"
            );
            request.downstream_id = None;
        }
    }
}
