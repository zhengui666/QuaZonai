//! Native Runtime HTTP only. Endpoints and socket destinations are deployment
//! authority, never a model/tool parameter or mutable browser allowlist.
use contracts::runtime::{RuntimeCapabilitiesV1, RuntimeProbeFailure};
use reqwest::{header, redirect::Policy, Client, StatusCode};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use store::lifecycle::RuntimeSnapshot;
use url::{Host, Url};

mod catalog;
mod downstream;
mod jobs;

pub use catalog::ReceivedCatalogMetadata;
pub use downstream::DownstreamTransport;
use integrations::json;
pub use jobs::{ReceivedRuntimeResult, RuntimeRequestError};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTarget {
    pub origin: String,
    pub addresses: Vec<SocketAddr>,
}

/// Parse once at startup. DNS overrides bind the original Host/SNI to approved
/// destinations; the native client never resolves a different host on redirects.
#[derive(Clone, Default)]
pub struct RuntimeTargets {
    entries: BTreeMap<String, Vec<SocketAddr>>,
    development_http: bool,
}

fn origin(value: &str, development: bool) -> Result<Url, RuntimeProbeFailure> {
    domain::settings::endpoint(value, development)
        .map_err(|_| RuntimeProbeFailure::EndpointDenied)?;
    Url::parse(value).map_err(|_| RuntimeProbeFailure::EndpointDenied)
}

fn allowed_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let bytes = ip.octets();
            !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_broadcast()
                && !ip.is_link_local()
                && bytes[0] != 0
                && bytes[0] < 224
                && bytes != [100, 100, 100, 200]
                && bytes != [168, 63, 129, 16]
        }
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            !ip.is_unspecified()
                && !ip.is_multicast()
                && !ip.is_unicast_link_local()
                && ip.to_ipv4_mapped().is_none()
                && ip.segments() != [0xfd00, 0x0ec2, 0, 0, 0, 0, 0, 0x0254]
                && first != 0x2002
                && !(first == 0x2001 && ip.segments()[1] == 0)
                && (ip.is_loopback() || (first & 0xe000 == 0x2000) || (first & 0xfe00 == 0xfc00))
        }
    }
}

impl RuntimeTargets {
    pub fn new(
        entries: Vec<RuntimeTarget>,
        development_http: bool,
    ) -> Result<Self, RuntimeProbeFailure> {
        let mut result = Self {
            entries: BTreeMap::new(),
            development_http,
        };
        for entry in entries {
            let is_http = entry.origin.starts_with("http:");
            if is_http && !development_http {
                return Err(RuntimeProbeFailure::EndpointDenied);
            }
            let url = origin(&entry.origin, is_http)?;
            let port = url
                .port_or_known_default()
                .ok_or(RuntimeProbeFailure::EndpointDenied)?;
            if entry.addresses.is_empty() {
                return Err(RuntimeProbeFailure::EndpointDenied);
            }
            for (index, address) in entry.addresses.iter().enumerate() {
                if address.port() != port
                    || !allowed_address(address.ip())
                    || entry.addresses[..index].contains(address)
                {
                    return Err(RuntimeProbeFailure::EndpointDenied);
                }
                // reqwest does not route literal IP URLs through the DNS resolver.
                // Therefore literal origins must agree with every pinned address.
                let literal = match url.host() {
                    Some(Host::Ipv4(ip)) => Some(IpAddr::V4(ip)),
                    Some(Host::Ipv6(ip)) => Some(IpAddr::V6(ip)),
                    _ => None,
                };
                if literal.is_some_and(|ip| ip != address.ip()) {
                    return Err(RuntimeProbeFailure::EndpointDenied);
                }
            }
            if result
                .entries
                .insert(url.origin().ascii_serialization(), entry.addresses)
                .is_some()
            {
                return Err(RuntimeProbeFailure::EndpointDenied);
            }
        }
        Ok(result)
    }

    fn target(
        &self,
        endpoint: &str,
        development_http: bool,
    ) -> Result<(Url, &[SocketAddr]), RuntimeProbeFailure> {
        if development_http && !self.development_http {
            return Err(RuntimeProbeFailure::EndpointDenied);
        }
        let url = origin(endpoint, development_http)?;
        let addresses = self
            .entries
            .get(&url.origin().ascii_serialization())
            .ok_or(RuntimeProbeFailure::EndpointDenied)?;
        Ok((url, addresses))
    }
}

/// Secret-bearing native Client intentionally has no Debug implementation.
pub struct RuntimeTransport {
    client: Client,
    origin: Url,
    credential: String,
}

// The two native protocols share the same restricted HTTP boundary, not task authority.
struct HttpConfiguration<'a> {
    endpoint: &'a str,
    development_http: bool,
    tls_policy: &'a str,
    ca_configured: bool,
    purpose: contracts::settings::IntegrationSecretPurpose,
}

