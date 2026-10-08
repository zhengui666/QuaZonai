//! Closed CLI routing over the shared Rust request/response types.
use super::{Failure, Result};
use clap::{Args, Subcommand};
use contracts::{
    artifacts::{ArtifactCreate, ArtifactView},
    brief::{BriefCreate, BriefUpdate, BriefView},
    codex::{
        CodexAccountOperationV1, CodexAccountRequestV1, CodexAccountStartV1, CodexLoginCancelV1,
        CodexObservationV1, CodexProbeRequestV1, CodexProbeViewV1, CodexProfileUpdateV1,
        CodexProfileViewV1,
    },
    control::{
        CommandResult, OperatorGrantRequest, OperatorGrantView, Page, ProjectCreate, ProjectUpdate,
        ProjectView,
    },
    cycles::{
        BriefFreezeV1, CycleFinishExternalV1, CycleSelectionTrialV1, CycleSelectionV1,
        CycleStartV1, CycleStartedV1, CycleViewV1, ExternalCycleStartV1, FrozenBriefV1,
    },
    data::*,
    evidence::{
        AlphaEvaluateRequestV1, AlphaView, CalibrationView, EvaluationView, MetricValueV1,
        QualificationView,
    },
    execution_assumptions::{ExecutionAssumptionsCreateV1, ExecutionAssumptionsViewV1},
    experiments::{ExperimentEvaluateV1, ExperimentProposalV1, ExperimentView},
    forward::{DownstreamWeightsSubmitV1, DownstreamWeightsViewV1},
    lifecycle::{RunCancelV1, RunListQuery},
    research::{
        EvaluationPolicyCreate, EvaluationPolicyView, InputSetCreate, InputSetSummary, InputSetView,
    },
    runs::RunSnapshotV1,
    runtime::{RuntimeProbeRequestV1, RuntimeProbeViewV1, RuntimeReadinessV1},
    settings::*,
    strategy_portfolio::{
        AlphaVersionEnvelopeV2, MandateCreateEnvelopeV2, MandateViewEnvelopeV2,
        PortfolioBuildEnvelopeV2, PortfolioCandidateEnvelopeV2, PortfolioCandidateListEnvelopeV2,
        StrategyAlphaAdoptV1, StrategyAlphaVersionV1,
    },
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
    /// Log in interactively with the frontend address and password; remember this machine.
    Login {
        /// Device label shown in authentication settings (defaults to the native hostname).
        #[arg(long)]
        name: Option<String>,
        /// Replace the saved connection; the previous device remains managed in settings.
        #[arg(long)]
        replace: bool,
    },
    /// Read the current device or scoped machine identity; never returns a token.
    Identity,
    #[command(subcommand)]
    Migrate(Migrate),
    /// Compatibility spelling for `forward weights submit`.
    #[command(hide = true)]
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
    /// Read the exact local CLI command grant request from stdin.
    /// This does not give a scoped CLI credential lasting Operator authority.
    OperatorGrant,
    /// Write-only IntegrationSecretCreate from stdin; prints only its native reference.
    CredentialRegister,
}
#[derive(Subcommand)]
pub enum Alpha {
    List(ProjectList),
    /// Read qualification history for one AlphaVersionEnvelopeV2.id, not the parent Alpha ID.
    Qualifications {
        #[arg(value_name = "ALPHA_VERSION_ID")]
        id: String,
        #[command(flatten)]
        page: List,
    },
    /// Evaluate one AlphaVersionEnvelopeV2.id using the exact authorized native request.
    Evaluate {
        #[arg(value_name = "ALPHA_VERSION_ID")]
        id: String,
    },
    /// Read calibration for one AlphaVersionEnvelopeV2.id, not the parent Alpha ID.
    Calibration {
        #[arg(value_name = "ALPHA_VERSION_ID")]
        id: String,
    },
    /// List versions of an Alpha entity; use the returned version IDs for evidence reads.
    Versions {
        #[arg(value_name = "ALPHA_ID")]
        id: String,
        #[command(flatten)]
        page: List,
    },
    /// Resolve an Alpha entity and decimal version number to its AlphaVersionEnvelopeV2.
    Show {
        #[arg(value_name = "ALPHA_ID")]
        id: String,
        version: String,
    },
    /// Read evaluation history for one AlphaVersionEnvelopeV2.id, not the parent Alpha ID.
    Evaluations {
        #[arg(value_name = "ALPHA_VERSION_ID")]
        id: String,
        #[command(flatten)]
        page: List,
    },
}

