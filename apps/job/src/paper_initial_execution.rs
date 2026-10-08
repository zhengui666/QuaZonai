//! One authenticated server-side consume for one foreground Paper owner.
//! Files and old command receipts are diagnostics, never launch capabilities.
//! The shared QZ transport supplies redirect/retry/proxy and response safeguards.
use anyhow::{Result, anyhow, ensure};
use contracts::Id;
use reqwest::{Client, Url};
use serde::Deserialize;
use std::{fs::File, io::Read, path::PathBuf, time::Duration};

#[path = "../../server/src/service_http.rs"]
pub(crate) mod service_http;

/// References the existing scoped QZ machine connection. This is not the
/// loopback service bearer, and it neither creates nor saves a credential.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InitialExecutionAuthorityConfig {
    pub origin: String,
    pub credential_file: PathBuf,
    #[serde(default)]
    pub ca_certificate: Option<PathBuf>,
}

/// Neither Clone nor Deserialize. The owner identity lives only in this process.
/// A timeout consumes this local attempt, even if no response was received.
pub(crate) struct InitialExecutionAuthority {
    client: Client,
    origin: Url,
    credential: String,
    owner_instance_id: Id,
    attempted: bool,
}

fn connection_origin(value: &str) -> Result<Url> {
    let origin =
        Url::parse(value).map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CONNECTION_INVALID"))?;
    let loopback = origin.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    ensure!(
        value.trim() == value
            && !value.chars().any(char::is_control)
            && matches!(origin.scheme(), "http" | "https")
            && origin.host().is_some()
            && (origin.scheme() == "https" || loopback)
            && origin.username().is_empty()
            && origin.password().is_none()
            && origin.path() == "/"
            && origin.query().is_none()
            && origin.fragment().is_none(),
        "PAPER_INITIAL_EXECUTION_CONNECTION_INVALID"
    );
    Ok(origin)
}

impl InitialExecutionAuthority {
    /// Reuse this process's existing scoped machine connection. This does not
    /// consume initialization, create a token, persist it, or change its scope.
    pub(crate) fn capital_exit_transport(
        &self,
    ) -> crate::capital_exit_transport::CapitalExitOwnerTransport {
        crate::capital_exit_transport::CapitalExitOwnerTransport::from_existing(
            self.client.clone(),
            self.origin.clone(),
            self.credential.clone(),
        )
    }

    pub(crate) fn open(config: &InitialExecutionAuthorityConfig) -> Result<Self> {
        let origin = connection_origin(&config.origin)?;
        ensure!(
            config.credential_file.is_absolute(),
            "PAPER_INITIAL_EXECUTION_CREDENTIAL_REQUIRED"
        );
        let metadata = std::fs::symlink_metadata(&config.credential_file)
            .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CREDENTIAL_REQUIRED"))?;
        ensure!(
            metadata.is_file() && metadata.len() <= 256,
            "PAPER_INITIAL_EXECUTION_CREDENTIAL_REQUIRED"
        );
        let credential = String::from_utf8(
            crate::paper_node::read_credential(&config.credential_file)
                .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CREDENTIAL_REQUIRED"))?,
        )
        .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CREDENTIAL_REQUIRED"))?;
        integrations::authentication::machine_token(&credential)
            .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_MACHINE_CREDENTIAL_REQUIRED"))?;
        let mut headers = service_http::bearer(&credential)
            .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CONNECTION_INVALID"))?;
        headers.insert(
            "x-quazonai-cli",
            reqwest::header::HeaderValue::from_static("1"),
        );
        if origin.scheme() == "http" {
            headers.insert(
                reqwest::header::ORIGIN,
                reqwest::header::HeaderValue::from_str(&origin.origin().ascii_serialization())
                    .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CONNECTION_INVALID"))?,
            );
        }
        let mut builder = service_http::builder(headers, Duration::from_secs(10));
        if let Some(path) = &config.ca_certificate {
            ensure!(
                path.is_absolute() && origin.scheme() == "https",
                "PAPER_INITIAL_EXECUTION_CA_INVALID"
            );
            let file =
                File::open(path).map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CA_INVALID"))?;
            let metadata = file
                .metadata()
                .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CA_INVALID"))?;
            ensure!(
                metadata.is_file() && metadata.len() <= 65_536,
                "PAPER_INITIAL_EXECUTION_CA_INVALID"
            );
            let mut bytes = Vec::new();
            file.take(65_537)
                .read_to_end(&mut bytes)
                .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CA_INVALID"))?;
            let certificates = reqwest::Certificate::from_pem_bundle(&bytes)
                .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CA_INVALID"))?;
            ensure!(
                !certificates.is_empty() && bytes.len() <= 65_536,
                "PAPER_INITIAL_EXECUTION_CA_INVALID"
            );
            builder = builder.tls_built_in_root_certs(false);
            for certificate in certificates {
                builder = builder.add_root_certificate(certificate);
            }
        }
        Ok(Self {
            client: builder
                .build()
                .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_CONNECTION_INVALID"))?,
            origin,
            credential,
            owner_instance_id: Id::new(),
            attempted: false,
        })
    }
}