impl RuntimeTransport {
    pub fn new(
        targets: &RuntimeTargets,
        snapshot: &RuntimeSnapshot,
        credential: &[u8],
        ca: Option<&[u8]>,
    ) -> Result<Self, RuntimeProbeFailure> {
        if snapshot.protocol_version != "1" {
            return Err(RuntimeProbeFailure::ContractUnsupported);
        }
        Self::connect(
            targets,
            HttpConfiguration {
                endpoint: &snapshot.endpoint,
                development_http: snapshot.development_http,
                tls_policy: &snapshot.tls_policy,
                ca_configured: snapshot.ca_certificate_ref.is_some(),
                purpose: contracts::settings::IntegrationSecretPurpose::Runtime,
            },
            credential,
            ca,
        )
    }

    fn connect(
        targets: &RuntimeTargets,
        configuration: HttpConfiguration<'_>,
        credential: &[u8],
        ca: Option<&[u8]>,
    ) -> Result<Self, RuntimeProbeFailure> {
        let (origin, addresses) =
            targets.target(configuration.endpoint, configuration.development_http)?;
        let credential =
            std::str::from_utf8(credential).map_err(|_| RuntimeProbeFailure::Authentication)?;
        domain::settings::secret_value(configuration.purpose, credential)
            .map_err(|_| RuntimeProbeFailure::Authentication)?;
        let mut bearer = header::HeaderValue::from_str(&format!("Bearer {credential}"))
            .map_err(|_| RuntimeProbeFailure::Authentication)?;
        bearer.set_sensitive(true);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, bearer);
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            header::ACCEPT_ENCODING,
            header::HeaderValue::from_static("identity"),
        );
        let host = origin
            .host_str()
            .ok_or(RuntimeProbeFailure::EndpointDenied)?;
        let mut builder = Client::builder()
            .use_rustls_tls()
            .https_only(!configuration.development_http)
            .resolve_to_addrs(host, addresses)
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(3))
            .pool_max_idle_per_host(1)
            .connection_verbose(false);
        match configuration.tls_policy {
            "SYSTEM_CA" if !configuration.ca_configured && ca.is_none() => {}
            "PINNED_CA" if !configuration.development_http && configuration.ca_configured => {
                let ca = ca.ok_or(RuntimeProbeFailure::TlsConfiguration)?;
                let certificates = reqwest::Certificate::from_pem_bundle(ca)
                    .map_err(|_| RuntimeProbeFailure::TlsConfiguration)?;
                if certificates.is_empty() {
                    return Err(RuntimeProbeFailure::TlsConfiguration);
                }
                builder = builder.tls_built_in_root_certs(false);
                for certificate in certificates {
                    builder = builder.add_root_certificate(certificate);
                }
            }
            _ => return Err(RuntimeProbeFailure::TlsConfiguration),
        }
        let client = builder
            .build()
            .map_err(|_| RuntimeProbeFailure::TlsConfiguration)?;
        Ok(Self {
            client,
            origin,
            credential: credential.to_owned(),
        })
    }

    pub async fn capabilities(&self) -> Result<RuntimeCapabilitiesV1, RuntimeProbeFailure> {
        let mut url = self.origin.clone();
        url.set_path("/runtime/v1/capabilities");
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| RuntimeProbeFailure::Unavailable)?;
        let (capabilities, _) = self
            .json_response(response, &[StatusCode::OK])
            .await
            .map_err(RuntimeRequestError::probe)?;
        domain::runtime::capabilities(&capabilities, chrono::Utc::now())
            .map_err(|_| RuntimeProbeFailure::ContractUnsupported)?;
        Ok(capabilities)
    }
}

#[cfg(test)]
mod capacity_tests {
    use super::*;

    fn entries() -> Vec<RuntimeTarget> {
        (0..65)
            .map(|index| RuntimeTarget {
                origin: format!("https://runtime-{index}.example"),
                addresses: (1..=17)
                    .map(|address| format!("10.0.0.{address}:443").parse().unwrap())
                    .collect(),
            })
            .collect()
    }

    #[test]
    fn complete_deployment_targets_cross_former_count_limits() {
        let input = entries();
        let targets = RuntimeTargets::new(input.clone(), false).unwrap();
        assert_eq!(targets.entries.len(), input.len());
        for entry in &input {
            let (_, addresses) = targets.target(&entry.origin, false).unwrap();
            assert_eq!(addresses, entry.addresses);
        }
        assert!(targets.target("https://not-registered.example", false).is_err());
    }

    #[test]
    fn large_target_configuration_retains_pinning_and_identity_checks() {
        let input = entries();
        let mut duplicate_origin = input.clone();
        duplicate_origin.push(input[0].clone());
        assert!(RuntimeTargets::new(duplicate_origin, false).is_err());
        for replacement in ["10.0.0.1:443", "10.0.0.18:8443", "169.254.169.254:443"] {
            let mut invalid = input.clone();
            invalid.last_mut().unwrap().addresses.push(replacement.parse().unwrap());
            assert!(RuntimeTargets::new(invalid, false).is_err());
        }
        let mut empty = input.clone();
        empty.last_mut().unwrap().addresses.clear();
        assert!(RuntimeTargets::new(empty, false).is_err());
        let mut literal_mismatch = input;
        literal_mismatch.last_mut().unwrap().origin = "https://10.0.0.1".into();
        assert!(RuntimeTargets::new(literal_mismatch, false).is_err());
    }
}
