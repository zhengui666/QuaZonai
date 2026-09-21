//! Fixed native requests and deployment-owned process parameters. These are not
//! HTTP DTOs: callers cannot inject an arbitrary command, environment or RPC.
use super::{
    projection::{self, Sandbox, Thread},
    NativeFailure, Result, CLIENT,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::process::{Child, Command};

/// A trusted deployment binding. No Debug/Serialize: explicit native environment
/// values can contain provider credentials. QZ never opens a native auth file.
pub struct Launch {
    pub binary: PathBuf,
    pub home: PathBuf,
    pub codex_home: PathBuf,
    pub working_directory: PathBuf,
    pub executable_path: OsString,
    pub native_environment: BTreeMap<OsString, OsString>,
}

fn directory(path: &Path) -> Result<()> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(NativeFailure::Configuration);
    }
    Ok(())
}

impl Launch {
    pub(super) fn spawn(self, resources: Option<&super::MissionProcess>) -> Result<Child> {
        if !self.binary.is_absolute() || !self.binary.is_file() {
            return Err(NativeFailure::Configuration);
        }
        for path in [&self.home, &self.codex_home, &self.working_directory] {
            directory(path)?;
        }
        let mut command = Command::new(&self.binary);
        command
            .arg("app-server")
            .env_clear()
            .envs(self.native_environment)
            .env("PATH", self.executable_path)
            .env("HOME", &self.home)
            .env("CODEX_HOME", &self.codex_home)
            .env("RUST_LOG", "off")
            .current_dir(&self.working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(resources) = resources {
            command = resources.wrap(command)?;
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true);
        }
        command.spawn().map_err(|_| NativeFailure::Unavailable)
    }
}

/// Native thread configuration after profile/catalog validation. Default settings
/// are represented by omitted fields, never guessed model/effort/provider values.
pub struct ThreadOptions {
    pub working_directory: PathBuf,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
    pub expected_provider: Option<String>,
    pub ephemeral: bool,
    pub mission: Option<super::mission::MissionOptions>,
}
impl ThreadOptions {
    pub fn read_only(working_directory: PathBuf) -> Self {
        Self {
            working_directory,
            model: None,
            reasoning_effort: None,
            service_tier: None,
            expected_provider: None,
            ephemeral: false,
            mission: None,
        }
    }

    pub fn start_params(&self) -> Result<Value> {
        directory(&self.working_directory)?;
        for value in [
            &self.model,
            &self.reasoning_effort,
            &self.service_tier,
            &self.expected_provider,
        ]
        .into_iter()
        .flatten()
        {
            projection::text(value, 200)?;
        }
        let mut request = json!({
            "cwd": self.working_directory,
            "approvalPolicy":"never", "sandbox":"read-only",
            "experimentalRawEvents":false, "ephemeral":self.ephemeral,
            "allowProviderModelFallback":false,
        });
        if let Some(model) = &self.model {
            request["model"] = json!(model);
        }
        if let Some(effort) = &self.reasoning_effort {
            request["config"] = json!({"model_reasoning_effort":effort});
        }
        if let Some(tier) = &self.service_tier {
            request["serviceTier"] = json!(tier);
        }
        // expected_provider verifies an observed native binding; it never
        // overrides the owner's configured provider.
        if let Some(mission) = &self.mission {
            mission.configure(&mut request, self)?;
        }
        Ok(request)
    }

    pub fn resume_params(&self, thread_id: &str) -> Result<Value> {
        projection::text(thread_id, 200)?;
        let mut request = self.start_params()?;
        let fields = request
            .as_object_mut()
            .ok_or(NativeFailure::Configuration)?;
        for field in [
            "ephemeral",
            "experimentalRawEvents",
            "allowProviderModelFallback",
        ] {
            fields.remove(field);
        }
        request["threadId"] = json!(thread_id);
        request["excludeTurns"] = json!(true);
        Ok(request)
    }

    pub(super) fn validate_response(&self, response: &Thread) -> Result<()> {
        response.validate()?;
        let expected = if self.mission.is_some() {
            Sandbox::WorkspaceWrite {
                network_access: false,
            }
        } else {
            Sandbox::ReadOnly
        };
        if response.cwd != self.working_directory || response.sandbox != expected {
            return Err(NativeFailure::Contract);
        }
        if self
            .model
            .as_ref()
            .is_some_and(|value| *value != response.model)
            || self
                .expected_provider
                .as_ref()
                .is_some_and(|value| *value != response.model_provider)
            || self
                .reasoning_effort
                .as_ref()
                .is_some_and(|value| Some(value) != response.reasoning_effort.as_ref())
            || self
                .service_tier
                .as_ref()
                .is_some_and(|value| Some(value) != response.service_tier.as_ref())
        {
            return Err(NativeFailure::ModelUnavailable);
        }
        Ok(())
    }
}

pub(super) fn initialize() -> Value {
    json!({"clientInfo":{"name":CLIENT,"version":env!("CARGO_PKG_VERSION"),"title":"QuaZonai native integration"},
        "capabilities":{"experimentalApi":true,"requestAttestation":false,
            "mcpServerOpenaiFormElicitation":false,
            "optOutNotificationMethods":["item/reasoning/textDelta","item/reasoning/summaryTextDelta",
                "item/reasoning/summaryPartAdded","item/started","item/completed",
                "item/agentMessage/delta","item/commandExecution/outputDelta",
                "item/fileChange/outputDelta","turn/diff/updated","turn/plan/updated"]}})
}

pub(super) fn turn(client_id: &str, thread_id: &str, prompt: &str) -> Result<Value> {
    projection::text(client_id, 200)?;
    projection::text(thread_id, 200)?;
    if prompt.trim().is_empty() || prompt.len() > 256 * 1024 || prompt.contains('\0') {
        return Err(NativeFailure::Configuration);
    }
    Ok(json!({"threadId":thread_id,"clientUserMessageId":client_id,
        "input":[{"type":"text","text":prompt,"text_elements":[]}]}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_native_settings_are_omitted_and_resume_never_requests_raw_history() {
        let root = tempfile::tempdir().unwrap();
        let options = ThreadOptions::read_only(root.path().to_path_buf());
        let start = options.start_params().unwrap();
        for key in ["model", "modelProvider", "config", "serviceTier"] {
            assert!(start.get(key).is_none());
        }
        assert_eq!(start["allowProviderModelFallback"], false);
        let resume = options.resume_params("stored-thread").unwrap();
        assert_eq!(resume["excludeTurns"], true);
        assert_eq!(resume["threadId"], "stored-thread");
        for key in ["path", "history", "experimentalRawEvents", "ephemeral"] {
            assert!(resume.get(key).is_none());
        }
    }
    #[test]
    fn effort_only_override_does_not_guess_a_model_and_rpc_identity_is_not_a_retry_token() {
        let root = tempfile::tempdir().unwrap();
        let mut options = ThreadOptions::read_only(root.path().to_path_buf());
        options.reasoning_effort = Some("native-advertised-effort".into());
        let start = options.start_params().unwrap();
        assert!(start.get("model").is_none());
        assert_eq!(
            start["config"]["model_reasoning_effort"],
            "native-advertised-effort"
        );
        let input = turn("reserved-correlation", "known-thread", "A bounded request").unwrap();
        assert_eq!(input["clientUserMessageId"], "reserved-correlation");
        assert!(input.get("idempotencyKey").is_none());
    }
}