/// A one-use in-memory capability minted only after this process's live HTTP
/// consume response. No public constructor, Clone, or Deserialize is provided.
pub(crate) struct InitialCapitalPermit {
    receipt: contracts::delivery::PaperInitialExecutionViewV1,
}

impl InitialCapitalPermit {
    /// Moves the permit into its one native construction. Diagnostic evidence
    /// returned by this function cannot be used to mint another permit.
    pub(crate) fn into_canonical(
        self,
        expected: &contracts::strategy_portfolio::HandoffClaimViewV2,
    ) -> Result<(
        Box<contracts::strategy_portfolio::HandoffClaimViewV2>,
        serde_json::Value,
    )> {
        ensure!(
            serde_json::to_value(&self.receipt.claim)? == serde_json::to_value(expected)?,
            "PAPER_INITIAL_EXECUTION_CANONICAL_CLAIM_MISMATCH"
        );
        // A permit may have waited for its owning thread. Recheck the original
        // execution deadline immediately before construction; never renew it.
        let request = contracts::delivery::PaperInitialExecutionConsumeV1 {
            schema_version: contracts::SchemaV1,
            paper_initialization: self.receipt.paper_initialization.clone(),
            release_id: self.receipt.claim.handoff.release_id,
            external_claim_id: self
                .receipt
                .claim
                .handoff
                .external_claim_id
                .clone()
                .ok_or_else(|| anyhow!("PAPER_INITIAL_EXECUTION_CLAIM_REQUIRED"))?,
            owner_instance_id: self.receipt.owner_instance_id,
        };
        domain::delivery::paper_initial_execution_claim(
            &request,
            self.receipt.claim.handoff.id,
            &self.receipt.claim,
            chrono::Utc::now(),
        )
        .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_EXPIRED_OR_CHANGED"))?;
        let evidence = serde_json::json!({
            "paper_initialization": self.receipt.paper_initialization,
            "owner_instance_id": self.receipt.owner_instance_id,
            "consuming_credential_id": self.receipt.consuming_credential_id,
            "consumed_at": self.receipt.consumed_at,
            "handoff_id": self.receipt.claim.handoff.id,
            "release_id": self.receipt.claim.handoff.release_id,
            "package_artifact_id": self.receipt.package_artifact_id,
            "execution_environment": "PAPER", "economic_origin": "SYNTHETIC",
        });
        Ok((self.receipt.claim, evidence))
    }
}

