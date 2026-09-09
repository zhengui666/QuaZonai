//! Fixed-route, credential-scoped HTTP adapter. No ambient proxy or browser authority.
use super::{Failure, MissionBinding};
use chrono::{DateTime, Utc};
use contracts::{
    artifacts::{ArtifactAccess, ArtifactCreate, ArtifactProducer, ArtifactView},
    brief::{BriefState, BriefView},
    control::{CommandResult, MachineScope, MachineSessionView, PrincipalKind},
    experiments::{
        ExperimentOutcome, ExperimentProposalV1, ExperimentResultVisibility, ExperimentSource,
        ExperimentView,
    },
    research::DataOrigin,
    runs::{RunKind, RunSnapshotV1, RunState},
    Id,
};
use reqwest::{header, redirect::Policy, Client};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;
use url::{Host, Url};

pub(super) const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

pub(super) fn origin(value: &str, development_http: bool) -> Result<Url, Failure> {
    let parsed = Url::parse(value).map_err(|_| Failure::Configuration)?;
    if value.trim() != value
        || parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return Err(Failure::Configuration);
    }
    let loopback = match parsed.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    if parsed.scheme() != "https" && !(parsed.scheme() == "http" && development_http && loopback) {
        return Err(Failure::Configuration);
    }
    Ok(parsed)
}