#[derive(Subcommand)]
pub enum Forward {
    /// Submit and read native Paper/Live account observations with an existing identity.
    #[command(subcommand)]
    Accounts(ForwardAccounts),
    Observations {
        id: String,
        #[command(flatten)]
        page: List,
    },
    Wakes {
        id: String,
        #[command(flatten)]
        page: List,
    },
    /// Submit or inspect target-only weights.
    #[command(
        args_conflicts_with_subcommands = true,
        subcommand_negates_reqs = true,
        subcommand_precedence_over_arg = true
    )]
    Weights {
        #[command(subcommand)]
        command: Option<ForwardWeights>,
        /// Compatibility spelling for `weights list PROJECT_ID`.
        #[arg(required = true)]
        project_id: Option<String>,
        #[command(flatten)]
        page: List,
    },
    #[command(subcommand)]
    Messages(ForwardMessages),
    /// Compatibility spelling for `forward messages submit`.
    #[command(hide = true)]
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
pub enum ForwardAccounts {
    /// Preview and control managed-capital exits; this never withdraws funds.
    #[command(subcommand)]
    Exits(CapitalExits),
    /// Relay retained native envelopes in order, preserving original bytes and replay identity.
    Relay(super::account_transport::Arguments),
    /// Submit one original native snapshot/heartbeat envelope from stdin. Retry unchanged.
    Submit,
    Sources {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
    Current {
        project_id: String,
        source_id: String,
    },
    History {
        project_id: String,
        source_id: String,
        #[command(flatten)]
        page: List,
    },
}

#[derive(Subcommand)]
pub enum CapitalExits {
    /// Request an evidence-bound server preview from stdin (no execution).
    /// The generic --preview only validates local transport syntax and sends nothing.
    Preview { #[arg(long)] project_id: String },
    /// Start the exact approved preview from stdin; may activate native reductions.
    /// A 202 receipt means requested, not cash released or withdrawal completed.
    Start { #[arg(long)] project_id: String },
    List(ProjectList),
    Show { id: String },
    /// Pause new reductions and resolve exit-owned pending orders; reserve remains.
    Pause { id: String },
    /// Cancel only future exit work; filled trades remain and reserve is not reinvested.
    Cancel { id: String },
    /// Continue remaining reductions using a fresh approved preview; never buy back.
    Resume { id: String },
    /// Report your own withdrawal for native verification; never performs a transfer.
    Reconcile { id: String },
}

impl CapitalExits {
    fn request(self) -> Result<Request> {
        use contracts::capital_exit::*;
        match self {
            Self::Preview { project_id } => Request::write::<CapitalExitPreviewRequestV1, CommandResult<CapitalExitPreviewV1>>(
                Method::POST, action("/api/v2/projects", project_id, "capital-exit-previews")?, 201, true),
            Self::Start { project_id } => Request::write::<CapitalExitStartV1, CommandResult<CapitalExitViewV1>>(
                Method::POST, action("/api/v2/projects", project_id, "capital-exits")?, 202, true),
            Self::List(list) => Request::get::<Page<CapitalExitViewV1>>(action("/api/v2/projects", list.project_id, "capital-exits")?).page(list.page),
            Self::Show { id } => Ok(Request::get::<CapitalExitViewV1>(item("/api/v2/capital-exits", id)?)),
            Self::Pause { id } => Self::action_input(id, "pause", "PAUSE", std::io::stdin().lock()),
            Self::Cancel { id } => Self::action_input(id, "cancel", "CANCEL", std::io::stdin().lock()),
            Self::Resume { id } => Self::action_input(id, "resume", "RESUME", std::io::stdin().lock()),
            Self::Reconcile { id } => Self::action_input(id, "reconcile-withdrawal", "RECONCILE_WITHDRAWAL", std::io::stdin().lock()),
        }
    }
    fn action_input(id: String, suffix: &str, expected_action: &str, input: impl Read) -> Result<Request> {
        use contracts::capital_exit::{CapitalExitActionV1, CapitalExitViewV1};
        let request = Request::write_input::<CapitalExitActionV1, CommandResult<CapitalExitViewV1>>(
            Method::POST, action("/api/v2/capital-exits", id, suffix)?, 202, true, input)?;
        let body: CapitalExitActionV1 = serde_json::from_slice(request.body.as_deref().ok_or(Failure::Input)?)
            .map_err(|_| Failure::Input)?;
        if body.action() != expected_action { return Err(Failure::Input); }
        Ok(request)
    }
}

#[derive(Subcommand)]
pub enum ForwardWeights {
    /// Submit target-only weights using the authenticated downstream identity.
    Submit,
    List {
        project_id: String,
        #[command(flatten)]
        page: List,
    },
}

#[derive(Subcommand)]
pub enum ForwardMessages {
    /// Submit the original downstream forward report.
    Submit,
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
    /// Claim only target package version 2; the response identifies its Forecast or Native source.
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
    /// Read the accepted strategy replay preview or current target-only outcome.
    Summary {
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
    /// Read the original frozen Brief and execution context without replaying a write.
    ExecutionContext {
        id: String,
    },
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
    /// Start a frozen research cycle for an external Agent, without internal profiles.
    StartExternal {
        project_id: String,
    },
    /// Close a settled external research batch; unexecuted proposals stay historical.
    FinishExternal {
        id: String,
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
    #[command(subcommand)]
    Features(RecordedFeatures),
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
    /// Read owner-only registration evidence summary for a non-Sealed Dataset.
    Evidence {
        id: String,
    },
    Register,
}
#[derive(Subcommand)]
pub enum RecordedFeatures {
    /// Read registered original feature parts for one project and Dataset.
    List {
        dataset_revision_id: String,
        #[arg(long)]
        project_id: String,
    },
    /// Register original recorded feature bytes against immutable native Dataset metadata.
    Register { dataset_revision_id: String },
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
    /// Read CodexProfileUpdateV1 on stdin; requires CAS and, for scoped credentials, an Operator grant.
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
    Show {
        id: String,
    },
    Propose,
    /// Queue independent native research execution under the original trial budget.
    Evaluate {
        id: String,
    },
    /// Freeze one original accepted fold as a reusable research target policy.
    AdoptAlpha {
        id: String,
    },
    /// Read bounded native statistics and display-only equity for separate fresh-capital folds.
    Summary {
        id: String,
    },
    /// Read the original adopted independent-fold native trading report.
    Result {
        id: String,
    },
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
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        max_seconds: Option<u32>,
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        max_events: Option<u64>,
    },
}

pub(super) enum Output {
    Json(fn(&[u8]) -> Result<serde_json::Value>),
    NativeReport(fn(&[u8]) -> Result<serde_json::Value>),
    Binary {
        id: Id,
        report: Option<Id>,
    },
    Events {
        run: Id,
        after: Option<String>,
        seconds: Option<u32>,
        events: Option<u64>,
    },
}
#[derive(Subcommand)]
pub enum Migrate {
    ArtifactSummary {
        id: String,
    },
    Artifacts {
        id: String,
        #[command(flatten)]
        page: List,
    },
    Artifact {
        id: String,
        record: String,
    },
    Download {
        id: String,
        record: String,
    },
    Fields {
        id: String,
        record: String,
    },
    Field {
        id: String,
        record: String,
        name: String,
        #[arg(long, default_value = "0")]
        offset: String,
    },
    Reports(List),
    Source {
        id: String,
    },
    Mappings {
        id: String,
        #[command(flatten)]
        page: List,
    },
    /// Import a deployment-registered historical projection; scoped credentials require an exact Operator grant.
    Import {
        #[arg(long)]
        export_ref: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Read an import report within the current identity's authority.
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
fn read_input<T: DeserializeOwned + Serialize>(mut input: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    input
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::Input)?;
    if bytes.is_empty() {
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
    pub(super) fn requires_idempotency_key(&self) -> bool {
        self.method != Method::GET
            && !(self.method == Method::POST
                && self.route == "/api/v2/forward/account-observations")
    }

    pub(super) fn account_observation(
        bytes: &[u8],
    ) -> Result<(
        Self,
        contracts::account_observation::AccountObservationSubmitV1,
    )> {
        use contracts::account_observation::{
            AccountObservationReceiptV1, AccountObservationSubmitV1,
        };
        let observation: AccountObservationSubmitV1 =
            serde_json::from_slice(bytes).map_err(|_| Failure::Input)?;
        Ok((
            Self {
                method: Method::POST,
                route: "/api/v2/forward/account-observations".into(),
                query: vec![],
                body: Some(bytes.to_vec()),
                status: 201,
                operator: false,
                output: Output::Json(decode::<AccountObservationReceiptV1>),
            },
            observation,
        ))
    }

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
    fn native_report<T: DeserializeOwned + Serialize>(route: impl Into<String>) -> Self {
        let mut request = Self::get::<T>(route);
        request.output = Output::NativeReport(decode::<T>);
        request
    }
    fn write<T: DeserializeOwned + Serialize, R: DeserializeOwned + Serialize>(
        method: Method,
        route: impl Into<String>,
        status: u16,
        operator: bool,
    ) -> Result<Self> {
        Self::write_input::<T, R>(method, route, status, operator, std::io::stdin().lock())
    }
    // The same typed input gate is used before previews and real sends.
    // In particular, historical target versions cannot become a Request body.
    fn write_input<T: DeserializeOwned + Serialize, R: DeserializeOwned + Serialize>(
        method: Method,
        route: impl Into<String>,
        status: u16,
        operator: bool,
        input: impl Read,
    ) -> Result<Self> {
        Ok(Self {
            method,
            route: route.into(),
            query: vec![],
            body: Some(read_input::<T>(input)?),
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

impl ForwardAccounts {
    fn request(self) -> Result<Request> {
        use contracts::account_observation::*;
        Ok(match self {
            Self::Exits(command) => return command.request(),
            Self::Relay(_) => return Err(Failure::Input),
            Self::Submit => {
                Request::write::<AccountObservationSubmitV1, AccountObservationReceiptV1>(
                    Method::POST,
                    "/api/v2/forward/account-observations",
                    201,
                    false,
                )?
            }
            Self::Sources { project_id, page } => Request::get::<Page<AccountSourceV1>>(action(
                "/api/v2/projects",
                project_id,
                "account-sources",
            )?)
            .page(page)?,
            Self::Current {
                project_id,
                source_id,
            } => {
                let base = action("/api/v2/projects", project_id, "account-sources")?;
                Request::get::<AccountCurrentV1>(action(&base, source_id, "current")?)
            }
            Self::History {
                project_id,
                source_id,
                page,
            } => {
                let base = action("/api/v2/projects", project_id, "account-sources")?;
                Request::get::<Page<AccountObservationV1>>(action(
                    &base,
                    source_id,
                    "observations",
                )?)
                .page(page)?
            }
        })
    }
}

impl Command {
    pub(super) fn request_for(self, device: bool) -> Result<Request> {
        if device && matches!(self, Self::Identity) {
            return Ok(Request::get::<contracts::auth::CliDevice>(
                "/api/v2/auth/cli/session",
            ));
        }
        let mut request = self.request()?;
        if device {
            request.operator = false;
        }
        Ok(request)
    }

    pub(super) fn request(self) -> Result<Request> {
        const GET: Method = Method::GET;
        const PATCH: Method = Method::PATCH;
        const POST: Method = Method::POST;
        let result = match self {
            Self::Login { .. } => return Err(Failure::Input),
            Self::Identity => {
                Request::get::<contracts::control::MachineSessionView>("/api/v2/auth/machine")
            }
            Self::Migrate(command) => match command {
                Migrate::ArtifactSummary { id: report } => {
                    Request::get::<contracts::imports::HistoricalArtifactSummaryV1>(format!(
                        "/api/v2/migrations/reports/{}/artifacts/summary",
                        id(report)?
                    ))
                }
                Migrate::Artifacts { id: report, page } => {
                    Request::get::<Page<contracts::imports::HistoricalArtifactResultV1>>(format!(
                        "/api/v2/migrations/reports/{}/artifacts",
                        id(report)?
                    ))
                    .page(page)?
                }
                Migrate::Artifact { id: report, record } => {
                    Request::get::<contracts::imports::HistoricalArtifactResultV1>(format!(
                        "/api/v2/migrations/reports/{}/artifacts/{}",
                        id(report)?,
                        id(record)?
                    ))
                }
                Migrate::Download { id: report, record } => {
                    let report = id(report)?;
                    let record = id(record)?;
                    Request {
                        method: GET,
                        route: format!(
                            "/api/v2/migrations/reports/{report}/artifacts/{record}/content"
                        ),
                        query: vec![],
                        body: None,
                        status: 200,
                        operator: false,
                        output: Output::Binary {
                            id: record,
                            report: Some(report),
                        },
                    }
                }
                Migrate::Fields { id: report, record } => {
                    Request::get::<contracts::imports::HistoricalRecordFieldsV1>(format!(
                        "/api/v2/migrations/reports/{}/records/{}/fields",
                        id(report)?,
                        id(record)?
                    ))
                }
                Migrate::Field {
                    id: report,
                    record,
                    name,
                    offset,
                } => {
                    let offset =
                        contracts::DbCounter::try_from(offset).map_err(|_| Failure::Input)?;
                    if name.is_empty() || name.len() > 63 {
                        return Err(Failure::Input);
                    }
                    let mut request =
                        Request::get::<contracts::imports::HistoricalFieldContentV1>(format!(
                            "/api/v2/migrations/reports/{}/records/{}/field",
                            id(report)?,
                            id(record)?
                        ));
                    request.query = vec![
                        ("name".into(), name),
                        ("offset".into(), String::from(offset)),
                    ];
                    request
                }
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
                Forward::Accounts(command) => command.request()?,
                Forward::Observations { id, page } => {
                    Request::get::<Page<contracts::forward::ForwardObservationViewV1>>(action(
                        "/api/v2/projects",
                        id,
                        "forward-observations",
                    )?)
                    .page(page)?
                }
                Forward::Wakes { id, page } => {
                    Request::get::<Page<contracts::forward::WakeViewV1>>(action(
                        "/api/v2/projects",
                        id,
                        "wakes",
                    )?)
                    .page(page)?
                }
                Forward::Weights {
                    command: Some(ForwardWeights::Submit),
                    ..
                } => return Self::ForwardWeights.request(),
                Forward::Weights {
                    command: Some(ForwardWeights::List { project_id, page }),
                    ..
                }
                | Forward::Weights {
                    command: None,
                    project_id: Some(project_id),
                    page,
                } => Request::get::<Page<contracts::forward::DownstreamWeightsViewV1>>(action(
                    "/api/v2/projects",
                    project_id,
                    "forward-weight-snapshots",
                )?)
                .page(page)?,
                Forward::Weights {
                    command: None,
                    project_id: None,
                    ..
                } => return Err(Failure::Input),
                Forward::Window { id, stream } => {
                    let mut request = Request::get::<contracts::forward::ForwardWindowViewV1>(
                        action("/api/v2/handoffs", id, "forward-window")?,
                    );
                    request.query.push(("stream_id".into(), stream));
                    request
                }
                Forward::Submit | Forward::Messages(ForwardMessages::Submit) => {
                    Request::write::<
                        contracts::forward::ForwardMessageSubmitV1,
                        CommandResult<contracts::forward::ForwardMessageViewV1>,
                    >(POST, "/api/v2/forward/messages", 201, false)?
                }
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
                        CommandResult<contracts::strategy_portfolio::HandoffClaimViewV2>,
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
                    Request::get::<Page<contracts::strategy_portfolio::ReleaseViewEnvelopeV2>>(
                        action("/api/v2/projects", project_id, "releases")?,
                    )
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
                    contracts::strategy_portfolio::ReleaseCreateEnvelopeV2,
                    CommandResult<contracts::strategy_portfolio::ReleaseViewEnvelopeV2>,
                >(POST, "/api/v2/releases", 201, true)?,
                Release::Show { id } => Request::get::<
                    contracts::strategy_portfolio::ReleaseViewEnvelopeV2,
                >(item("/api/v2/releases", id)?),
            },
            Self::Portfolio(Portfolio::Build) => Request::write::<
                PortfolioBuildEnvelopeV2,
                CommandResult<RunSnapshotV1>,
            >(
                POST, "/api/v2/portfolio-builds", 202, true
            )?,
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
                Candidate::List { project_id, page } => {
                    Request::get::<Page<PortfolioCandidateListEnvelopeV2>>(action(
                        "/api/v2/projects",
                        project_id,
                        "portfolio-candidates",
                    )?)
                    .page(page)?
                }
                Candidate::Show { id } => Request::get::<PortfolioCandidateEnvelopeV2>(item(
                    "/api/v2/portfolio-candidates",
                    id,
                )?),
                Candidate::Summary { id } => {
                    Request::get::<contracts::strategy_portfolio::StrategyPortfolioSummaryV1>(
                        action("/api/v2/portfolio-candidates", id, "summary")?,
                    )
                }
                Candidate::Evaluations { id, page } => Request::get::<Page<EvaluationView>>(
                    action("/api/v2/portfolio-candidates", id, "evaluations")?,
                )
                .page(page)?,
            },
            Self::Portfolio(Portfolio::Mandate(command)) => match command {
                Mandate::Create => Request::write::<
                    MandateCreateEnvelopeV2,
                    CommandResult<MandateViewEnvelopeV2>,
                >(POST, "/api/v2/portfolio-mandates", 201, true)?,
                Mandate::List { project_id, page } => Request::get::<Page<MandateViewEnvelopeV2>>(
                    action("/api/v2/projects", project_id, "portfolio-mandates")?,
                )
                .page(page)?,
                Mandate::Show { id } => {
                    Request::get::<MandateViewEnvelopeV2>(item("/api/v2/portfolio-mandates", id)?)
                }
            },
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
                Brief::ExecutionContext { id } => Request::get::<FrozenBriefV1>(action(
                    "/api/v2/briefs",
                    id,
                    "execution-context",
                )?),
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
                    Cycle::FinishExternal { id } => {
                        Request::write::<CycleFinishExternalV1, CommandResult<CycleViewV1>>(
                            POST,
                            action("/api/v2/cycles", id, "finish-external")?,
                            200,
                            true,
                        )?
                    }
                    Cycle::StartExternal { project_id } => {
                        Request::write::<ExternalCycleStartV1, CommandResult<CycleViewV1>>(
                            POST,
                            action("/api/v2/projects", project_id, "cycles/external")?,
                            201,
                            true,
                        )?
                    }
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
            Self::Data(Data::Features(command)) => match command {
                RecordedFeatures::List {
                    dataset_revision_id,
                    project_id,
                } => {
                    let mut request = Request::get::<RecordedFeatureListV1>(action(
                        "/api/v2/data/revisions",
                        dataset_revision_id,
                        "features",
                    )?);
                    request
                        .query
                        .push(("project_id".into(), id(project_id)?.to_string()));
                    request
                }
                RecordedFeatures::Register {
                    dataset_revision_id,
                } => {
                    let dataset = id(dataset_revision_id)?;
                    let request = Request::write::<
                        RecordedFeatureRegisterV1,
                        CommandResult<RecordedFeatureViewV1>,
                    >(
                        POST,
                        format!("/api/v2/data/revisions/{dataset}/features"),
                        201,
                        true,
                    )?;
                    let body: RecordedFeatureRegisterV1 =
                        serde_json::from_slice(request.body.as_deref().ok_or(Failure::Input)?)
                            .map_err(|_| Failure::Input)?;
                    if body.dataset_revision_id != dataset {
                        return Err(Failure::Input);
                    }
                    request
                }
            },
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
                Revision::Evidence { id } => Request::get::<DatasetEvidenceViewV1>(action(
                    "/api/v2/data/revisions",
                    id,
                    "evidence",
                )?),
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
                Experiment::Evaluate { id } => {
                    Request::write::<ExperimentEvaluateV1, CommandResult<RunSnapshotV1>>(
                        POST,
                        action("/api/v2/experiments", id, "evaluate")?,
                        202,
                        true,
                    )?
                }
                Experiment::AdoptAlpha { id } => {
                    Request::write::<StrategyAlphaAdoptV1, CommandResult<StrategyAlphaVersionV1>>(
                        POST,
                        action("/api/v2/experiments", id, "adopt-alpha")?,
                        201,
                        true,
                    )?
                }
                Experiment::Summary { id } => {
                    Request::get::<contracts::experiment_summary::ExperimentSummaryV1>(action(
                        "/api/v2/experiments",
                        id,
                        "summary",
                    )?)
                }
                Experiment::Result { id } => {
                    Request::native_report::<contracts::science::NativeExperimentEvaluationResultV1>(
                        action("/api/v2/experiments", id, "evaluation")?,
                    )
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
                    Alpha::Versions { id, page } => Request::get::<Page<AlphaVersionEnvelopeV2>>(
                        action("/api/v2/alphas", id, "versions")?,
                    )
                    .page(page)?,
                    Alpha::Show { id, version } => {
                        let version: contracts::Revision =
                            version.try_into().map_err(|_| Failure::Input)?;
                        Request::get::<AlphaVersionEnvelopeV2>(format!(
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
                        output: Output::Binary { id, report: None },
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

#[cfg(test)]
mod brief_read_tests {
    use super::*;
    use clap::Parser;
    use serde_json::{json, Value};

    const BRIEF: &str = "018fc823-8e40-7000-8000-000000000001";
    const PROJECT: &str = "018fc823-8e40-7000-8000-000000000002";

    #[derive(Parser)]
    struct Arguments {
        #[command(subcommand)]
        command: Command,
    }

    fn read(command: &str, id: &str) -> Result<Request> {
        Arguments::try_parse_from(["client", "brief", command, id])
            .map_err(|_| Failure::Input)?
            .command
            .request()
    }

    fn frozen() -> Value {
        let create: Value = serde_json::from_str(include_str!(
            "../../../../tests/contracts/research-brief.json"
        ))
        .unwrap();
        json!({
            "schema_version": 1,
            "brief": {
                "id": BRIEF, "project_id": PROJECT, "version": 1,
                "revision": "9007199254740993", "state": "FROZEN",
                "content": create["content"], "bindings": create["bindings"],
                "supersedes_id": null,
                "frozen_at": "2026-09-01T00:00:00Z",
                "created_at": "2026-08-31T00:00:00Z",
                "updated_at": "2026-09-01T00:00:00Z"
            },
            "execution_context": {
                "schema_version": 1,
                "runtime_id": "018fc823-8e40-7000-8000-000000000003",
                "runtime_revision": "9007199254740993",
                "discovery_input_set_id": "018fc823-8e40-7000-8000-000000000004",
                "validation_input_set_id": "018fc823-8e40-7000-8000-000000000005",
                "sealed_input_set_id": "018fc823-8e40-7000-8000-000000000006"
            }
        })
    }

    #[test]
    fn execution_context_is_an_existing_read_only_route() {
        let request = read("execution-context", BRIEF).unwrap();
        assert_eq!(request.method, Method::GET);
        assert_eq!(
            request.route,
            format!("/api/v2/briefs/{BRIEF}/execution-context")
        );
        assert_eq!(request.status, 200);
        assert!(!request.operator);
        assert!(!request.requires_idempotency_key());
        assert!(request.query.is_empty());
        assert!(request.body.is_none());
        assert!(read("execution-context", "not-a-uuid").is_err());
    }

    #[test]
    fn execution_context_preserves_original_frozen_ids_and_revisions() {
        let request = read("execution-context", BRIEF).unwrap();
        let Output::Json(decode) = request.output else {
            panic!("JSON response required")
        };
        let response = frozen();
        assert_eq!(
            decode(&serde_json::to_vec(&response).unwrap()).unwrap(),
            response
        );
        let mut missing = response.clone();
        missing.as_object_mut().unwrap().remove("execution_context");
        assert!(matches!(
            decode(&serde_json::to_vec(&missing).unwrap()),
            Err(Failure::Contract)
        ));
        let mut invalid = response.clone();
        invalid["execution_context"]["runtime_revision"] = json!(9007199254740993_u64);
        assert!(matches!(
            decode(&serde_json::to_vec(&invalid).unwrap()),
            Err(Failure::Contract)
        ));
        invalid = response;
        invalid["execution_context"]["native_path"] = json!("/untrusted");
        assert!(matches!(
            decode(&serde_json::to_vec(&invalid).unwrap()),
            Err(Failure::Contract)
        ));
    }

    #[test]
    fn brief_show_keeps_its_original_view_contract() {
        let request = read("show", BRIEF).unwrap();
        assert_eq!(request.route, format!("/api/v2/briefs/{BRIEF}"));
        let Output::Json(decode) = request.output else {
            panic!("JSON response required")
        };
        let response = frozen();
        assert_eq!(
            decode(&serde_json::to_vec(&response["brief"]).unwrap()).unwrap(),
            response["brief"]
        );
        assert!(matches!(
            decode(&serde_json::to_vec(&response).unwrap()),
            Err(Failure::Contract)
        ));
    }
}

#[cfg(test)]
mod dataset_evidence_tests {
    use super::*;
    use clap::Parser;
    const ID: &str = "018fc823-8e40-7000-8000-000000000001";
    #[derive(Parser)]
    struct Arguments {
        #[command(subcommand)]
        command: Command,
    }
    fn request(id: &str) -> Result<Request> {
        Arguments::try_parse_from(["client", "data", "revision", "evidence", id])
            .map_err(|_| Failure::Input)?
            .command
            .request()
    }
    #[test]
    fn dataset_evidence_routes_only_a_revision_id_with_no_write_or_artifact_fallback() {
        let request = request(ID).unwrap();
        assert_eq!(request.method, Method::GET);
        assert_eq!(
            request.route,
            format!("/api/v2/data/revisions/{ID}/evidence")
        );
        assert_eq!(request.status, 200);
        assert!(!request.operator);
        assert!(!request.requires_idempotency_key());
        assert!(request.query.is_empty());
        assert!(request.body.is_none());
        assert!(super::dataset_evidence_tests::request("../artifacts/other").is_err());
    }
    #[test]
    fn dataset_evidence_decodes_strict_typed_summary_and_preserves_labels_and_integer_strings() {
        let Output::Json(decode) = request(ID).unwrap().output else {
            panic!("JSON response")
        };
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../tests/contracts/dataset-evidence.json"
        ))
        .unwrap();
        assert_eq!(decode(&serde_json::to_vec(&value).unwrap()).unwrap(), value);
        for field in [
            "native_metadata_artifact_id",
            "license_state",
            "pit_status",
            "quality",
        ] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(decode(&serde_json::to_vec(&missing).unwrap()).is_err());
        }
        let mut raw = value.clone();
        raw["provenance_reference"] = serde_json::json!("https://must-not-leak.example");
        assert!(decode(&serde_json::to_vec(&raw).unwrap()).is_err());
        let mut wrong_version = value.clone();
        wrong_version["schema_version"] = serde_json::json!(2);
        assert!(decode(&serde_json::to_vec(&wrong_version).unwrap()).is_err());
        let mut number = value;
        number["quality"]["row_count"] = serde_json::json!(9007199254740993_u64);
        assert!(decode(&serde_json::to_vec(&number).unwrap()).is_err());
    }
}

#[cfg(test)]
#[path = "../../../../tests/support/execution_models.rs"]
mod target_delivery_execution_models;

#[cfg(test)]
mod target_delivery_v2_tests {
    use super::target_delivery_execution_models as execution_models;
    use super::*;
    use contracts::{delivery::HandoffClaimV1, strategy_portfolio::HandoffClaimViewV2};
    use serde_json::{json, Value};

    const ID: &str = "018fc823-8e40-7000-8000-000000000001";
    const TIME: &str = "2026-10-03T00:00:00Z";

    // Controlled wire fixtures only; these do not establish publication eligibility.
    fn cash() -> Value {
        json!({"downstream_id":Id::new(),"trader_id":"TRADER-001","account_id":"BINANCE-001",
        "base_currency":"USDT","starting_capital":"1000","execution_assumptions_id":Id::new()})
    }

    fn v1_package() -> Value {
        let input: Value = serde_json::from_str(include_str!(
            "../../../../tests/contracts/allocation-input.json"
        ))
        .unwrap();
        json!({
            "release_id":Id::new(),"package_schema_version":"1","environment_origin":"DEMO",
            "project_id":Id::new(),"candidate_id":Id::new(),"mandate_id":Id::new(),
            "qualification_refs":[Id::new(),Id::new()],"evaluation_refs":[Id::new()],"input_revision_refs":[Id::new()],
            "engine_versions":{"nautilus":"0.63.0"},"asof":"2026-10-03T00:00:00Z",
            "valid_from":"2026-10-03T00:00:00Z","valid_until":"2026-10-03T00:01:00Z",
            "base_currency":"USD","capital_assumption":"1000","current_weights_source":"NONE",
            "targets":[{"instrument_id":"EXAMPLE.SIM","target_weight":"0.25","currency":"USD"}],
            "cash_weight":"0.75","constraints_summary":input["constraints"],"exposure_tolerance":"0.01",
            "cost_assumption_ref":Id::new(),"compatible_market_capabilities":["fixture/1"],
            "limitations":["synthetic wire fixture"],"provenance_artifact_refs":[Id::new()]
        })
    }

    fn forecast_v2_package() -> Value {
        let mut package = v1_package();
        package["package_schema_version"] = json!("2");
        package["source_kind"] = json!("FORECAST_EVALUATION");
        let dataset = Id::new();
        let metadata = Id::new();
        package["source"] = json!({
            "build_run_id":Id::new(), "build_accepted_attempt_id":Id::new(),
            "build_parameters_artifact_id":Id::new(), "build_report_artifact_id":Id::new(),
            "build_input_set_id":Id::new(), "build_environment":"LIVE",
            "forward_dataset_revision_id":dataset, "forward_metadata_artifact_id":metadata,
            "current_weights_artifact_id":Id::new()
        });
        package["current_weights"] = json!({
            "schema_version":1, "source":{"kind":"LAST_TARGET", "candidate_id":Id::new()},
            "asof_ns":"9007199254740993", "available_ns":"9007199254740994",
            "valid_until_ns":"9007199254740995", "base_currency":"USD", "cash_weight":"0.75",
            "weights":[{"instrument_id":"EXAMPLE.SIM","weight":"0.25","currency":"USD"}]
        });
        package["execution_settings"] = v2_package()["execution_settings"].clone();
        package["forward_dataset"] = json!({
            "dataset_revision_id":dataset,"native_metadata_artifact_id":metadata,
            "storage_version":"original-version", "data_kind":"BAR", "partition":"FORWARD",
            "origin":"SYNTHETIC","pit_status":"UNVERIFIED","revision_policy":"UNKNOWN",
            "event_start":"2026-10-02T23:00:00Z","event_end":"2026-10-03T00:00:00Z",
            "available_through":"2026-10-03T00:00:00Z","row_count":"1",
            "selection":{"schema_version":1,"bar_types":["EXAMPLE.SIM-1-MINUTE-LAST-EXTERNAL"],
                "event_start_ns":"1","event_end_ns":"9007199254740993",
                "decision_cutoff_ns":"9007199254740993","maximum_rows":1},
            "instrument_definitions":[{"CurrencyPair":{"id":"EXAMPLE.SIM","ts_event":0,"ts_init":0,"price_increment":"0.01"}}]
        });
        package
    }

    fn v2_package() -> Value {
        let mut package = v1_package();
        let object = package.as_object_mut().unwrap();
        for name in [
            "environment_origin",
            "qualification_refs",
            "evaluation_refs",
            "current_weights_source",
        ] {
            object.remove(name);
        }
        object.insert("package_schema_version".into(), json!("2"));
        object.insert("source_kind".into(), json!("NATIVE_TARGET_DECISION"));
        object.insert("execution_environment".into(), json!("PAPER"));
        object.insert("account_start".into(), cash());
        object.insert("execution_settings".into(), json!({
        "schema_version":1,"base_currency":"USDT","starting_capital":"1000",
        "account_kind":"MARGIN","leverage":"1","fee_model":execution_models::fee(),
        "fill_model":execution_models::fill(),"latency_model":execution_models::latency(1000000),
        "snapshot_interval_ms":1000,"exposure_tolerance":"0.01",
        "fee_rates":[{"instrument_id":"BTCUSDT.BINANCE","maker":"0","taker":"0.001"}]
    }));
        object.insert(
            "source".into(),
            json!({"run_id":Id::new(),"accepted_attempt_id":Id::new(),
        "report_artifact_id":Id::new(),"alpha_version_ids":[Id::new()],
        "input_provenance":{"dataset_revision_id":Id::new(),"market_data_origin":"REAL",
            "pit_status":"UNVERIFIED","revision_policy":"UNKNOWN","feature_artifact_origins":{}}}),
        );
        package
    }

    fn claim(version: Value) -> Result<Request> {
        let body = serde_json::to_vec(&json!({
            "schema_version":1, "external_claim_id":"original-claim", "package_schema_version":version
        })).unwrap();
        Request::write_input::<HandoffClaimV1, CommandResult<HandoffClaimViewV2>>(
            Method::POST,
            action("/api/v2/handoffs", ID.into(), "claim")?,
            200,
            false,
            body.as_slice(),
        )
    }

    fn handoff() -> Value {
        json!({"id":ID,"project_id":ID,"candidate_id":ID,"mandate_id":ID,"release_id":ID,
            "approval_id":ID,"downstream_id":ID,"environment":"PAPER","delivery_sequence":"1",
            "revision":"1","state":"CLAIMED","supersedes_handoff_id":null,
            "offered_at":TIME,"expires_at":TIME,"claimed_at":TIME,
            "external_claim_id":"original-claim","acknowledged_at":null})
    }

    fn configuration(versions: Value) -> Value {
        json!({"name":"Original downstream","endpoint":"https://downstream.example",
            "accepted_package_versions":versions,"environments":"PAPER","enabled":true,"development_http":false})
    }

    #[test]
    fn target_v2_input_gate_rejects_legacy_claim_before_request_or_preview() {
        for version in [json!("1"), json!(1), json!(2), json!("3"), Value::Null] {
            assert!(matches!(claim(version), Err(Failure::Input)));
        }
        let request = claim(json!("2")).unwrap();
        assert_eq!(request.method, Method::POST);
        assert_eq!(request.route, format!("/api/v2/handoffs/{ID}/claim"));
        assert_eq!(request.status, 200);
        assert!(!request.operator);
        assert!(request.requires_idempotency_key());
        assert_eq!(
            serde_json::from_slice::<Value>(request.body.as_ref().unwrap()).unwrap()
                ["package_schema_version"],
            "2"
        );
        assert!(matches!(
            super::super::preview::inspect(&request, "https://api.example", false, None, None),
            Err(Failure::IdempotencyRequired)
        ));
        let preview = super::super::preview::inspect(
            &request,
            "https://api.example",
            false,
            Some("claim-v2"),
            None,
        )
        .unwrap();
        assert_eq!(preview["request_sent"], false);
        assert_eq!(preview["authorization_checked"], false);
        assert_eq!(preview["requires_idempotency_key"], true);
        assert_eq!(preview["requires_operator_grant"], false);
        assert_eq!(preview["body_redacted"], true);
        assert!(preview.get("body").is_none());
    }

    #[test]
    fn target_v2_cli_claim_keeps_frozen_forecast_and_native_bodies_distinct() {
        let Output::Json(decode) = claim(json!("2")).unwrap().output else {
            panic!("typed claim response")
        };
        for package in [forecast_v2_package(), v2_package()] {
            let response = json!({"schema_version":1,"replayed":true,"resource":{"handoff":handoff(),"package":package}});
            let parsed = decode(&serde_json::to_vec(&response).unwrap()).unwrap();
            assert_eq!(parsed, response);
            assert_eq!(parsed["resource"]["package"]["package_schema_version"], "2");
        }
        let forecast = forecast_v2_package();
        assert_eq!(forecast["source"]["build_environment"], "LIVE");
        assert_eq!(forecast["current_weights"]["asof_ns"], "9007199254740993");
        assert!(forecast.get("account_start").is_none());
        let mut malformed = Vec::from([v1_package()]);
        for source in [
            Value::Null,
            json!("UNKNOWN"),
            json!("NATIVE_TARGET_DECISION"),
        ] {
            let mut crossed = forecast.clone();
            crossed["source_kind"] = source;
            malformed.push(crossed);
        }
        for field in [
            "source_kind",
            "source",
            "forward_dataset",
            "current_weights",
            "execution_settings",
        ] {
            let mut missing = forecast.clone();
            missing.as_object_mut().unwrap().remove(field);
            malformed.push(missing);
        }
        for package in malformed {
            let response = json!({"schema_version":1,"replayed":false,"resource":{"handoff":handoff(),"package":package}});
            assert!(matches!(
                decode(&serde_json::to_vec(&response).unwrap()),
                Err(Failure::Contract)
            ));
        }
    }

    #[test]
    fn target_v2_cli_historical_downstream_reads_never_become_write_defaults() {
        let request = Command::Downstream(Downstream::Show { id: ID.into() })
            .request()
            .unwrap();
        assert_eq!(request.method, Method::GET);
        assert!(!request.requires_idempotency_key());
        let Output::Json(decode) = request.output else {
            panic!("typed historical view")
        };
        for versions in [json!(["1"]), json!(["1", "2"])] {
            let original = configuration(versions);
            let view = json!({"id":ID,"configuration":original,"credential_configured":true,
                "revision":"9007199254740993","created_at":TIME,"updated_at":TIME});
            assert_eq!(decode(&serde_json::to_vec(&view).unwrap()).unwrap(), view);
            let create = serde_json::to_vec(
                &json!({"schema_version":1,"configuration":original,"credential_ref":ID}),
            )
            .unwrap();
            assert!(matches!(
                Request::write_input::<DownstreamCreate, CommandResult<DownstreamView>>(
                    Method::POST,
                    "/api/v2/integrations/downstreams",
                    201,
                    true,
                    create.as_slice()
                ),
                Err(Failure::Input)
            ));
            let update = serde_json::to_vec(&json!({"schema_version":1,"configuration":original,"credential_ref":null,"expected_revision":"9007199254740993"})).unwrap();
            assert!(matches!(
                Request::write_input::<DownstreamUpdate, CommandResult<DownstreamView>>(
                    Method::PATCH,
                    format!("/api/v2/integrations/downstreams/{ID}"),
                    200,
                    true,
                    update.as_slice()
                ),
                Err(Failure::Input)
            ));
        }
        let create = serde_json::to_vec(&json!({"schema_version":1,"configuration":configuration(json!(["2"])),"credential_ref":ID})).unwrap();
        let active = Request::write_input::<DownstreamCreate, CommandResult<DownstreamView>>(
            Method::POST,
            "/api/v2/integrations/downstreams",
            201,
            true,
            create.as_slice(),
        )
        .unwrap();
        assert!(active.operator);
        assert!(active.requires_idempotency_key());
    }

    #[test]
    fn target_v2_cli_release_history_keeps_original_v1_read_only() {
        let request = Command::Release(Release::Show { id: ID.into() })
            .request()
            .unwrap();
        assert_eq!(request.method, Method::GET);
        assert!(!request.requires_idempotency_key());
        assert!(!request.operator);
        assert!(request.body.is_none());
        let Output::Json(decode) = request.output else {
            panic!("typed release view")
        };
        let original = json!({"id":ID,"project_id":ID,"candidate_id":ID,"mandate_id":ID,"evaluation_id":ID,
            "package_artifact_id":ID,"package_schema_version":"1","market_capability_version":"original/1",
            "asof":TIME,"valid_from":TIME,"valid_until":TIME,"environment":"REAL","created_at":TIME});
        assert_eq!(
            decode(&serde_json::to_vec(&original).unwrap()).unwrap(),
            original
        );
        assert!(!PackageSchemaVersion::V1.is_deliverable());
    }
}

#[cfg(test)]
mod capital_exit_cli_tests {
    use super::*;
    const ID: &str = "018fc823-8e40-7000-8000-000000000001";
    #[test]
    fn capital_exit_reads_use_original_paginated_routes() {
        let request = CapitalExits::List(ProjectList { project_id: ID.into(), page: List { cursor: None, limit: 25 } }).request().unwrap();
        assert_eq!(request.route, format!("/api/v2/projects/{ID}/capital-exits"));
        assert_eq!(request.query, [("limit".into(), "25".into())]);
        assert_eq!(request.method, Method::GET);
        assert!(!request.requires_idempotency_key());
        assert_eq!(CapitalExits::Show { id: ID.into() }.request().unwrap().route, format!("/api/v2/capital-exits/{ID}"));
    }
    #[test]
    fn capital_exit_actions_require_typed_matching_tag_and_preserve_exact_scalars() {
        for (suffix, action) in [("pause", "PAUSE"), ("cancel", "CANCEL"), ("resume", "RESUME"), ("reconcile-withdrawal", "RECONCILE_WITHDRAWAL")] {
            let mut body = serde_json::json!({"schema_version":1,"expected_revision":"9007199254740993","action":action});
            if action == "RESUME" { body["preview_id"] = ID.into(); }
            if action == "RECONCILE_WITHDRAWAL" {
                body["user_reported_amount"] = "1234567890.123456789012345678".into(); body["currency"] = "USD".into();
                body["external_transfer_ref"] = "original-transfer".into();
            }
            let bytes = serde_json::to_vec(&body).unwrap();
            let request = CapitalExits::action_input(ID.into(), suffix, action, bytes.as_slice()).unwrap();
            assert_eq!(request.route, format!("/api/v2/capital-exits/{ID}/{suffix}"));
            assert_eq!(request.status, 202); assert!(request.operator); assert!(request.requires_idempotency_key());
            assert_eq!(serde_json::from_slice::<serde_json::Value>(request.body.as_ref().unwrap()).unwrap(), body);
            assert!(CapitalExits::action_input(ID.into(), suffix, "WRONG_ACTION", bytes.as_slice()).is_err());
            body["unknown_field"] = true.into();
            assert!(CapitalExits::action_input(ID.into(), suffix, action, serde_json::to_vec(&body).unwrap().as_slice()).is_err());
        }
    }
}

#[cfg(test)]
mod complete_input_tests {
    use super::*;

    #[test]
    fn typed_input_crosses_the_former_transport_limit_without_truncation() {
        let expected = serde_json::json!({"original": "a".repeat(16 * 1024 * 1024 + 1)});
        let raw = serde_json::to_vec(&expected).unwrap();
        let parsed = read_input::<serde_json::Value>(raw.as_slice()).unwrap();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&parsed).unwrap(), expected);
        let mut incomplete = raw;
        incomplete.pop();
        assert!(read_input::<serde_json::Value>(incomplete.as_slice()).is_err());
    }
}

#[cfg(test)]
mod uncapped_watch_tests {
    use super::*;
    use clap::Parser;
    #[derive(Parser)]
    struct Arguments { #[command(subcommand)] command: Command }
    const RUN: &str = "018fc823-8e40-7000-8000-000000000001";

    #[test]
    fn watch_has_no_default_time_or_event_ceiling_and_preserves_explicit_values() {
        let request = Arguments::try_parse_from(["client", "run", "watch", RUN]).unwrap().command.request().unwrap();
        assert!(matches!(request.output, Output::Events { seconds: None, events: None, .. }));
        let request = Arguments::try_parse_from(["client", "run", "watch", RUN, "--max-seconds", "3601", "--max-events", "10001"]).unwrap().command.request().unwrap();
        assert!(matches!(request.output, Output::Events { seconds: Some(3601), events: Some(10001), .. }));
        for field in ["--max-seconds", "--max-events"] {
            assert!(Arguments::try_parse_from(["client", "run", "watch", RUN, field, "0"]).is_err());
        }
    }
}