impl InitialExecutionAuthority {
    pub(crate) async fn consume(
        &mut self,
        expected: &contracts::strategy_portfolio::HandoffClaimViewV2,
    ) -> Result<InitialCapitalPermit> {
        use contracts::{
            SchemaV1,
            control::CommandResult,
            delivery::{PaperInitialExecutionConsumeV1, PaperInitialExecutionViewV1},
            strategy_portfolio::TargetPackageEnvelopeV2,
        };
        ensure!(!self.attempted, "PAPER_INITIAL_EXECUTION_ALREADY_ATTEMPTED");
        let TargetPackageEnvelopeV2::Forecast(package) = &expected.package else {
            return Err(anyhow!("PAPER_INITIAL_EXECUTION_REQUIRES_FORECAST"));
        };
        let account = crate::paper_node::forecast_initial_account(package)?
            .ok_or_else(|| anyhow!(crate::paper_node::forecast_initialization_blocker(package)))?;
        ensure!(
            expected.handoff.environment == contracts::forward::ForwardEnvironmentV1::Paper
                && expected.handoff.downstream_id == account.downstream_id,
            "PAPER_INITIAL_EXECUTION_SCOPE"
        );
        let request = PaperInitialExecutionConsumeV1 {
            schema_version: SchemaV1,
            paper_initialization: package
                .current_weights
                .paper_initialization
                .clone()
                .ok_or_else(|| anyhow!("PAPER_INITIAL_EXECUTION_ROOT_REQUIRED"))?,
            release_id: expected.handoff.release_id,
            external_claim_id: expected
                .handoff
                .external_claim_id
                .clone()
                .ok_or_else(|| anyhow!("PAPER_INITIAL_EXECUTION_CLAIM_REQUIRED"))?,
            owner_instance_id: self.owner_instance_id,
        };
        let mut endpoint = self.origin.clone();
        endpoint.set_path(&format!(
            "/api/v2/handoffs/{}/paper-initial-execution/consume",
            expected.handoff.id
        ));
        // Once attempted, transport loss and every replay remain blocked. Never
        // retry with a new key/owner to turn uncertainty into another funding.
        self.attempted = true;
        let response = service_http::send(
            self.client
                .post(endpoint)
                .header("idempotency-key", self.owner_instance_id.to_string())
                .json(&request),
        )
        .await
        .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_RESULT_UNKNOWN"))?;
        let response = service_http::checked(response, 200, &self.credential)
            .await
            .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_BLOCKED"))?;
        service_http::media(&response, "application/json")
            .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_RESPONSE_INVALID"))?;
        let bytes = service_http::body(response, 8 * 1024 * 1024)
            .await
            .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_RESULT_UNKNOWN"))?;
        let result: CommandResult<PaperInitialExecutionViewV1> =
            service_http::decode(&bytes, &self.credential)
                .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_RESPONSE_INVALID"))?;
        domain::delivery::paper_initial_execution_response(
            &request,
            expected,
            self.owner_instance_id,
            &result,
            chrono::Utc::now(),
        )
        .map_err(|_| anyhow!("PAPER_INITIAL_EXECUTION_RESPONSE_BLOCKED"))?;
        Ok(InitialCapitalPermit {
            receipt: result.resource,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::{Json, Router, extract::State, routing::post};
    use contracts::{
        SchemaV1,
        control::CommandResult,
        delivery::{
            PaperInitialExecutionConsumeV1, PaperInitialExecutionStateV1,
            PaperInitialExecutionViewV1,
        },
        strategy_portfolio::HandoffClaimViewV2,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Clone)]
    struct FixtureServer {
        claim: HandoffClaimViewV2,
        count: Arc<AtomicUsize>,
        mode: &'static str,
        authorization: String,
        stop_on_response: Option<tokio::sync::watch::Sender<bool>>,
        response_gate: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    }

    async fn respond(
        State(state): State<FixtureServer>,
        axum::extract::Path(handoff_id): axum::extract::Path<Id>,
        headers: axum::http::HeaderMap,
        Json(request): Json<PaperInitialExecutionConsumeV1>,
    ) -> Json<CommandResult<PaperInitialExecutionViewV1>> {
        assert_eq!(handoff_id, state.claim.handoff.id);
        assert!(
            headers
                .get("authorization")
                .is_some_and(|h| h == state.authorization.as_str())
        );
        assert!(
            headers
                .get("idempotency-key")
                .is_some_and(|h| h == request.owner_instance_id.to_string().as_str())
        );
        let previous = state.count.fetch_add(1, Ordering::SeqCst);
        if let Some(stop) = &state.stop_on_response {
            stop.send_replace(true);
        }
        if let Some((received, release)) = &state.response_gate {
            received.notify_one();
            release.notified().await;
        }
        let mut claim = state.claim;
        if state.mode == "changed-target" {
            let contracts::strategy_portfolio::TargetPackageEnvelopeV2::Forecast(package) =
                &mut claim.package
            else {
                unreachable!()
            };
            package.targets[0].target_weight = "0.1".parse().unwrap();
        }
        Json(CommandResult {
            schema_version: SchemaV1,
            replayed: previous > 0 || state.mode == "replayed",
            resource: PaperInitialExecutionViewV1 {
                schema_version: SchemaV1,
                state: PaperInitialExecutionStateV1::Consumed,
                paper_initialization: request.paper_initialization,
                package_artifact_id: Id::new(),
                owner_instance_id: if state.mode == "wrong-owner" {
                    Id::new()
                } else {
                    request.owner_instance_id
                },
                consuming_credential_id: Id::new(),
                consumed_at: chrono::Utc::now(),
                claim: Box::new(claim),
            },
        })
    }

    pub(crate) async fn fixture(
        mode: &'static str,
    ) -> (
        tempfile::TempDir,
        InitialExecutionAuthorityConfig,
        HandoffClaimViewV2,
        Arc<AtomicUsize>,
        tokio::task::JoinHandle<()>,
    ) {
        fixture_with_stop(mode, None).await
    }

    pub(crate) async fn fixture_with_stop(
        mode: &'static str,
        stop_on_response: Option<tokio::sync::watch::Sender<bool>>,
    ) -> (
        tempfile::TempDir,
        InitialExecutionAuthorityConfig,
        HandoffClaimViewV2,
        Arc<AtomicUsize>,
        tokio::task::JoinHandle<()>,
    ) {
        fixture_with_response_gate(mode, stop_on_response, None).await
    }

    pub(crate) async fn fixture_with_response_gate(
        mode: &'static str,
        stop_on_response: Option<tokio::sync::watch::Sender<bool>>,
        response_gate: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    ) -> (
        tempfile::TempDir,
        InitialExecutionAuthorityConfig,
        HandoffClaimViewV2,
        Arc<AtomicUsize>,
        tokio::task::JoinHandle<()>,
    ) {
        let (claim, _) = crate::polymarket_paper_host::tests::initial_claim_and_config();
        let directory = crate::paper_service::tests::private_state_directory();
        let token = format!("qz2.{}.{}", Id::new(), "A".repeat(43));
        let credential_file = directory.path().join("synthetic-machine-token");
        std::fs::write(&credential_file, &token).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&credential_file, std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }
        let count = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/api/v2/handoffs/{id}/paper-initial-execution/consume",
                post(respond),
            )
            .with_state(FixtureServer {
                claim: claim.clone(),
                count: count.clone(),
                mode,
                authorization: format!("Bearer {token}"),
                stop_on_response,
                response_gate,
            });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (
            directory,
            InitialExecutionAuthorityConfig {
                origin,
                credential_file,
                ca_certificate: None,
            },
            claim,
            count,
            task,
        )
    }

