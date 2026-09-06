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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::{Id, SchemaV1};

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
