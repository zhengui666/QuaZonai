//! Thin client for the pinned official Codex App Server. Codex owns its tool loop,
//! authentication and canonical history; QZ owns only bounded transport and bindings.
mod mission;
mod projection;
#[cfg(test)]
mod projection_tests;
mod requests;
mod resources;
mod wire;

pub use mission::MissionOptions;
pub use projection::{
    Account, AccountState, DeviceLogin, LoginCancellation, LoginCancellationStatus, NativeEffort,
    NativeModel, NativeServiceTier, Observation, Sandbox, Thread, ThreadIdentity, TokenCounts,
    Turn, TurnStatus,
};
pub use requests::{CustomProvider, Launch, ThreadOptions};
pub use resources::MissionProcess;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{collections::BTreeSet, fmt, time::Duration};
use tokio::process::{Child, ChildStdin, ChildStdout};
use wire::{RequestId, Wire};

pub const VERSION: &str = "0.144.4";
pub const MAX_FRAME: usize = 2 * 1024 * 1024;
const CLIENT: &str = "quazonai_native";
const RPC_TIMEOUT: Duration = Duration::from_secs(20);
pub type Result<T> = std::result::Result<T, NativeFailure>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeFailure {
    Configuration,
    Version,
    Unavailable,
    Closed,
    Contract,
    Correlation,
    FrameLimit,
    ObservationLimit,
    Rejected(i64),
    ModelUnavailable,
    ProfileInstructions,
}
impl fmt::Display for NativeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Configuration => "native Codex deployment configuration is invalid",
            Self::Version => "native Codex version differs from the pinned contract",
            Self::Unavailable => {
                "native Codex connection is unavailable; request outcome may be unknown"
            }
            Self::Closed => "native Codex connection cannot be reused",
            Self::Contract => "native Codex response violates the selected contract",
            Self::Correlation => "native Codex response does not match the pending request",
            Self::FrameLimit => "native Codex frame exceeds its bounded transport",
            Self::ObservationLimit => "native Codex observation capacity is exhausted",
            Self::Rejected(_) => "native Codex rejected the request",
            Self::ModelUnavailable => "native Codex did not honor the selected model settings",
            Self::ProfileInstructions => "Mission requires a dedicated native profile without personal instruction files or overrides",
        })
    }
}
impl std::error::Error for NativeFailure {}

/// One native child and one serial RPC stream. There is deliberately no Clone:
/// profile/session owners serialize access, and never spawn a second tool driver.
pub struct Client {
    group: Option<resources::ProcessGroup>,
    child: Child,
    wire: Wire<ChildStdout, ChildStdin>,
    binary: std::path::PathBuf,
    codex_home: std::path::PathBuf,
    rpc_timeout: Duration,
}

impl Client {
    pub async fn start(launch: Launch) -> Result<Self> {
        Self::start_process(launch, None).await
    }

    pub async fn start_mission(launch: Launch, limits: MissionProcess) -> Result<Self> {
        Self::start_process(launch, Some(limits)).await
    }

    async fn start_process(launch: Launch, limits: Option<MissionProcess>) -> Result<Self> {
        let binary =
            std::fs::canonicalize(&launch.binary).map_err(|_| NativeFailure::Configuration)?;
        let codex_home = launch.codex_home.clone();
        let mut child = launch.spawn(limits.as_ref())?;
        let input = child.stdin.take().ok_or(NativeFailure::Unavailable)?;
        let output = child.stdout.take().ok_or(NativeFailure::Unavailable)?;
        let mut client = Self {
            group: None,
            child,
            wire: Wire::new(output, input),
            binary,
            codex_home,
            rpc_timeout: if limits.is_some() {
                Duration::from_secs(60)
            } else {
                RPC_TIMEOUT
            },
        };
        let initialized: projection::Initialized =
            client.call("initialize", requests::initialize()).await?;
        domain::codex::verified_codex_version(&initialized.user_agent, CLIENT, VERSION)
            .map_err(|_| NativeFailure::Version)?;
        if let Some(limits) = limits {
            client.group =
                Some(limits.capture(client.child.id().ok_or(NativeFailure::Unavailable)?)?);
        }
        client.wire.notify("initialized").await?;
        Ok(client)
    }

    pub fn is_closed(&self) -> bool {
        self.wire.closed()
    }

    async fn call<T: DeserializeOwned>(
        &mut self,
        method: &'static str,
        params: Value,
    ) -> Result<T> {
        self.call_with_id(contracts::Id::new().to_string(), method, params)
            .await
    }

    async fn call_with_id<T: DeserializeOwned>(
        &mut self,
        id: String,
        method: &'static str,
        params: Value,
    ) -> Result<T> {
        projection::text(&id, 200)?;
        let response = self
            .wire
            .request(RequestId::Text(id), method, params, self.rpc_timeout)
            .await?;
        match serde_json::from_str(response.get()) {
            Ok(value) => Ok(value),
            Err(_) => {
                self.wire.invalidate();
                Err(NativeFailure::Contract)
            }
        }
    }

    pub async fn account(&mut self) -> Result<AccountState> {
        let response: AccountState = self
            .call("account/read", json!({"refreshToken":false}))
            .await?;
        response.validate()?;
        Ok(response)
    }

    /// Only the native device-code flow is exposed. QZ neither injects tokens nor
    /// reimplements OAuth polling/refresh. The returned code is write-only UI state.
    pub async fn device_login(&mut self) -> Result<projection::DeviceLogin> {
        let login: projection::DeviceLogin = self
            .call("account/login/start", json!({"type":"chatgptDeviceCode"}))
            .await?;
        login.validate()?;
        Ok(login)
    }

