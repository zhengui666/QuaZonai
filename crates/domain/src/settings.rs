//! Pure configuration rules. A saved configuration is not a native capability probe.
use crate::{research::invalid, DomainError};
use contracts::settings::*;
use url::{Host, Url};

pub fn secret_intent(request: &IntegrationSecretIntent) -> Result<(), DomainError> {
    if crate::control::name(&request.label).is_err() {
        return Err(invalid("label", "INVALID_LABEL"));
    }
    Ok(())
}

pub fn secret_value(purpose: IntegrationSecretPurpose, value: &str) -> Result<(), DomainError> {
    let valid = match purpose {
        IntegrationSecretPurpose::TlsCa => {
            !value.is_empty() && value.len() <= 65536 && value.is_ascii()
        }
        _ => {
            !value.is_empty()
                && value.len() <= 8192
                && value.bytes().all(|byte| (b'!'..=b'~').contains(&byte))
        }
    };
    if !valid {
        return Err(invalid("value", "INVALID_SECRET_MATERIAL"));
    }
    Ok(())
}

/// Reuse the native URL parser. Deployment allowlisting and TLS/IP binding are
/// additional transport checks; this parser never resolves or contacts a host.
pub fn endpoint(value: &str, development_http: bool) -> Result<(), DomainError> {
    let bad = || invalid("configuration.endpoint", "INVALID_INTEGRATION_ORIGIN");
    if value.is_empty()
        || value.len() > 2048
        || value.trim() != value
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return Err(bad());
    }
    let parsed = Url::parse(value).map_err(|_| bad())?;
    if parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return Err(bad());
    }
    let loopback = match parsed.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    match parsed.scheme() {
        "https" if !development_http => Ok(()),
        "http" if development_http && loopback => Ok(()),
        _ => Err(bad()),
    }
}

pub fn runtime_configuration(request: &RuntimeConfigurationV1) -> Result<(), DomainError> {
    if crate::control::name(&request.name).is_err() {
        return Err(invalid("configuration.name", "INVALID_NAME"));
    }
    endpoint(&request.endpoint, request.development_http)?;
    if request.allowed_capabilities.is_empty()
        || request.allowed_capabilities.len() > 8
        || request
            .allowed_capabilities
            .iter()
            .enumerate()
            .any(|(index, kind)| request.allowed_capabilities[..index].contains(kind))
    {
        return Err(invalid(
            "configuration.allowed_capabilities",
            "INVALID_OR_DUPLICATE_CAPABILITY",
        ));
    }
    if request.development_http && request.tls_policy != TlsPolicy::SystemCa {
        return Err(invalid(
            "configuration.tls_policy",
            "TLS_NOT_USED_FOR_DEVELOPMENT_HTTP",
        ));
    }
    Ok(())
}

pub fn runtime_create(request: &RuntimeCreate) -> Result<(), DomainError> {
    runtime_configuration(&request.configuration)?;
    if (request.configuration.tls_policy == TlsPolicy::PinnedCa)
        != request.ca_certificate_ref.is_some()
    {
        return Err(invalid("ca_certificate_ref", "CA_POLICY_MISMATCH"));
    }
    Ok(())
}

pub fn runtime_update(request: &RuntimeUpdate) -> Result<(), DomainError> {
    runtime_configuration(&request.configuration)?;
    if request.configuration.tls_policy == TlsPolicy::SystemCa
        && request.ca_certificate_ref.is_some()
    {
        return Err(invalid("ca_certificate_ref", "CA_POLICY_MISMATCH"));
    }
    // A transition to PINNED_CA also requires the stored or replacement CA; the
    // transaction checks that after locking the exact current configuration.
    Ok(())
}

pub fn downstream_configuration(request: &DownstreamConfigurationV1) -> Result<(), DomainError> {
    if crate::control::name(&request.name).is_err() {
        return Err(invalid("configuration.name", "INVALID_NAME"));
    }
    endpoint(&request.endpoint, request.development_http)?;
    if request.accepted_package_versions != [PackageSchemaVersion::V1] {
        return Err(invalid(
            "configuration.accepted_package_versions",
            "UNSUPPORTED_OR_DUPLICATE_PACKAGE_VERSION",
        ));
    }
    Ok(())
}
