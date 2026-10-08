//! Thin client for the official Codex App Server. Codex owns its tool loop,
//! authentication and canonical history; QZ owns transport correlation and bindings.
mod close_diagnostics;
pub(crate) mod container;
mod mission;
mod projection;
#[cfg(test)]
mod projection_tests;
mod requests;
mod resource_account;
mod resource_monitor;
pub use resource_monitor::ResourceMonitor;
mod resources;
mod service;
mod service_exec;
pub use service_exec::exec as service_exec;
mod wire;

pub use container::ContainerBackend;
pub use mission::MissionOptions;
pub use projection::{
    Account, AccountState, DeviceLogin, LoginCancellation, LoginCancellationStatus, NativeEffort,
    NativeModel, NativeServiceTier, Observation, PublicMessage, Sandbox, Thread, ThreadIdentity,
    TokenCounts, Turn, TurnStatus,
};
pub use requests::{Launch, ThreadOptions};
pub use resources::MissionProcess;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{collections::BTreeSet, fmt, time::Duration};
use tokio::process::Child;
use wire::{RequestId, Wire};

const CLIENT: &str = "quazonai_native";
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
    CpuBudgetExceeded,
    ReconciliationOnly,
    UnboundedMissionLifecycleUnavailable,
}
impl fmt::Display for NativeFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnboundedMissionLifecycleUnavailable => "finite Mission CPU caps without wall limits lack verified owner-independent enforcement",
            Self::ReconciliationOnly => "native reconciliation cannot start a session or model turn",
            Self::CpuBudgetExceeded => "native Mission cumulative CPU grant is exhausted",
            Self::Configuration => "native Codex deployment configuration is invalid",
            Self::Version => "native Codex did not identify a valid version",
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
    child: Option<Child>,
    container: Option<container::Container>,
    wire: Wire<container::Reader, container::Writer>,
    binary: std::path::PathBuf,
    codex_home: std::path::PathBuf,
    version: String,
    reconciliation_thread: Option<String>,
}

impl Client {
    pub async fn start(launch: Launch) -> Result<Self> {
        Self::start_process(launch, None).await
    }

    pub async fn start_mission(launch: Launch, limits: MissionProcess) -> Result<Self> {
        Self::start_process(launch, Some(limits)).await
    }

    async fn start_process(launch: Launch, mut limits: Option<MissionProcess>) -> Result<Self> {
        let codex_home = launch.codex_home.clone();
        let reconciliation_thread = limits
            .as_ref()
            .and_then(|limits| limits.reconciliation_thread_id.clone());
        let (binary, child, container, input, output, native_limits) = if launch.container.is_some()
        {
            let (container, output, input) = container::start(launch, limits).await?;
            (
                std::path::PathBuf::from(container::BINARY),
                None,
                Some(container),
                input,
                output,
                None,
            )
        } else {
            let binary =
                std::fs::canonicalize(&launch.binary).map_err(|_| NativeFailure::Configuration)?;
            if let Some(limits) = &mut limits {
                limits.prepare(None).await?;
                limits.wait_released().await?;
            }
            if let Some(limits) = &limits {
                limits.begin_launch().await?;
            }
            let mut child = match launch.spawn(limits.as_ref()) {
                Ok(child) => child,
                Err(error) => {
                    if let Some(limits) = &limits {
                        limits.abort_before_spawn().await?;
                    }
                    return Err(error);
                }
            };
            let input: container::Writer =
                Box::pin(child.stdin.take().ok_or(NativeFailure::Unavailable)?);
            let output: container::Reader =
                Box::pin(child.stdout.take().ok_or(NativeFailure::Unavailable)?);
            (binary, Some(child), None, input, output, limits)
        };
        let mut client = Self {
            group: None,
            child,
            container,
            wire: Wire::new(output, input),
            binary,
            codex_home,
            version: String::new(),
            reconciliation_thread,
        };
        if let Some(limits) = native_limits {
            client.group = Some(
                limits
                    .capture_bound(
                        client
                            .child
                            .as_ref()
                            .and_then(Child::id)
                            .ok_or(NativeFailure::Unavailable)?,
                    )
                    .await?,
            );
        }
        let initialized: projection::Initialized =
            client.call("initialize", requests::initialize()).await?;
        client.version = domain::codex::verified_codex_version(&initialized.user_agent, CLIENT)
            .map_err(|_| NativeFailure::Version)?
            .to_owned();
        client.wire.notify("initialized").await?;
        Ok(client)
    }

    pub fn is_closed(&self) -> bool {
        self.wire.closed()
    }