    pub async fn cancel_login(&mut self, login_id: &str) -> Result<projection::LoginCancellation> {
        projection::text(login_id, 200)?;
        self.call("account/login/cancel", json!({"loginId":login_id}))
            .await
    }

    pub async fn logout(&mut self) -> Result<()> {
        let _: projection::Empty = self.call("account/logout", json!({})).await?;
        Ok(())
    }

    /// All pages or an error. A partial/duplicate/stale catalog is not silently
    /// turned into a default model or an invented reasoning-effort list.
    pub async fn models(&mut self) -> Result<Vec<NativeModel>> {
        let mut models = Vec::new();
        let mut ids = BTreeSet::new();
        let mut cursors = BTreeSet::new();
        let mut cursor = None::<String>;
        for _ in 0..128 {
            let mut params = json!({"limit":100,"includeHidden":true});
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            let page: projection::ModelPage = self.call("model/list", params).await?;
            if page.data.len() > 100 || models.len() + page.data.len() > 4096 {
                return Err(NativeFailure::ObservationLimit);
            }
            for model in page.data {
                model.validate()?;
                if !ids.insert(model.id.clone()) {
                    return Err(NativeFailure::Contract);
                }
                models.push(model);
            }
            match page.next_cursor {
                None => {
                    return if models.is_empty() {
                        Err(NativeFailure::ModelUnavailable)
                    } else {
                        Ok(models)
                    }
                }
                Some(next) => {
                    projection::text(&next, 4096)?;
                    if !cursors.insert(next.clone()) {
                        return Err(NativeFailure::Correlation);
                    }
                    cursor = Some(next);
                }
            }
        }
        Err(NativeFailure::ObservationLimit)
    }

    pub async fn start_thread(&mut self, options: &ThreadOptions) -> Result<Thread> {
        let params = self
            .mission_params(options, options.start_params()?)
            .await?;
        let response: Thread = self.call("thread/start", params).await?;
        options.validate_response(&response)?;
        Ok(response)
    }

    pub async fn resume_thread(
        &mut self,
        thread_id: &str,
        options: &ThreadOptions,
    ) -> Result<Thread> {
        projection::text(thread_id, 200)?;
        let params = self
            .mission_params(options, options.resume_params(thread_id)?)
            .await?;
        let response: Thread = self.call("thread/resume", params).await?;
        options.validate_response(&response)?;
        if response.thread.id != thread_id {
            self.wire.invalidate();
            return Err(NativeFailure::Correlation);
        }
        Ok(response)
    }

    /// Caller must persist the exact send intent before invoking this once. RPC
    /// IDs and clientUserMessageId are correlations, not a native retry guarantee.
    pub async fn start_turn(
        &mut self,
        rpc_id: &str,
        thread_id: &str,
        prompt: &str,
    ) -> Result<Turn> {
        let response: projection::TurnResponse = self
            .call_with_id(
                rpc_id.to_owned(),
                "turn/start",
                requests::turn(rpc_id, thread_id, prompt)?,
            )
            .await?;
        response.turn.validate()?;
        Ok(response.turn)
    }

    pub async fn interrupt_turn(&mut self, thread_id: &str, turn_id: &str) -> Result<()> {
        projection::text(thread_id, 200)?;
        projection::text(turn_id, 200)?;
        let _: projection::Empty = self
            .call(
                "turn/interrupt",
                json!({"threadId":thread_id,"turnId":turn_id}),
            )
            .await?;
        // This is only an interruption request, not a terminal or a usage receipt.
        Ok(())
    }

    pub async fn turns(&mut self, thread_id: &str) -> Result<Vec<Turn>> {
        projection::text(thread_id, 200)?;
        let mut turns = Vec::new();
        let mut ids = BTreeSet::new();
        let mut cursors = BTreeSet::new();
        let mut cursor = None::<String>;
        for _ in 0..128 {
            let mut params = json!({"threadId":thread_id,"limit":100,"sortDirection":"desc","itemsView":"notLoaded"});
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            let page: projection::TurnPage = self.call("thread/turns/list", params).await?;
            if page.data.len() > 100 || turns.len() + page.data.len() > 4096 {
                return Err(NativeFailure::ObservationLimit);
            }
            for turn in page.data {
                turn.validate()?;
                if !ids.insert(turn.id.clone()) {
                    return Err(NativeFailure::Correlation);
                }
                turns.push(turn);
            }
            match page.next_cursor {
                None => return Ok(turns),
                Some(next) => {
                    projection::text(&next, 4096)?;
                    if !cursors.insert(next.clone()) {
                        return Err(NativeFailure::Correlation);
                    }
                    cursor = Some(next);
                }
            }
        }
        Err(NativeFailure::ObservationLimit)
    }

    pub async fn observations(&mut self, wait: Duration) -> Result<Vec<Observation>> {
        if wait > Duration::from_secs(30) {
            return Err(NativeFailure::Configuration);
        }
        self.wire.poll(wait).await
    }

    /// Kill-on-drop remains the failure fallback. Closing this transport never
    /// claims that an upstream model turn or a scientific Runtime job did not run.
    pub async fn close(mut self) -> Result<()> {
        self.wire.shutdown().await;
        let result = match tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err(NativeFailure::Unavailable),
            Err(_) => self
                .child
                .kill()
                .await
                .map_err(|_| NativeFailure::Unavailable),
        };
        if let Some(group) = &mut self.group {
            group.close().await?;
        }
        result
    }
}
