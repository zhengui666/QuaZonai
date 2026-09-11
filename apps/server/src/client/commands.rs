//! Closed CLI routing over the shared Rust request/response types.
use super::{Failure, Result};
use clap::{Args, Subcommand};
use contracts::{
    artifacts::{ArtifactCreate, ArtifactView},
    brief::{BriefCreate, BriefUpdate, BriefView},
    control::{
        CommandResult, OperatorGrantRequest, OperatorGrantView, Page, ProjectCreate, ProjectUpdate,
        ProjectView,
    },
    cycles::{BriefFreezeV1, CycleStartV1, CycleStartedV1, CycleViewV1, FrozenBriefV1},
    data::*,
    experiments::{ExperimentProposalV1, ExperimentView},
    lifecycle::{RunCancelV1, RunListQuery},
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
    Project(Project),
    #[command(subcommand)]
    Brief(Brief),
    #[command(subcommand)]
    Cycle(Cycle),
    #[command(subcommand)]
    Data(Data),
    #[command(subcommand)]
    Runtime(Runtime),
    #[command(subcommand)]
    Downstream(Downstream),
    #[command(subcommand)]
    InputSet(InputSet),
    #[command(subcommand)]
    Policy(Policy),
    #[command(subcommand)]
    Experiment(Experiment),
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
pub enum Downstream {
    List(List),
    Show { id: String },
    Create,
    Update { id: String },
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
            Self::Cycle(command) => match command {
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
            },
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