    #[tokio::test(flavor = "current_thread")]
    async fn authenticated_transport_mints_once_and_restart_cannot_use_old_response() {
        // A controlled HTTP contract fixture, not an actual Store consume or market account.
        let (_files, config, claim, count, server) = fixture("first-only").await;
        let mut owner = InitialExecutionAuthority::open(&config).unwrap();
        let permit = owner.consume(&claim).await.unwrap();
        let (canonical, evidence) = permit.into_canonical(&claim).unwrap();
        assert_eq!(
            serde_json::to_value(canonical).unwrap(),
            serde_json::to_value(&claim).unwrap()
        );
        assert_eq!(evidence["economic_origin"], "SYNTHETIC");
        assert!(owner.consume(&claim).await.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let mut restarted = InitialExecutionAuthority::open(&config).unwrap();
        assert_ne!(restarted.owner_instance_id, owner.owner_instance_id);
        assert!(restarted.consume(&claim).await.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 2);
        server.abort();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn replay_changed_package_and_foreign_owner_never_mint_permits() {
        for mode in ["replayed", "changed-target", "wrong-owner"] {
            let (_files, config, claim, count, server) = fixture(mode).await;
            let mut owner = InitialExecutionAuthority::open(&config).unwrap();
            assert!(owner.consume(&claim).await.is_err(), "{mode}");
            assert!(owner.consume(&claim).await.is_err(), "{mode}");
            assert_eq!(count.load(Ordering::SeqCst), 1);
            server.abort();
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn lost_response_is_unknown_and_not_automatically_retried() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (_files, mut config, claim, _, server) = fixture("unused").await;
        server.abort();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        config.origin = format!("http://{}", listener.local_addr().unwrap());
        let peer = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let _ = socket.read(&mut bytes).await;
            socket.shutdown().await.unwrap();
        });
        let mut owner = InitialExecutionAuthority::open(&config).unwrap();
        let error = owner.consume(&claim).await.err().unwrap();
        assert_eq!(error.to_string(), "PAPER_INITIAL_EXECUTION_RESULT_UNKNOWN");
        assert_eq!(
            owner.consume(&claim).await.err().unwrap().to_string(),
            "PAPER_INITIAL_EXECUTION_ALREADY_ATTEMPTED"
        );
        peer.await.unwrap();
    }

    #[test]
    fn connection_is_explicit_machine_only_and_does_not_accept_uploaded_owner_identity() {
        for origin in [
            "http://public.example",
            "https://user@host.example",
            "https://host.example/path",
            "https://host.example/?q=x",
        ] {
            assert!(connection_origin(origin).is_err());
        }
        assert!(connection_origin("https://qz.example").is_ok());
        assert!(connection_origin("http://127.0.0.1:8080").is_ok());
        assert!(
            serde_json::from_value::<InitialExecutionAuthorityConfig>(serde_json::json!({
                "origin": "https://qz.example", "credential_file": "/existing/qz-credential",
                "owner_instance_id": Id::new()
            }))
            .is_err()
        );
    }
}
