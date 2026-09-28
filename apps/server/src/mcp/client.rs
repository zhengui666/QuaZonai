//! Fixed-route, credential-scoped HTTP adapter. No ambient proxy or browser authority.
use super::{Failure, MissionBinding};
use crate::service_http;
use chrono::{DateTime, Utc};
use contracts::{
    artifacts::{ArtifactAccess, ArtifactCreate, ArtifactProducer, ArtifactView},
    brief::{BriefState, BriefView},
    control::{CommandResult, MachineScope, MachineSessionView, PrincipalKind},
    cycles::CycleViewV1,
    experiments::{
        ExperimentOutcome, ExperimentProposalV1, ExperimentResultVisibility, ExperimentSource,
        ExperimentView,
    },
    research::DataOrigin,
    runs::{RunKind, RunSnapshotV1, RunState},
    Id,
};
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;
use url::Url;

pub(super) const MAX_RESPONSE_BYTES: usize = service_http::MAX_JSON_BYTES;

pub(super) fn origin(value: &str, development_http: bool) -> Result<Url, Failure> {
    Ok(service_http::origin(value, development_http)?)
}

// Deliberately no Debug: the HTTP client contains a sensitive Authorization header.
#[derive(Clone)]
pub(super) struct ControlClient {
    http: Client,
    origin: Url,
    binding: MissionBinding,
    credential: String,
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
        let http = service_http::builder(service_http::bearer(token)?, Duration::from_secs(10))
            .build()
            .map_err(|_| Failure::Configuration)?;
        Ok(Self {
            http,
            origin,
            binding,
            credential: token.to_owned(),
        })
    }

    fn url(&self, route: Route) -> Url {
        let path = match route {
            Route::Identity => "/api/v2/auth/machine".to_owned(),
            Route::Run(id) => format!("/api/v2/runs/{id}"),
            Route::Cycle(id) => format!("/api/v2/cycles/{id}"),
            Route::Brief(id) => format!("/api/v2/briefs/{id}"),
            Route::Artifacts => "/api/v2/artifacts".to_owned(),
            Route::Experiments => "/api/v2/experiments".to_owned(),
        };
        let mut url = self.origin.clone();
        url.set_path(&path);
        url
    }

    async fn response<T: DeserializeOwned>(
        &self,
        response: reqwest::Response,
        expected: reqwest::StatusCode,
    ) -> Result<T, Failure> {
        let status = response.status().as_u16();
        let response =
            match service_http::checked(response, expected.as_u16(), &self.credential).await {
                // Preserve a closed status-only error for non-native error bodies and
                // redirects. Only a validated native Problem can expose safe metadata.
                Err(service_http::Failure::Contract) if status != expected.as_u16() => {
                    return Err(Failure::Http(status));
                }
                result => result?,
            };
        service_http::media(&response, "application/json")?;
        let bytes = service_http::body(response, MAX_RESPONSE_BYTES).await?;
        Ok(service_http::decode(&bytes, &self.credential)?)
    }

    async fn get<T: DeserializeOwned>(&self, route: Route) -> Result<T, Failure> {
        let response = service_http::send(self.http.get(self.url(route))).await?;
        self.response(response, reqwest::StatusCode::OK).await
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
        let response = service_http::send(
            self.http
                .post(self.url(route))
                .header("idempotency-key", key)
                .json(body),
        )
        .await?;
        self.response(response, reqwest::StatusCode::CREATED).await
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
            || !matches!(
                run.state,
                RunState::Dispatching | RunState::Running | RunState::Reconciling
            )
            || run.deadline_at <= Utc::now()
            || session.expires_at <= Utc::now()
        {
            return Err(Failure::Authority);
        }
        let expires = run.deadline_at.min(session.expires_at);
        Ok((run, expires))
    }

    pub(super) async fn brief(&self) -> Result<BriefView, Failure> {
        let cycle: CycleViewV1 = self.get(Route::Cycle(self.binding.cycle_id)).await?;
        if cycle.id != self.binding.cycle_id
            || cycle.project_id != self.binding.project_id
            || cycle.brief_id != self.binding.brief_id
        {
            return Err(Failure::Authority);
        }
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
    Cycle(Id),
    Brief(Id),
    Artifacts,
    Experiments,
}

impl From<service_http::Failure> for Failure {
    fn from(value: service_http::Failure) -> Self {
        match value {
            service_http::Failure::Configuration => Self::Configuration,
            service_http::Failure::Unavailable => Self::Unavailable,
            service_http::Failure::Contract => Self::Contract,
            service_http::Failure::ResponseLimit => Self::ResponseLimit,
            service_http::Failure::Rejected(problem) => Self::Rejected {
                status: problem.status,
                code: problem.code,
                retryable: problem.retryable,
                request_id: problem.request_id,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_matches_the_configured_console_without_ambient_authority() {
        for url in [
            "http://192.168.1.1",
            "http://research.example",
            "ftp://127.0.0.1",
            "https://user:secret@research.example",
            "https://research.example/api",
            "https://research.example?token=secret",
            "https://research.example#token",
            " https://research.example",
            "https://research.example ",
            " http://localhost",
            "http://localhost ",
        ] {
            assert!(origin(url, true).is_err());
        }
        assert!(origin("http://127.0.0.1:8080", false).is_err());
        assert!(origin("http://127.0.0.1:8080", true).is_ok());
        assert!(origin("http://[::1]:8080", true).is_ok());
        assert!(origin("http://localhost:8081", false).is_err());
        assert_eq!(
            origin("http://localhost:8081", true).unwrap().as_str(),
            "http://localhost:8081/"
        );
        assert!(origin("https://localhost", false).is_ok());
        assert!(origin("https://research.example", false).is_ok());
    }
}