    pub fn version(&self) -> &str {
        &self.version
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
        if self
            .reconciliation_thread
            .as_ref()
            .is_some_and(|thread| !reconciliation_call_allowed(thread, method, &params))
        {
            return Err(NativeFailure::ReconciliationOnly);
        }
        self.enforce_cpu().await?;
        let response = {
            let request = self
                .wire
                .request(RequestId::Text(id), method, params);
            tokio::pin!(request);
            loop {
                tokio::select! {
                    result=&mut request=>break result?,
                    _=tokio::time::sleep(Duration::from_millis(200))=>{
                        check_cpu(&mut self.group,&mut self.container).await?;
                    }
                }
            }
        };
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
        loop {
            let mut params = json!({"limit":100,"includeHidden":true});
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            let page: projection::ModelPage = self.call("model/list", params).await?;
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
        loop {
            let mut params = json!({"threadId":thread_id,"limit":100,"sortDirection":"desc","itemsView":"notLoaded"});
            if let Some(cursor) = &cursor {
                params["cursor"] = json!(cursor);
            }
            let page: projection::TurnPage = self.call("thread/turns/list", params).await?;
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
    }

    pub async fn observations(&mut self, wait: Duration) -> Result<Vec<Observation>> {
        self.enforce_cpu().await?;
        self.wire.poll(wait).await
    }

    /// Native summary view omits hidden/tool items upstream. Its status is not a
    /// terminal proof: the Mission requires its separately recorded notification.
    pub async fn public_summary(
        &mut self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<Option<PublicMessage>> {
        projection::text(thread_id, 200)?;
        projection::text(turn_id, 200)?;
        let mut cursor = None::<String>;
        let mut cursors = BTreeSet::new();
        loop {
            // Read public summaries lazily, without limiting total history pages.
            let page:projection::SummaryPage=self.call("thread/turns/list",json!({
                "threadId":thread_id,"itemsView":"summary","limit":1,"sortDirection":"desc","cursor":cursor
            })).await?;
            if page.data.len() > 1 {
                return Err(NativeFailure::Contract);
            }
            for item in page.data {
                let matched = item.turn.id == turn_id;
                let message = item.message()?;
                if matched {
                    return Ok(message);
                }
            }
            match page.next_cursor {
                None => return Ok(None),
                Some(next) => {
                    projection::text(&next, 4096)?;
                    if !cursors.insert(next.clone()) {
                        return Err(NativeFailure::Correlation);
                    }
                    cursor = Some(next);
                }
            }
        }
    }

    /// Kill-on-drop remains the failure fallback. Closing this transport never
    /// claims that an upstream model turn or a scientific Runtime job did not run.
    pub async fn enforce_cpu(&mut self) -> Result<()> {
        check_cpu(&mut self.group, &mut self.container).await
    }

    pub async fn close(mut self) -> Result<()> {
        use close_diagnostics::{failure, Phase};
        // Freeze and commit final CPU before EOF can let the native leader exit
        // and systemd/Docker discard its process-tree accounting.
        if let Some(group) = &mut self.group {
            group
                .close()
                .await
                .map_err(|error| failure(Phase::ClientGroup, error))?;
        }
        if let Some(container) = &mut self.container {
            container
                .close()
                .await
                .map_err(|error| failure(Phase::ClientContainerBeforeShutdown, error))?;
        }
        self.wire.shutdown().await;
        if let Some(container) = &mut self.container {
            return container
                .close()
                .await
                .map_err(|error| failure(Phase::ClientContainerAfterShutdown, error));
        }
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| failure(Phase::ClientChildMissing, NativeFailure::Unavailable))?;
        let result = match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err(failure(Phase::ClientChildWait, NativeFailure::Unavailable)),
            Err(_) => child
                .kill()
                .await
                .map_err(|_| failure(Phase::ClientChildKill, NativeFailure::Unavailable)),
        };

        result
    }
}

async fn check_cpu(
    group: &mut Option<resources::ProcessGroup>,
    container: &mut Option<container::Container>,
) -> Result<()> {
    if let Some(group) = group {
        group.check_cpu().await?;
    }
    if let Some(container) = container {
        container.check_cpu().await?;
    }
    Ok(())
}

fn reconciliation_call_allowed(thread: &str, method: &str, params: &Value) -> bool {
    match method {
        "initialize" | "account/read" | "model/list" | "config/read" => true,
        "thread/resume" | "thread/read" | "thread/turns/list" | "turn/interrupt" => {
            params.get("threadId").and_then(Value::as_str) == Some(thread)
        }
        _ => false,
    }
}
#[cfg(test)]
mod reconciliation_tests {
    use super::*;
    #[test]
    fn control_client_cannot_start_or_borrow_another_native_session() {
        let own = json!({"threadId":"original","turnId":"already-sent"});
        for method in [
            "thread/resume",
            "thread/read",
            "thread/turns/list",
            "turn/interrupt",
        ] {
            assert!(reconciliation_call_allowed("original", method, &own));
            assert!(!reconciliation_call_allowed("other", method, &own));
        }
        for method in [
            "thread/start",
            "turn/start",
            "account/login/start",
            "account/logout",
            "mcpServer/oauth/login",
        ] {
            assert!(!reconciliation_call_allowed("original", method, &own));
        }
    }
}
