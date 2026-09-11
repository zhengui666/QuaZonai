//! Selected native status fields. Canonical history and reasoning stay in Codex.
use super::{NativeFailure, Result};
use serde::Deserialize;
use serde_json::value::RawValue;

pub(super) fn text(value: &str, maximum: usize) -> Result<()> {
    if value.is_empty()
        || value.chars().count() > maximum
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(NativeFailure::Contract)
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observation {
    TurnStarted {
        thread_id: String,
        turn: Turn,
    },
    TurnCompleted {
        thread_id: String,
        turn: Turn,
    },
    Usage {
        thread_id: String,
        turn_id: String,
        total: TokenCounts,
    },
    LoginCompleted {
        login_id: String,
        success: bool,
    },
    ThreadClosed {
        thread_id: String,
    },
    ModelRerouted {
        thread_id: String,
        turn_id: String,
        from_model: String,
        to_model: String,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct TokenCounts {
    #[serde(rename = "inputTokens")]
    pub input: i64,
    #[serde(rename = "cachedInputTokens")]
    pub cached_input: i64,
    #[serde(rename = "outputTokens")]
    pub output: i64,
    #[serde(rename = "reasoningOutputTokens")]
    pub reasoning_output: i64,
    #[serde(rename = "totalTokens")]
    pub total: i64,
}
impl TokenCounts {
    pub fn validate(self) -> Result<Self> {
        if [
            self.input,
            self.cached_input,
            self.output,
            self.reasoning_output,
            self.total,
        ]
        .into_iter()
        .any(|n| n < 0)
        {
            return Err(NativeFailure::Contract);
        }
        Ok(self)
    }
    /// Use a known cumulative baseline, not the last request within a tool loop.
    pub fn since(self, baseline: Self) -> Result<Self> {
        self.validate()?;
        baseline.validate()?;
        let difference = Self {
            input: self.input - baseline.input,
            cached_input: self.cached_input - baseline.cached_input,
            output: self.output - baseline.output,
            reasoning_output: self.reasoning_output - baseline.reasoning_output,
            total: self.total - baseline.total,
        };
        difference.validate()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TurnNotification {
    thread_id: String,
    turn: Turn,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsageNotification {
    thread_id: String,
    turn_id: String,
    token_usage: ThreadUsage,
}
#[derive(Deserialize)]
struct ThreadUsage {
    total: TokenCounts,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginNotification {
    login_id: String,
    success: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClosedNotification {
    thread_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RerouteNotification {
    thread_id: String,
    turn_id: String,
    from_model: String,
    to_model: String,
}

fn decode<T: serde::de::DeserializeOwned>(raw: Option<&RawValue>) -> Result<T> {
    serde_json::from_str(raw.ok_or(NativeFailure::Contract)?.get())
        .map_err(|_| NativeFailure::Contract)
}

pub(super) fn notification(method: &str, raw: Option<&RawValue>) -> Result<Option<Observation>> {
    Ok(Some(match method {
        "turn/started" | "turn/completed" => {
            let value: TurnNotification = decode(raw)?;
            text(&value.thread_id, 200)?;
            value.turn.validate()?;
            if (method == "turn/started") == value.turn.status.terminal() {
                return Err(NativeFailure::Contract);
            }
            if method == "turn/started" {
                Observation::TurnStarted {
                    thread_id: value.thread_id,
                    turn: value.turn,
                }
            } else {
                Observation::TurnCompleted {
                    thread_id: value.thread_id,
                    turn: value.turn,
                }
            }
        }
        "thread/tokenUsage/updated" => {
            let value: UsageNotification = decode(raw)?;
            text(&value.thread_id, 200)?;
            text(&value.turn_id, 200)?;
            Observation::Usage {
                thread_id: value.thread_id,
                turn_id: value.turn_id,
                total: value.token_usage.total.validate()?,
            }
        }
        "account/login/completed" => {
            let value: LoginNotification = decode(raw)?;
            text(&value.login_id, 200)?;
            Observation::LoginCompleted {
                login_id: value.login_id,
                success: value.success,
            }
        }
        "thread/closed" => {
            let value: ClosedNotification = decode(raw)?;
            text(&value.thread_id, 200)?;
            Observation::ThreadClosed {
                thread_id: value.thread_id,
            }
        }
        "model/rerouted" => {
            let value: RerouteNotification = decode(raw)?;
            for text_ in [
                &value.thread_id,
                &value.turn_id,
                &value.from_model,
                &value.to_model,
            ] {
                text(text_, 200)?;
            }
            Observation::ModelRerouted {
                thread_id: value.thread_id,
                turn_id: value.turn_id,
                from_model: value.from_model,
                to_model: value.to_model,
            }
        }
        // Do not deserialize raw Responses events, reasoning, item contents,
        // native errors, plans, login secrets or unsupported event bodies.
        _ => return Ok(None),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Initialized {
    pub user_agent: String,
}

#[derive(Deserialize)]
pub(super) struct Empty {}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type")]
pub enum Account {
    #[serde(rename = "apiKey")]
    ApiKey,
    #[serde(rename = "chatgpt")]
    Chatgpt {
        #[serde(rename = "planType")]
        plan_type: String,
    },
    #[serde(rename = "amazonBedrock")]
    AmazonBedrock,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountState {
    pub account: Option<Account>,
    pub requires_openai_auth: bool,
}
impl AccountState {
    pub fn validate(&self) -> Result<()> {
        if let Some(Account::Chatgpt { plan_type }) = &self.account {
            text(plan_type, 120)?;
        }
        Ok(())
    }
}

// A device login code is displayed only to the initiating Operator, never Debug.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLogin {
    #[serde(rename = "type")]
    kind: String,
    pub login_id: String,
    pub verification_url: String,
    pub user_code: String,
}
impl DeviceLogin {
    pub fn validate(&self) -> Result<()> {
        if self.kind != "chatgptDeviceCode" {
            return Err(NativeFailure::Contract);
        }
        text(&self.login_id, 200)?;
        text(&self.user_code, 200)?;
        text(&self.verification_url, 2048)?;
        let url = url::Url::parse(&self.verification_url).map_err(|_| NativeFailure::Contract)?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(NativeFailure::Contract);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoginCancellationStatus {
    Canceled,
    NotFound,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct LoginCancellation {
    pub status: LoginCancellationStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeEffort {
    pub reasoning_effort: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct NativeServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeModel {
    pub id: String,
    pub model: String,
    pub display_name: String,
    pub hidden: bool,
    pub is_default: bool,
    pub default_reasoning_effort: String,
    pub supported_reasoning_efforts: Vec<NativeEffort>,
    #[serde(default)]
    pub service_tiers: Vec<NativeServiceTier>,
    pub default_service_tier: Option<String>,
}
impl NativeModel {
    pub fn validate(&self) -> Result<()> {
        for value in [
            &self.id,
            &self.model,
            &self.display_name,
            &self.default_reasoning_effort,
        ] {
            text(value, 200)?;
        }
        if self.supported_reasoning_efforts.is_empty()
            || self.supported_reasoning_efforts.len() > 64
            || self.service_tiers.len() > 64
        {
            return Err(NativeFailure::Contract);
        }
        let mut efforts = std::collections::BTreeSet::new();
        for effort in &self.supported_reasoning_efforts {
            text(&effort.reasoning_effort, 200)?;
            if effort.description.len() > 8192 || !efforts.insert(&effort.reasoning_effort) {
                return Err(NativeFailure::Contract);
            }
        }
        if !efforts.contains(&self.default_reasoning_effort) {
            return Err(NativeFailure::Contract);
        }
        let mut tiers = std::collections::BTreeSet::new();
        for tier in &self.service_tiers {
            text(&tier.id, 200)?;
            text(&tier.name, 200)?;
            if tier.description.len() > 8192 || !tiers.insert(&tier.id) {
                return Err(NativeFailure::Contract);
            }
        }
        if let Some(tier) = &self.default_service_tier {
            text(tier, 200)?;
            if !tiers.contains(tier) {
                return Err(NativeFailure::Contract);
            }
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ModelPage {
    pub data: Vec<NativeModel>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ThreadIdentity {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Sandbox {
    ReadOnly,
    WorkspaceWrite {
        #[serde(rename = "networkAccess", default)]
        network_access: bool,
    },
    #[serde(other)]
    Unsupported,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub thread: ThreadIdentity,
    pub model: String,
    pub model_provider: String,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub cwd: std::path::PathBuf,
    pub sandbox: Sandbox,
    pub approval_policy: String,
}
impl Thread {
    pub fn validate(&self) -> Result<()> {
        for value in [&self.thread.id, &self.model, &self.model_provider] {
            text(value, 200)?;
        }
        for value in [&self.reasoning_effort, &self.service_tier]
            .into_iter()
            .flatten()
        {
            text(value, 200)?;
        }
        if !self.cwd.is_absolute()
            || self.approval_policy != "never"
            || matches!(self.sandbox, Sandbox::Unsupported)
        {
            return Err(NativeFailure::Contract);
        }
        Ok(())
    }
}
#[derive(Deserialize)]
pub(super) struct TurnResponse {
    pub turn: Turn,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TurnPage {
    pub data: Vec<Turn>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnStatus {
    Completed,
    Interrupted,
    Failed,
    InProgress,
}
impl TurnStatus {
    pub fn terminal(self) -> bool {
        self != Self::InProgress
    }
}

fn has_value<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<bool, D::Error> {
    Ok(Option::<serde::de::IgnoredAny>::deserialize(deserializer)?.is_some())
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    pub id: String,
    pub status: TurnStatus,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub duration_ms: Option<i64>,
    #[serde(default, rename = "error", deserialize_with = "has_value")]
    pub has_error: bool,
}
impl Turn {
    pub fn validate(&self) -> Result<()> {
        text(&self.id, 200)?;
        if self.started_at.is_some_and(|v| v < 0)
            || self.completed_at.is_some_and(|v| v < 0)
            || self.duration_ms.is_some_and(|v| v < 0)
            || self
                .started_at
                .zip(self.completed_at)
                .is_some_and(|(a, b)| b < a)
            || (self.has_error && self.status != TurnStatus::Failed)
        {
            return Err(NativeFailure::Contract);
        }
        Ok(())
    }
}
