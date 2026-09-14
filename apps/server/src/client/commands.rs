//! Closed CLI routing over the shared Rust request/response types.
use super::{Failure, Result};
use clap::{Args, Subcommand};
use contracts::{
    artifacts::{ArtifactCreate, ArtifactView},
    brief::{BriefCreate, BriefUpdate, BriefView},
    codex::{
        CodexAccountOperationV1, CodexAccountRequestV1, CodexAccountStartV1, CodexHomeBindingV1,
        CodexLoginCancelV1, CodexObservationV1, CodexProbeRequestV1, CodexProbeViewV1,
        CodexProfileCreateV1, CodexProfileUpdateV1, CodexProfileViewV1,
    },
    control::{
        CommandResult, OperatorGrantRequest, OperatorGrantView, Page, ProjectCreate, ProjectUpdate,
        ProjectView,
    },
    cycles::{
        BriefFreezeV1, CycleSelectionTrialV1, CycleSelectionV1, CycleStartV1, CycleStartedV1,
        CycleViewV1, FrozenBriefV1,
    },
    data::*,
    evidence::{
        AlphaEvaluateRequestV1, AlphaVersionView, AlphaView, CalibrationView, EvaluationView,
        MetricValueV1, QualificationView,
    },
    execution_assumptions::{ExecutionAssumptionsCreateV1, ExecutionAssumptionsViewV1},
    experiments::{ExperimentProposalV1, ExperimentView},
    forward::{DownstreamWeightsSubmitV1, DownstreamWeightsViewV1},
    lifecycle::{RunCancelV1, RunListQuery},
    portfolio::{CandidateDetailV1, CandidateViewV1, MandateCreateV1, MandateViewV1},
    research::{
        EvaluationPolicyCreate, EvaluationPolicyView, InputSetCreate, InputSetSummary, InputSetView,
    },
    runs::RunSnapshotV1,
    runtime::{RuntimeProbeRequestV1, RuntimeProbeViewV1, RuntimeReadinessV1},
    settings::*,
    Id,
};
use reqwest::Method;
use serde::{de::DeserializeOwned, Serialize};
use std::io::Read;

#[derive(Args)]
#[group(id = "Pagination")]
pub struct List {
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=100))]
    pub limit: u16,
}
#[derive(Args)]
pub struct ProjectList {
    #[arg(long)]
    pub project_id: String,
    #[command(flatten)]
    pub page: List,
}
#[derive(Subcommand)]
pub enum Command {
    #[command(subcommand)]
    Migrate(Migrate),
    /// Submit target-only weights using the authenticated downstream identity.
    ForwardWeights,
    #[command(subcommand)]
    Forward(Forward),
    #[command(subcommand)]
    Project(Project),
    #[command(subcommand)]
    Brief(Brief),
    #[command(subcommand)]
    Portfolio(Portfolio),
    #[command(subcommand)]
    Release(Release),
    #[command(subcommand)]
    Approval(Approval),
    #[command(subcommand)]
    Automation(Automation),
    #[command(subcommand)]
    Handoff(Handoff),
    #[command(subcommand)]
    Cycle(Cycle),
    #[command(subcommand)]
    Data(Data),
    #[command(subcommand)]
    Runtime(Runtime),
    #[command(subcommand)]
    Codex(Codex),
    #[command(subcommand)]
    Downstream(Downstream),
    #[command(subcommand)]
    InputSet(InputSet),
    #[command(subcommand)]
    Policy(Policy),
    #[command(subcommand)]
    Experiment(Experiment),
    #[command(subcommand)]
    Alpha(Alpha),
    #[command(subcommand)]
    Evidence(Evidence),
    #[command(subcommand)]
    Artifact(Artifact),
    #[command(subcommand)]
    Run(Run),
    /// Read a strict OperatorGrantRequest including current TOTP from stdin.
    /// This does not give the CLI lasting Operator authority.
    OperatorGrant,
    /// Write-only IntegrationSecretCreate from stdin; prints only its native reference.
    CredentialRegister,
}
#[derive(Subcommand)]
pub enum Alpha {
    List(ProjectList),
    Qualifications {
        id: String,
        #[command(flatten)]
        page: List,
    },
    Evaluate {
        id: String,
    },
    Calibration {
        id: String,
    },
    Versions {
        id: String,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
        version: String,
    },
    Evaluations {
        id: String,
        #[command(flatten)]
        page: List,
    },
}