// Deliberately no Debug: the HTTP client contains a sensitive Authorization header.
#[derive(Clone)]
pub(super) struct ControlClient {
    http: Client,
    origin: Url,
    binding: MissionBinding,
}
impl ControlClient {
    pub(super) fn new(
        api_origin: &str,
        development_http: bool,
        token: &str,
        binding: MissionBinding,
    ) -> Result<Self, Failure> {
        let origin = origin(api_origin, development_http)?;
        integrations::authentication::machine_token(token).map_err(|_| Failure::Configuration)?;
        let mut bearer = header::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| Failure::Configuration)?;
        bearer.set_sensitive(true);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, bearer);
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );
        let http = Client::builder()
            .default_headers(headers)
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| Failure::Configuration)?;
        Ok(Self {
            http,
            origin,
            binding,
        })
    }

    fn url(&self, route: Route) -> Url {
        let path = match route {
            Route::Identity => "/api/v2/auth/machine".to_owned(),
            Route::Run(id) => format!("/api/v2/runs/{id}"),
            Route::Brief(id) => format!("/api/v2/briefs/{id}"),
            Route::Artifacts => "/api/v2/artifacts".to_owned(),
            Route::Experiments => "/api/v2/experiments".to_owned(),
        };
        let mut url = self.origin.clone();
        url.set_path(&path);
        url
    }

    async fn response<T: DeserializeOwned>(
        mut response: reqwest::Response,
        expected: reqwest::StatusCode,
    ) -> Result<T, Failure> {
        if response.status() != expected {
            // Never parse/forward arbitrary upstream error bodies or redirects.
            return Err(Failure::Http(response.status().as_u16()));
        }
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next());
        if content_type != Some("application/json") {
            return Err(Failure::Contract);
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
        {
            return Err(Failure::ResponseLimit);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unavailable)? {
            if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) {
                return Err(Failure::ResponseLimit);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| Failure::Contract)
    }

    async fn get<T: DeserializeOwned>(&self, route: Route) -> Result<T, Failure> {
        let response = self
            .http
            .get(self.url(route))
            .send()
            .await
            .map_err(|_| Failure::Unavailable)?;
        Self::response(response, reqwest::StatusCode::OK).await
    }

    async fn post<T: DeserializeOwned>(
        &self,
        route: Route,
        key: &str,
        body: &impl Serialize,
    ) -> Result<T, Failure> {
        // Reuse the actual HTTP entrypoint's key validator. No automatic retries:
        // a lost POST response remains unknown and the caller retains this key.
        let mut headers = axum::http::HeaderMap::new();
        let header = axum::http::HeaderValue::from_str(key).map_err(|_| Failure::Contract)?;
        headers.insert("idempotency-key", header);
        crate::access::idempotency_key(&headers).map_err(|_| Failure::Contract)?;
        let response = self
            .http
            .post(self.url(route))
            .header("idempotency-key", key)
            .json(body)
            .send()
            .await
            .map_err(|_| Failure::Unavailable)?;
        Self::response(response, reqwest::StatusCode::CREATED).await
    }

    pub(super) async fn authority(&self) -> Result<(RunSnapshotV1, DateTime<Utc>), Failure> {
        self.checked_authority(None).await
    }

    pub(super) async fn require(
        &self,
        scope: MachineScope,
    ) -> Result<(RunSnapshotV1, DateTime<Utc>), Failure> {
        self.checked_authority(Some(scope)).await
    }

    async fn checked_authority(
        &self,
        required: Option<MachineScope>,
    ) -> Result<(RunSnapshotV1, DateTime<Utc>), Failure> {
        let session: MachineSessionView = self.get(Route::Identity).await?;
        let now = Utc::now();
        if session.kind != PrincipalKind::Mission
            || session.project_id != Some(self.binding.project_id)
            || session.run_id != Some(self.binding.run_id)
            || session.downstream_id.is_some()
            || session.expires_at <= now
            || !session.scope_codes.contains(&MachineScope::ResearchRead)
            || !session.scope_codes.contains(&MachineScope::RunRead)
            || required.is_some_and(|scope| !session.scope_codes.contains(&scope))
            || session.scope_codes.iter().any(|scope| {
                matches!(
                    scope,
                    MachineScope::DownstreamClaim
                        | MachineScope::DownstreamAck
                        | MachineScope::ForwardSubmit
                        | MachineScope::DoctorRead
                )
            })
        {
            return Err(Failure::Authority);
        }
        let run: RunSnapshotV1 = self.get(Route::Run(self.binding.run_id)).await?;
        if run.id != self.binding.run_id
            || run.project_id != self.binding.project_id
            || run.cycle_id != Some(self.binding.cycle_id)
            || run.active_attempt_id != Some(self.binding.attempt_id)
            || run.kind != RunKind::AgentResearch
            || !matches!(run.state, RunState::Dispatching | RunState::Running)
            || run.deadline_at <= Utc::now()
            || session.expires_at <= Utc::now()
        {
            return Err(Failure::Authority);
        }
        let expires = run.deadline_at.min(session.expires_at);
        Ok((run, expires))
    }

    pub(super) async fn brief(&self) -> Result<BriefView, Failure> {
        let brief: BriefView = self.get(Route::Brief(self.binding.brief_id)).await?;
        if brief.id != self.binding.brief_id
            || brief.project_id != self.binding.project_id
            || brief.state != BriefState::Frozen
            || brief.frozen_at.is_none()
        {
            return Err(Failure::Authority);
        }
        Ok(brief)
    }

    pub(super) async fn artifact(
        &self,
        key: &str,
        request: &ArtifactCreate,
    ) -> Result<CommandResult<ArtifactView>, Failure> {
        if request.project_id != self.binding.project_id {
            return Err(Failure::Authority);
        }
        let (_, expires) = self.require(MachineScope::ArtifactSubmit).await?;
        if expires <= Utc::now() {
            return Err(Failure::Deadline);
        }
        let result: CommandResult<ArtifactView> = self.post(Route::Artifacts, key, request).await?;
        let artifact = &result.resource;
        if artifact.project_id != self.binding.project_id
            || artifact.producer_run_id != Some(self.binding.run_id)
            || artifact.producer_attempt_id != Some(self.binding.attempt_id)
            || artifact.kind != request.kind.code()
            || artifact.media_type != request.kind.media_type()
            || artifact.schema_name != request.kind.schema_name()
            || artifact.schema_version != "1"
            || artifact.byte_count.get() != request.content.len() as u64
            || artifact.origin != DataOrigin::Synthetic
            || artifact.access_class != ArtifactAccess::Research
            || artifact.created_by != ArtifactProducer::Agent
        {
            return Err(Failure::Contract);
        }
        Ok(result)
    }

    pub(super) async fn propose(
        &self,
        key: &str,
        request: &ExperimentProposalV1,
    ) -> Result<CommandResult<ExperimentView>, Failure> {
        if request.cycle_id != self.binding.cycle_id {
            return Err(Failure::Authority);
        }
        domain::experiments::proposal(request).map_err(|_| Failure::Contract)?;
        self.require(MachineScope::ExperimentSubmit).await?;
        let result: CommandResult<ExperimentView> =
            self.post(Route::Experiments, key, request).await?;
        let experiment = &result.resource;
        if experiment.project_id != self.binding.project_id
            || experiment.cycle_id != self.binding.cycle_id
            || experiment.family_id != request.family_id
            || experiment.parent_experiment_id != request.parent_experiment_id
            || experiment.hypothesis != request.hypothesis
            || experiment.expected_failure_modes != request.expected_failure_modes
            || experiment.proposal_artifact_id != request.proposal_artifact_id
            || experiment.parameter_artifact_id != Some(request.parameter_artifact_id)
            || experiment.code_artifact_id != request.code_artifact_id
            || experiment.author_run_id != Some(self.binding.run_id)
            || experiment.author_attempt_id != Some(self.binding.attempt_id)
            || experiment.trial_source != ExperimentSource::Codex
            || experiment.ordinal == 0
            || experiment.result_visibility != ExperimentResultVisibility::Pending
            || experiment.outcome != Some(ExperimentOutcome::Pending)
            || experiment.outcome_reason.is_some()
            || experiment.conclusion_artifact_id.is_some()
            || experiment.run_id.is_some()
        {
            return Err(Failure::Contract);
        }
        Ok(result)
    }
}

enum Route {
    Identity,
    Run(Id),
    Brief(Id),
    Artifacts,
    Experiments,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_never_accepts_ambient_or_nonlocal_http_authority() {
        for url in [
            "http://localhost",
            "http://research.example",
            "ftp://127.0.0.1",
            "https://user:secret@research.example",
            "https://research.example/api",
            "https://research.example?token=secret",
            "https://research.example#token",
            " https://research.example",
            "https://research.example ",
        ] {
            assert!(origin(url, true).is_err());
        }
        assert!(origin("http://127.0.0.1:8080", false).is_err());
        assert!(origin("http://127.0.0.1:8080", true).is_ok());
        assert!(origin("http://[::1]:8080", true).is_ok());
        assert!(origin("https://research.example", false).is_ok());
    }
}
