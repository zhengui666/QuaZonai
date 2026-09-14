//! Target-only downstream protocol. No Runtime job methods are exposed.
use super::{HttpConfiguration, RuntimeRequestError, RuntimeTargets, RuntimeTransport};
use contracts::{
    delivery::DownstreamCapabilitiesV1, runtime::RuntimeProbeFailure,
    settings::IntegrationSecretPurpose,
};
use reqwest::StatusCode;

pub struct DownstreamTransport {
    http: RuntimeTransport,
}

impl DownstreamTransport {
    /// The trusted caller resolves a DOWNSTREAM vault reference. This constructor
    /// cannot read secrets or accept arbitrary credentials from an Agent tool.
    pub fn new(
        targets: &RuntimeTargets,
        endpoint: &str,
        development_http: bool,
        credential: &[u8],
    ) -> Result<Self, RuntimeProbeFailure> {
        Ok(Self {
            http: RuntimeTransport::connect(
                targets,
                HttpConfiguration {
                    endpoint,
                    development_http,
                    tls_policy: "SYSTEM_CA",
                    ca_configured: false,
                    purpose: IntegrationSecretPurpose::Downstream,
                },
                credential,
                None,
            )?,
        })
    }

    pub async fn capabilities(&self) -> Result<DownstreamCapabilitiesV1, RuntimeProbeFailure> {
        let started_at = chrono::Utc::now();
        let mut url = self.http.origin.clone();
        url.set_path("/downstream/v1/capabilities");
        let response = self
            .http
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| RuntimeProbeFailure::Unavailable)?;
        let (capabilities, _) = self
            .http
            .json_response(response, &[StatusCode::OK], 64 * 1024)
            .await
            .map_err(RuntimeRequestError::probe)?;
        domain::delivery::downstream_capabilities(&capabilities, chrono::Utc::now())
            .map_err(|_| RuntimeProbeFailure::ContractUnsupported)?;
        // Classify stale native replies as failed observations before Store
        // publication, so a previous successful probe cannot mask this failure.
        if capabilities.checked_at < started_at - chrono::Duration::seconds(5) {
            return Err(RuntimeProbeFailure::ContractUnsupported);
        }
        Ok(capabilities)
    }
}