#[derive(Subcommand)]
pub enum Forward {
    Weights {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
    Submit,
    Window {
        id: String,
        #[arg(long)]
        stream: String,
    },
    List {
        id: String,
        #[command(flatten)]
        page: List,
    },
}

#[derive(Subcommand)]
pub enum Handoff {
    List {
        id: String,
        #[command(flatten)]
        page: List,
    },
    Offer,
    Ack {
        id: String,
    },
    Claim {
        id: String,
    },
    Show {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum Automation {
    Authorize {
        id: String,
    },
    Revoke {
        id: String,
    },
    Show {
        id: String,
    },
    List {
        id: String,
        #[command(flatten)]
        page: List,
    },
    Revocations {
        id: String,
        #[command(flatten)]
        page: List,
    },
}
#[derive(Subcommand)]
pub enum Approval {
    List {
        release_id: String,
        #[command(flatten)]
        page: List,
    },
    Revoke {
        id: String,
    },
    Revocations {
        id: String,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum Release {
    Create,
    List {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
    Approve {
        id: String,
    },
    Reject {
        id: String,
    },
    Reconsider {
        id: String,
    },
    Decisions {
        id: String,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum Portfolio {
    /// Queue a source-bound portfolio build, not an approval or delivery.
    Build,
    /// Hold original Candidate targets in one simulated account; not Evaluation approval.
    Simulate,
    /// Study the original policy window and complete Candidate cohort; not approval or delivery.
    Study,
    #[command(subcommand)]
    Candidate(Candidate),
    #[command(subcommand)]
    Mandate(Mandate),
    #[command(subcommand)]
    Assumptions(ExecutionAssumptions),
}
#[derive(Subcommand)]
pub enum Candidate {
    List {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
    Evaluations {
        id: String,
        #[command(flatten)]
        page: List,
    },
}
#[derive(Subcommand)]
pub enum ExecutionAssumptions {
    Create,
    List {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
}
#[derive(Subcommand)]
pub enum Mandate {
    Create,
    List {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
}
#[derive(Subcommand)]
pub enum Evidence {
    Show {
        id: String,
    },
    Metrics {
        id: String,
        #[command(flatten)]
        page: List,
    },
}

#[derive(Subcommand)]
pub enum Project {
    List(List),
    Show { id: String },
    Create,
    Update { id: String },
}
#[derive(Subcommand)]
pub enum Brief {
    List {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
    Create {
        project_id: String,
    },
    Update {
        id: String,
    },
    Freeze {
        id: String,
    },
}
#[derive(Subcommand)]
pub enum Cycle {
    Selection {
        id: String,
    },
    Trials {
        id: String,
        #[command(flatten)]
        page: List,
    },
    List {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
    Start {
        project_id: String,
    },
}
#[derive(Subcommand)]
pub enum Data {
    /// Start bounded native validation of the frozen InputSet described on stdin.
    Validate,
    #[command(subcommand)]
    Source(Source),
    #[command(subcommand)]
    Grant(Grant),
    #[command(subcommand)]
    Revision(Revision),
    #[command(subcommand)]
    Universe(Universe),
}
#[derive(Subcommand)]
pub enum Source {
    List(List),
    Show { id: String },
    Create,
    Update { id: String },
}
#[derive(Subcommand)]
pub enum Grant {
    List {
        source_id: String,
        #[command(flatten)]
        page: List,
    },
    Create {
        source_id: String,
    },
    Revoke {
        id: String,
    },
    Revocations {
        id: String,
        #[command(flatten)]
        page: List,
    },
}
#[derive(Subcommand)]
pub enum Revision {
    List {
        #[arg(long)]
        source_id: Option<String>,
        #[arg(long)]
        partition: Option<String>,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
    Register,
}
#[derive(Subcommand)]
pub enum Universe {
    List(List),
    Show { id: String },
}
#[derive(Subcommand)]
pub enum Runtime {
    List(List),
    Show { id: String },
    Create,
    Update { id: String },
    Probe { id: String },
    Readiness { id: String },
}
#[derive(Subcommand)]
pub enum Codex {
    /// List nonsecret native profile configuration.
    List(List),
    Show {
        id: String,
    },
    /// Read CodexProfileCreateV1 on stdin and register a deployment label.
    Create,
    /// Read CodexProfileUpdateV1 on stdin; requires CAS and an Operator grant.
    Update {
        id: String,
    },
    /// Explicit native inspection, no paid inference. Read CodexProbeRequestV1 on stdin.
    Probe {
        id: String,
    },
    /// Read the last observation without invoking model/list or refreshing authentication.
    Models {
        id: String,
    },
    /// Read the last nonsecret account observation, never native auth.json.
    Account {
        id: String,
    },
    /// List deployment labels, never native paths or environment values.
    Homes,
    /// Start native device login; read CodexAccountRequestV1 on stdin.
    Login,
    /// Sign out through native Codex; read CodexAccountRequestV1 on stdin.
    Logout,
    /// Cancel the exact native login; read CodexLoginCancelV1 on stdin.
    LoginCancel,
    /// Read an account operation without returning a device code.
    LoginStatus {
        id: String,
    },
}

#[derive(Subcommand)]
pub enum Downstream {
    List(List),
    Show { id: String },
    Create,
    Update { id: String },
    Probe { id: String },
    Readiness { id: String },
}
#[derive(Subcommand)]
pub enum InputSet {
    List(ProjectList),
    Show { id: String },
    Create,
}
#[derive(Subcommand)]
pub enum Policy {
    List(ProjectList),
    Show { id: String },
    Create,
}
#[derive(Subcommand)]
pub enum Experiment {
    List(ProjectList),
    Show { id: String },
    Propose,
}
#[derive(Subcommand)]
pub enum Artifact {
    List(ProjectList),
    Show { id: String },
    Submit,
    Export { id: String },
}
#[derive(Subcommand)]
pub enum Run {
    /// Read original automatic rebalance provenance.
    Rebalance {
        id: String,
    },
    List {
        #[arg(long)]
        project_id: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[command(flatten)]
        page: List,
    },
    Show {
        id: String,
    },
    Cancel {
        id: String,
    },
    Watch {
        id: String,
        #[arg(long)]
        after: Option<String>,
        #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u32).range(1..=3600))]
        max_seconds: u32,
        #[arg(long, default_value_t = 1000, value_parser = clap::value_parser!(u32).range(1..=10000))]
        max_events: u32,
    },
}

pub(super) enum Output {
    Json(fn(&[u8]) -> Result<serde_json::Value>),
    Binary {
        id: Id,
    },
    Events {
        run: Id,
        after: Option<String>,
        seconds: u32,
        events: u32,
    },
}
#[derive(Subcommand)]
pub enum Migrate {
    Reports(List),
    Source {
        id: String,
    },
    Mappings {
        id: String,
        #[command(flatten)]
        page: List,
    },
    /// Import a deployment-registered historical projection; requires exact Operator grant.
    Import {
        #[arg(long)]
        export_ref: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Read an import report created by this CLI credential.
    Report {
        id: String,
    },
}
pub(super) struct Request {
    pub method: Method,
    pub route: String,
    pub query: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub status: u16,
    pub operator: bool,
    pub output: Output,
}
fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<serde_json::Value> {
    let value: T = serde_json::from_slice(bytes).map_err(|_| Failure::Contract)?;
    serde_json::to_value(value).map_err(|_| Failure::Contract)
}
fn read_input<T: DeserializeOwned + Serialize>() -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::Input)?;
    if bytes.is_empty() || bytes.len() > 16 * 1024 * 1024 {
        return Err(Failure::Input);
    }
    let value: T = serde_json::from_slice(&bytes).map_err(|_| Failure::Input)?;
    serde_json::to_vec(&value).map_err(|_| Failure::Input)
}
fn id(value: String) -> Result<Id> {
    value.try_into().map_err(|_| Failure::Input)
}
fn item(base: &str, value: String) -> Result<String> {
    Ok(format!("{base}/{}", id(value)?))
}
fn action(base: &str, value: String, suffix: &str) -> Result<String> {
    Ok(format!("{}/{suffix}", item(base, value)?))
}
impl Request {
    fn get<T: DeserializeOwned + Serialize>(route: impl Into<String>) -> Self {
        Self {
            method: Method::GET,
            route: route.into(),
            query: vec![],
            body: None,
            status: 200,
            operator: false,
            output: Output::Json(decode::<T>),
        }
    }
    fn write<T: DeserializeOwned + Serialize, R: DeserializeOwned + Serialize>(
        method: Method,
        route: impl Into<String>,
        status: u16,
        operator: bool,
    ) -> Result<Self> {
        Ok(Self {
            method,
            route: route.into(),
            query: vec![],
            body: Some(read_input::<T>()?),
            status,
            operator,
            output: Output::Json(decode::<R>),
        })
    }
    fn page(mut self, page: List) -> Result<Self> {
        self.query.push(("limit".into(), page.limit.to_string()));
        if let Some(cursor) = page.cursor {
            self.query.push(("cursor".into(), id(cursor)?.to_string()));
        }
        Ok(self)
    }
    fn project(mut self, list: ProjectList) -> Result<Self> {
        self.query
            .push(("project_id".into(), id(list.project_id)?.to_string()));
        self.page(list.page)
    }
}

impl Command {
    pub(super) fn request(self) -> Result<Request> {
        const GET: Method = Method::GET;
        const PATCH: Method = Method::PATCH;
        const POST: Method = Method::POST;
        let result = match self {
            Self::Migrate(command) => match command {
                Migrate::Reports(page) => Request::get::<
                    Page<contracts::imports::HistoricalImportReportV1>,
                >("/api/v2/migrations/reports")
                .page(page)?,
                Migrate::Source { id } => {
                    Request::get::<contracts::imports::HistoricalRowExportV1>(action(
                        "/api/v2/migrations/reports",
                        id,
                        "source",
                    )?)
                }
                Migrate::Mappings { id, page } => {
                    Request::get::<Page<contracts::imports::HistoricalMappingViewV1>>(action(
                        "/api/v2/migrations/reports",
                        id,
                        "mappings",
                    )?)
                    .page(page)?
                }
                Migrate::Import {
                    export_ref,
                    dry_run,
                } => Request {
                    method: POST,
                    route: "/api/v2/migrations/import".into(),
                    query: vec![],
                    body: Some(
                        serde_json::to_vec(&contracts::imports::HistoricalImportRequestV1 {
                            schema_version: contracts::SchemaV1,
                            export_ref: id(export_ref)?,
                            dry_run,
                        })
                        .map_err(|_| Failure::Input)?,
                    ),
                    status: 202,
                    operator: true,
                    output: Output::Json(
                        decode::<CommandResult<contracts::imports::HistoricalImportReportV1>>,
                    ),
                },
                Migrate::Report { id } => {
                    Request::get::<contracts::imports::HistoricalImportReportV1>(item(
                        "/api/v2/migrations/reports",
                        id,
                    )?)
                }
            },
            Self::ForwardWeights => Request::write::<
                DownstreamWeightsSubmitV1,
                CommandResult<DownstreamWeightsViewV1>,
            >(POST, "/api/v2/forward/weights", 201, false)?,
            Self::Project(command) => match command {
                Project::List(page) => {
                    Request::get::<Page<ProjectView>>("/api/v2/projects").page(page)?
                }
                Project::Show { id } => Request::get::<ProjectView>(item("/api/v2/projects", id)?),
                Project::Create => Request::write::<ProjectCreate, CommandResult<ProjectView>>(
                    POST,
                    "/api/v2/projects",
                    201,
                    true,
                )?,
                Project::Update { id } => Request::write::<
                    ProjectUpdate,
                    CommandResult<ProjectView>,
                >(
                    PATCH, item("/api/v2/projects", id)?, 200, true
                )?,
            },
            Self::Automation(command) => match command {
                Automation::Authorize { id } => Request::write::<
                    contracts::delivery::AutomationAuthorizeV1,
                    CommandResult<contracts::delivery::AutomationPolicyViewV1>,
                >(
                    POST,
                    action("/api/v2/projects", id, "automation-policies")?,
                    201,
                    true,
                )?,
                Automation::Revoke { id } => Request::write::<
                    contracts::delivery::PolicyRevokeV1,
                    CommandResult<contracts::delivery::PolicyRevocationViewV1>,
                >(
                    POST,
                    action("/api/v2/automation-policies", id, "revoke")?,
                    201,
                    true,
                )?,
                Automation::Show { id } => {
                    Request::get::<contracts::delivery::AutomationPolicyViewV1>(item(
                        "/api/v2/automation-policies",
                        id,
                    )?)
                }
                Automation::List { id, page } => {
                    Request::get::<Page<contracts::delivery::AutomationPolicyViewV1>>(action(
                        "/api/v2/projects",
                        id,
                        "automation-policies",
                    )?)
                    .page(page)?
                }
                Automation::Revocations { id, page } => {
                    Request::get::<Page<contracts::delivery::PolicyRevocationViewV1>>(action(
                        "/api/v2/automation-policies",
                        id,
                        "revocations",
                    )?)
                    .page(page)?
                }
            },
            Self::Forward(command) => match command {
                Forward::Weights { project_id, page } => {
                    Request::get::<Page<contracts::forward::DownstreamWeightsViewV1>>(action(
                        "/api/v2/projects",
                        project_id,
                        "forward-weight-snapshots",
                    )?)
                    .page(page)?
                }
                Forward::Window { id, stream } => {
                    let mut request = Request::get::<contracts::forward::ForwardWindowViewV1>(
                        action("/api/v2/handoffs", id, "forward-window")?,
                    );
                    request.query.push(("stream_id".into(), stream));
                    request
                }
                Forward::Submit => Request::write::<
                    contracts::forward::ForwardMessageSubmitV1,
                    CommandResult<contracts::forward::ForwardMessageViewV1>,
                >(POST, "/api/v2/forward/messages", 201, false)?,
                Forward::List { id, page } => {
                    Request::get::<Page<contracts::forward::ForwardMessageViewV1>>(action(
                        "/api/v2/projects",
                        id,
                        "forward",
                    )?)
                    .page(page)?
                }
            },
            Self::Handoff(command) => match command {
                Handoff::List { id, page } => {
                    Request::get::<Page<contracts::delivery::HandoffViewV1>>(action(
                        "/api/v2/projects",
                        id,
                        "handoffs",
                    )?)
                    .page(page)?
                }
                Handoff::Claim { id } => {
                    Request::write::<
                        contracts::delivery::HandoffClaimV1,
                        CommandResult<contracts::delivery::HandoffClaimViewV1>,
                    >(
                        POST, action("/api/v2/handoffs", id, "claim")?, 200, false
                    )?
                }
                Handoff::Ack { id } => {
                    Request::write::<
                        contracts::delivery::HandoffAckV1,
                        CommandResult<contracts::delivery::HandoffViewV1>,
                    >(POST, action("/api/v2/handoffs", id, "ack")?, 200, false)?
                }
                Handoff::Offer => Request::write::<
                    contracts::delivery::HandoffOfferV1,
                    CommandResult<contracts::delivery::HandoffViewV1>,
                >(POST, "/api/v2/handoffs", 201, true)?,
                Handoff::Show { id } => Request::get::<contracts::delivery::HandoffViewV1>(item(
                    "/api/v2/handoffs",
                    id,
                )?),
            },
            Self::Approval(Approval::Revoke { id }) => {
                Request::write::<
                    contracts::delivery::ApprovalRevokeV1,
                    CommandResult<contracts::delivery::ApprovalRevocationViewV1>,
                >(POST, action("/api/v2/approvals", id, "revoke")?, 201, true)?
            }
            Self::Approval(Approval::Revocations { id, page }) => {
                Request::get::<Page<contracts::delivery::ApprovalRevocationViewV1>>(action(
                    "/api/v2/approvals",
                    id,
                    "revocations",
                )?)
                .page(page)?
            }
            Self::Approval(Approval::List { release_id, page }) => {
                Request::get::<Page<contracts::delivery::ApprovalViewV1>>(action(
                    "/api/v2/releases",
                    release_id,
                    "approvals",
                )?)
                .page(page)?
            }
            Self::Approval(Approval::Show { id }) => {
                Request::get::<contracts::delivery::ApprovalViewV1>(item("/api/v2/approvals", id)?)
            }
            Self::Release(command) => match command {
                Release::List { project_id, page } => {
                    Request::get::<Page<contracts::delivery::ReleaseViewV1>>(action(
                        "/api/v2/projects",
                        project_id,
                        "releases",
                    )?)
                    .page(page)?
                }
                Release::Approve { id } => Request::write::<
                    contracts::delivery::ReleaseApproveV1,
                    CommandResult<contracts::delivery::ApprovalViewV1>,
                >(
                    POST,
                    action("/api/v2/releases", id, "approvals")?,
                    201,
                    true,
                )?,
                Release::Reject { id } => Request::write::<
                    contracts::delivery::ReleaseRejectV1,
                    CommandResult<contracts::delivery::ReleaseDecisionViewV1>,
                >(
                    POST,
                    action("/api/v2/releases", id, "rejections")?,
                    201,
                    true,
                )?,
                Release::Reconsider { id } => Request::write::<
                    contracts::delivery::ReleaseReopenV1,
                    CommandResult<contracts::delivery::ReleaseDecisionViewV1>,
                >(
                    POST,
                    action("/api/v2/release-decisions", id, "reopen")?,
                    201,
                    true,
                )?,
                Release::Decisions { id, page } => {
                    Request::get::<Page<contracts::delivery::ReleaseDecisionViewV1>>(action(
                        "/api/v2/releases",
                        id,
                        "decisions",
                    )?)
                    .page(page)?
                }
                Release::Create => Request::write::<
                    contracts::delivery::ReleaseCreateV1,
                    CommandResult<contracts::delivery::ReleaseViewV1>,
                >(POST, "/api/v2/releases", 201, true)?,
                Release::Show { id } => Request::get::<contracts::delivery::ReleaseViewV1>(item(
                    "/api/v2/releases",
                    id,
                )?),
            },
            Self::Portfolio(Portfolio::Build) => {
                Request::write::<
                    contracts::portfolio::PortfolioBuildRequestV1,
                    CommandResult<RunSnapshotV1>,
                >(POST, "/api/v2/portfolio-builds", 202, true)?
            }
            Self::Portfolio(Portfolio::Simulate) => {
                Request::write::<
                    contracts::portfolio::CandidateSimulationRequestV1,
                    CommandResult<RunSnapshotV1>,
                >(POST, "/api/v2/candidate-simulations", 202, true)?
            }
            Self::Portfolio(Portfolio::Study) => {
                Request::write::<
                    contracts::portfolio::PortfolioStudyRequestV1,
                    CommandResult<RunSnapshotV1>,
                >(POST, "/api/v2/portfolio-studies", 202, true)?
            }
            Self::Portfolio(Portfolio::Candidate(command)) => match command {
                Candidate::List { project_id, page } => Request::get::<Page<CandidateViewV1>>(
                    action("/api/v2/projects", project_id, "portfolio-candidates")?,
                )
                .page(page)?,
                Candidate::Show { id } => {
                    Request::get::<CandidateDetailV1>(item("/api/v2/portfolio-candidates", id)?)
                }
                Candidate::Evaluations { id, page } => Request::get::<Page<EvaluationView>>(
                    action("/api/v2/portfolio-candidates", id, "evaluations")?,
                )
                .page(page)?,
            },
            Self::Portfolio(Portfolio::Mandate(command)) => {
                match command {
                    Mandate::Create => Request::write::<
                        MandateCreateV1,
                        CommandResult<MandateViewV1>,
                    >(
                        POST, "/api/v2/portfolio-mandates", 201, true
                    )?,
                    Mandate::List { project_id, page } => Request::get::<Page<MandateViewV1>>(
                        action("/api/v2/projects", project_id, "portfolio-mandates")?,
                    )
                    .page(page)?,
                    Mandate::Show { id } => {
                        Request::get::<MandateViewV1>(item("/api/v2/portfolio-mandates", id)?)
                    }
                }
            }
            Self::Portfolio(Portfolio::Assumptions(command)) => match command {
                ExecutionAssumptions::Create => {
                    Request::write::<
                        ExecutionAssumptionsCreateV1,
                        CommandResult<ExecutionAssumptionsViewV1>,
                    >(POST, "/api/v2/execution-assumptions", 201, true)?
                }
                ExecutionAssumptions::List { project_id, page } => {
                    Request::get::<Page<ExecutionAssumptionsViewV1>>(action(
                        "/api/v2/projects",
                        project_id,
                        "execution-assumptions",
                    )?)
                    .page(page)?
                }
                ExecutionAssumptions::Show { id } => Request::get::<ExecutionAssumptionsViewV1>(
                    item("/api/v2/execution-assumptions", id)?,
                ),
            },
            Self::Brief(command) => match command {
                Brief::List { project_id, page } => Request::get::<Page<BriefView>>(action(
                    "/api/v2/projects",
                    project_id,
                    "briefs",
                )?)
                .page(page)?,
                Brief::Show { id } => Request::get::<BriefView>(item("/api/v2/briefs", id)?),
                Brief::Create { project_id } => {
                    Request::write::<BriefCreate, CommandResult<BriefView>>(
                        POST,
                        action("/api/v2/projects", project_id, "briefs")?,
                        201,
                        true,
                    )?
                }
                Brief::Update { id } => Request::write::<BriefUpdate, CommandResult<BriefView>>(
                    PATCH,
                    item("/api/v2/briefs", id)?,
                    200,
                    true,
                )?,
                Brief::Freeze { id } => Request::write::<
                    BriefFreezeV1,
                    CommandResult<FrozenBriefV1>,
                >(
                    POST, action("/api/v2/briefs", id, "freeze")?, 200, true
                )?,
            },
            Self::Cycle(command) => {
                match command {
                    Cycle::Selection { id } => {
                        Request::get::<CycleSelectionV1>(action("/api/v2/cycles", id, "selection")?)
                    }
                    Cycle::Trials { id, page } => Request::get::<Page<CycleSelectionTrialV1>>(
                        action("/api/v2/cycles", id, "selection/trials")?,
                    )
                    .page(page)?,
                    Cycle::List { project_id, page } => Request::get::<Page<CycleViewV1>>(action(
                        "/api/v2/projects",
                        project_id,
                        "cycles",
                    )?)
                    .page(page)?,
                    Cycle::Show { id } => Request::get::<CycleViewV1>(item("/api/v2/cycles", id)?),
                    Cycle::Start { project_id } => {
                        Request::write::<CycleStartV1, CommandResult<CycleStartedV1>>(
                            POST,
                            action("/api/v2/projects", project_id, "cycles")?,
                            202,
                            true,
                        )?
                    }
                }
            }
            Self::Data(Data::Validate) => Request::write::<
                DataValidateRequest,
                CommandResult<RunSnapshotV1>,
            >(POST, "/api/v2/data/validate", 202, true)?,
            Self::Data(Data::Source(command)) => {
                match command {
                    Source::List(page) => {
                        Request::get::<Page<DataSourceView>>("/api/v2/data/sources").page(page)?
                    }
                    Source::Show { id } => {
                        Request::get::<DataSourceView>(item("/api/v2/data/sources", id)?)
                    }
                    Source::Create => Request::write::<
                        DataSourceCreate,
                        CommandResult<DataSourceView>,
                    >(
                        POST, "/api/v2/data/sources", 201, true
                    )?,
                    Source::Update { id } => Request::write::<
                        DataSourceUpdate,
                        CommandResult<DataSourceView>,
                    >(
                        PATCH, item("/api/v2/data/sources", id)?, 200, true
                    )?,
                }
            }
            Self::Data(Data::Grant(command)) => match command {
                Grant::List { source_id, page } => Request::get::<Page<DataGrantView>>(action(
                    "/api/v2/data/sources",
                    source_id,
                    "grants",
                )?)
                .page(page)?,
                Grant::Create { source_id } => {
                    let source = id(source_id)?;
                    let value = Request::write::<DataGrantCreate, CommandResult<DataGrantView>>(
                        POST,
                        format!("/api/v2/data/sources/{source}/grants"),
                        201,
                        true,
                    )?;
                    let body: DataGrantCreate =
                        serde_json::from_slice(value.body.as_ref().ok_or(Failure::Input)?)
                            .map_err(|_| Failure::Input)?;
                    if body.source_id != source {
                        return Err(Failure::Input);
                    }
                    value
                }
                Grant::Revoke { id } => {
                    Request::write::<DataGrantRevoke, CommandResult<DataGrantRevocationView>>(
                        POST,
                        action("/api/v2/data/grants", id, "revoke")?,
                        201,
                        true,
                    )?
                }
                Grant::Revocations { id, page } => Request::get::<Page<DataGrantRevocationView>>(
                    action("/api/v2/data/grants", id, "revocations")?,
                )
                .page(page)?,
            },
            Self::Data(Data::Revision(command)) => match command {
                Revision::Show { id } => {
                    Request::get::<DatasetView>(item("/api/v2/data/revisions", id)?)
                }
                Revision::Register => {
                    Request::write::<DatasetRegister, CommandResult<DatasetView>>(
                        POST,
                        "/api/v2/data/revisions",
                        200,
                        true,
                    )?
                }
                Revision::List {
                    source_id,
                    partition,
                    page,
                } => {
                    let mut request =
                        Request::get::<Page<DatasetView>>("/api/v2/data/revisions").page(page)?;
                    if let Some(source) = source_id {
                        request
                            .query
                            .push(("source_id".into(), id(source)?.to_string()));
                    }
                    if let Some(partition) = partition {
                        let value: contracts::research::DataPartition =
                            serde_json::from_value(serde_json::Value::String(partition))
                                .map_err(|_| Failure::Input)?;
                        request
                            .query
                            .push(("partition".into(), value.code().into()));
                    }
                    request
                }
            },
            Self::Data(Data::Universe(command)) => match command {
                Universe::List(page) => {
                    Request::get::<Page<UniverseView>>("/api/v2/data/universes").page(page)?
                }
                Universe::Show { id } => {
                    Request::get::<UniverseView>(item("/api/v2/data/universes", id)?)
                }
            },
            Self::Runtime(command) => match command {
                Runtime::List(page) => {
                    Request::get::<Page<RuntimeView>>("/api/v2/integrations/runtimes").page(page)?
                }
                Runtime::Show { id } => {
                    Request::get::<RuntimeView>(item("/api/v2/integrations/runtimes", id)?)
                }
                Runtime::Create => Request::write::<RuntimeCreate, CommandResult<RuntimeView>>(
                    POST,
                    "/api/v2/integrations/runtimes",
                    201,
                    true,
                )?,
                Runtime::Update { id } => {
                    Request::write::<RuntimeUpdate, CommandResult<RuntimeView>>(
                        PATCH,
                        item("/api/v2/integrations/runtimes", id)?,
                        200,
                        true,
                    )?
                }
                Runtime::Probe { id } => {
                    Request::write::<RuntimeProbeRequestV1, CommandResult<RuntimeProbeViewV1>>(
                        POST,
                        action("/api/v2/integrations/runtimes", id, "probe")?,
                        200,
                        true,
                    )?
                }
                Runtime::Readiness { id } => Request::get::<RuntimeReadinessV1>(action(
                    "/api/v2/integrations/runtimes",
                    id,
                    "readiness",
                )?),
            },
            Self::Codex(command) => match command {
                Codex::List(page) => {
                    Request::get::<Page<CodexProfileViewV1>>("/api/v2/settings/codex").page(page)?
                }
                Codex::Show { id } => {
                    Request::get::<CodexProfileViewV1>(item("/api/v2/settings/codex", id)?)
                }
                Codex::Create => Request::write::<
                    CodexProfileCreateV1,
                    CommandResult<CodexProfileViewV1>,
                >(POST, "/api/v2/settings/codex", 201, true)?,
                Codex::Update { id } => Request::write::<
                    CodexProfileUpdateV1,
                    CommandResult<CodexProfileViewV1>,
                >(
                    PATCH, item("/api/v2/settings/codex", id)?, 200, true
                )?,
                Codex::Probe { id: selected } => {
                    let selected = id(selected)?;
                    let request = Request::write::<
                        CodexProbeRequestV1,
                        CommandResult<CodexProbeViewV1>,
                    >(POST, "/api/v2/codex/probe", 200, true)?;
                    let body: CodexProbeRequestV1 =
                        serde_json::from_slice(request.body.as_ref().ok_or(Failure::Input)?)
                            .map_err(|_| Failure::Input)?;
                    if body.profile_id != selected {
                        return Err(Failure::Input);
                    }
                    request
                }
                Codex::Models { id: selected } => {
                    let mut request = Request::get::<CodexObservationV1>("/api/v2/codex/models");
                    request
                        .query
                        .push(("profile_id".into(), id(selected)?.to_string()));
                    request
                }
                Codex::Account { id: selected } => {
                    let mut request = Request::get::<CodexObservationV1>("/api/v2/codex/account");
                    request
                        .query
                        .push(("profile_id".into(), id(selected)?.to_string()));
                    request
                }
                Codex::Homes => Request::get::<Vec<CodexHomeBindingV1>>("/api/v2/codex/homes"),
                Codex::Login => Request::write::<CodexAccountRequestV1, CodexAccountStartV1>(
                    POST,
                    "/api/v2/codex/login/start",
                    202,
                    true,
                )?,
                Codex::Logout => Request::write::<CodexAccountRequestV1, CodexAccountStartV1>(
                    POST,
                    "/api/v2/codex/logout",
                    202,
                    true,
                )?,
                Codex::LoginCancel => Request::write::<
                    CodexLoginCancelV1,
                    CommandResult<CodexAccountOperationV1>,
                >(
                    POST, "/api/v2/codex/login/cancel", 202, true
                )?,
                Codex::LoginStatus { id } => {
                    Request::get::<CodexAccountOperationV1>(item("/api/v2/codex/login", id)?)
                }
            },
            Self::Downstream(command) => match command {
                Downstream::List(page) => {
                    Request::get::<Page<DownstreamView>>("/api/v2/integrations/downstreams")
                        .page(page)?
                }
                Downstream::Show { id } => {
                    Request::get::<DownstreamView>(item("/api/v2/integrations/downstreams", id)?)
                }
                Downstream::Create => Request::write::<
                    DownstreamCreate,
                    CommandResult<DownstreamView>,
                >(
                    POST, "/api/v2/integrations/downstreams", 201, true
                )?,
                Downstream::Update { id } => {
                    Request::write::<DownstreamUpdate, CommandResult<DownstreamView>>(
                        PATCH,
                        item("/api/v2/integrations/downstreams", id)?,
                        200,
                        true,
                    )?
                }
                Downstream::Probe { id } => Request::write::<
                    contracts::delivery::DownstreamProbeRequestV1,
                    CommandResult<contracts::delivery::DownstreamProbeViewV1>,
                >(
                    POST,
                    action("/api/v2/integrations/downstreams", id, "probe")?,
                    200,
                    true,
                )?,
                Downstream::Readiness { id } => {
                    Request::get::<contracts::delivery::DownstreamReadinessV1>(action(
                        "/api/v2/integrations/downstreams",
                        id,
                        "readiness",
                    )?)
                }
            },
            Self::InputSet(command) => match command {
                InputSet::List(list) => {
                    Request::get::<Page<InputSetSummary>>("/api/v2/input-sets").project(list)?
                }
                InputSet::Show { id } => {
                    Request::get::<InputSetView>(item("/api/v2/input-sets", id)?)
                }
                InputSet::Create => Request::write::<InputSetCreate, CommandResult<InputSetView>>(
                    POST,
                    "/api/v2/input-sets",
                    201,
                    true,
                )?,
            },
            Self::Policy(command) => match command {
                Policy::List(list) => {
                    Request::get::<Page<EvaluationPolicyView>>("/api/v2/evaluation-policies")
                        .project(list)?
                }
                Policy::Show { id } => {
                    Request::get::<EvaluationPolicyView>(item("/api/v2/evaluation-policies", id)?)
                }
                Policy::Create => Request::write::<
                    EvaluationPolicyCreate,
                    CommandResult<EvaluationPolicyView>,
                >(POST, "/api/v2/evaluation-policies", 201, true)?,
            },
            Self::Experiment(command) => match command {
                Experiment::List(list) => {
                    Request::get::<Page<ExperimentView>>("/api/v2/experiments").project(list)?
                }
                Experiment::Show { id } => {
                    Request::get::<ExperimentView>(item("/api/v2/experiments", id)?)
                }
                Experiment::Propose => Request::write::<
                    ExperimentProposalV1,
                    CommandResult<ExperimentView>,
                >(POST, "/api/v2/experiments", 201, false)?,
            },
            Self::Alpha(command) => {
                match command {
                    Alpha::List(list) => {
                        Request::get::<Page<AlphaView>>("/api/v2/alphas").project(list)?
                    }
                    Alpha::Evaluate { id } => {
                        Request::write::<AlphaEvaluateRequestV1, CommandResult<RunSnapshotV1>>(
                            POST,
                            action("/api/v2/alpha-versions", id, "evaluations")?,
                            202,
                            true,
                        )?
                    }
                    Alpha::Calibration { id } => Request::get::<CalibrationView>(action(
                        "/api/v2/alpha-versions",
                        id,
                        "calibration",
                    )?),
                    Alpha::Qualifications { id, page } => Request::get::<Page<QualificationView>>(
                        action("/api/v2/alpha-versions", id, "qualifications")?,
                    )
                    .page(page)?,
                    Alpha::Versions { id, page } => Request::get::<Page<AlphaVersionView>>(action(
                        "/api/v2/alphas",
                        id,
                        "versions",
                    )?)
                    .page(page)?,
                    Alpha::Show { id, version } => {
                        let version: contracts::Revision =
                            version.try_into().map_err(|_| Failure::Input)?;
                        Request::get::<AlphaVersionView>(format!(
                            "{}/{}",
                            action("/api/v2/alphas", id, "versions")?,
                            String::from(version)
                        ))
                    }
                    Alpha::Evaluations { id, page } => Request::get::<Page<EvaluationView>>(
                        action("/api/v2/alpha-versions", id, "evaluations")?,
                    )
                    .page(page)?,
                }
            }
            Self::Evidence(command) => match command {
                Evidence::Show { id } => {
                    Request::get::<EvaluationView>(item("/api/v2/evaluations", id)?)
                }
                Evidence::Metrics { id, page } => Request::get::<Page<MetricValueV1>>(action(
                    "/api/v2/evaluations",
                    id,
                    "metrics",
                )?)
                .page(page)?,
            },
            Self::Artifact(command) => match command {
                Artifact::List(list) => {
                    Request::get::<Page<ArtifactView>>("/api/v2/artifacts").project(list)?
                }
                Artifact::Show { id } => {
                    Request::get::<ArtifactView>(item("/api/v2/artifacts", id)?)
                }
                Artifact::Submit => Request::write::<ArtifactCreate, CommandResult<ArtifactView>>(
                    POST,
                    "/api/v2/artifacts",
                    201,
                    false,
                )?,
                Artifact::Export { id: raw } => {
                    let id = id(raw)?;
                    Request {
                        method: GET,
                        route: format!("/api/v2/artifacts/{id}/content"),
                        query: vec![],
                        body: None,
                        status: 200,
                        operator: false,
                        output: Output::Binary { id },
                    }
                }
            },
            Self::Run(command) => match command {
                Run::Show { id } => Request::get::<RunSnapshotV1>(item("/api/v2/runs", id)?),
                Run::Rebalance { id } => Request::get::<contracts::runs::RunRebalanceViewV1>(
                    action("/api/v2/runs", id, "rebalance")?,
                ),
                Run::Cancel { id } => Request::write::<RunCancelV1, CommandResult<RunSnapshotV1>>(
                    POST,
                    action("/api/v2/runs", id, "cancel")?,
                    202,
                    false,
                )?,
                Run::List {
                    project_id,
                    state,
                    page,
                } => {
                    let project_id = project_id.map(id).transpose()?;
                    let state = state
                        .map(|value| {
                            serde_json::from_value(serde_json::Value::String(value))
                                .map_err(|_| Failure::Input)
                        })
                        .transpose()?;
                    let query = RunListQuery {
                        project_id,
                        state,
                        cursor: page.cursor.map(id).transpose()?,
                        limit: page.limit,
                    };
                    let mut request = Request::get::<Page<RunSnapshotV1>>("/api/v2/runs");
                    request
                        .query
                        .push(("limit".into(), query.limit.to_string()));
                    if let Some(project) = query.project_id {
                        request
                            .query
                            .push(("project_id".into(), project.to_string()));
                    }
                    if let Some(cursor) = query.cursor {
                        request.query.push(("cursor".into(), cursor.to_string()));
                    }
                    if let Some(state) = query.state {
                        let wire = serde_json::to_value(state).map_err(|_| Failure::Input)?;
                        request.query.push((
                            "state".into(),
                            wire.as_str().ok_or(Failure::Input)?.to_owned(),
                        ));
                    }
                    request
                }
                Run::Watch {
                    id: run,
                    after,
                    max_seconds,
                    max_events,
                } => {
                    let run = id(run)?;
                    Request {
                        method: GET,
                        route: format!("/api/v2/runs/{run}/events"),
                        query: vec![],
                        body: None,
                        status: 200,
                        operator: false,
                        output: Output::Events {
                            run,
                            after,
                            seconds: max_seconds,
                            events: max_events,
                        },
                    }
                }
            },
            Self::OperatorGrant => Request::write::<
                OperatorGrantRequest,
                CommandResult<OperatorGrantView>,
            >(
                POST, "/api/v2/auth/operator-command-grants", 201, false
            )?,
            Self::CredentialRegister => Request::write::<
                IntegrationSecretCreate,
                CommandResult<IntegrationSecretView>,
            >(
                POST, "/api/v2/settings/credentials", 201, true
            )?,
        };
        Ok(result)
    }
}
