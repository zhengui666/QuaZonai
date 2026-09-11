//! Native Mission permissions and stdio MCP configuration, not an Agent tool loop.
use super::{requests::ThreadOptions, Client, NativeFailure, Result};
use crate::mcp::MissionBinding;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf};

const MCP_NAME: &str = "quazonai_mission";

/// Trusted launcher input only. No Debug/Serialize: the scoped credential belongs
/// to the native MCP child, never to model-visible input or an audit document.
pub struct MissionOptions {
    pub server_binary: PathBuf,
    pub api_origin: String,
    pub development_http: bool,
    pub binding: MissionBinding,
    pub token: String,
    pub executable_path: String,
}

impl MissionOptions {
    pub(super) fn configure(&self, request: &mut Value, options: &ThreadOptions) -> Result<()> {
        if !self.server_binary.is_absolute()
            || !self.server_binary.is_file()
            || self.executable_path.is_empty()
            || self.executable_path.contains('\0')
            || options.ephemeral
        {
            return Err(NativeFailure::Configuration);
        }
        domain::settings::endpoint(&self.api_origin, self.development_http)
            .map_err(|_| NativeFailure::Configuration)?;
        integrations::authentication::machine_token(&self.token)
            .map_err(|_| NativeFailure::Configuration)?;
        let mut args = vec![
            "mcp".to_owned(),
            "--api-origin".into(),
            self.api_origin.clone(),
            "--project-id".into(),
            self.binding.project_id.to_string(),
            "--cycle-id".into(),
            self.binding.cycle_id.to_string(),
            "--run-id".into(),
            self.binding.run_id.to_string(),
            "--attempt-id".into(),
            self.binding.attempt_id.to_string(),
            "--brief-id".into(),
            self.binding.brief_id.to_string(),
            "--workspace-root".into(),
            options
                .working_directory
                .to_str()
                .ok_or(NativeFailure::Configuration)?
                .into(),
        ];
        if self.development_http {
            args.push("--development-http".into());
        }
        let name = format!("quazonai-{}", self.binding.run_id);
        request
            .as_object_mut()
            .ok_or(NativeFailure::Configuration)?
            .remove("sandbox");
        request["permissions"] = json!(name);
        request["runtimeWorkspaceRoots"] = json!([options.working_directory]);
        if request.get("config").is_none() {
            request["config"] = json!({});
        }
        let config = &mut request["config"];
        // Native refresh rebuilds the thread configuration without the original
        // typed `permissions` request override; the catalog also needs a default.
        config["default_permissions"] = json!(name);
        config["permissions"] = json!({ name: {
            "filesystem":{":root":"deny",":minimal":"read",":workspace_roots":{".":"write"}},
            "network":{"enabled":false}
        }});
        config["shell_environment_policy"] =
            json!({"inherit":"none","set":{"PATH":self.executable_path}});
        config["allow_login_shell"] = json!(false);
        config["project_doc_max_bytes"] = json!(0);
        config["skills"] = json!({"include_instructions":false});
        config["web_search"] = json!("disabled");
        config["features"] = json!({"apps":false,"plugins":false,"hooks":false,"codex_hooks":false,
            "plugin_hooks":false,"memories":false,"memory_tool":false,"multi_agent":false,
            "browser_use":false,"computer_use":false,"in_app_browser":false,"image_generation":false,
            "goals":false,"shell_snapshot":false,"code_mode":false,"code_mode_only":false});
        config["mcp_servers"] = json!({ MCP_NAME: {"command":self.server_binary,"args":args,
            "env":{"QUAZONAI_MCP_TOKEN":self.token},"required":true,"enabled":true,
            "startup_timeout_sec":45,"tool_timeout_sec":20}});
        Ok(())
    }
}

#[derive(Deserialize)]
struct Configuration {
    config: McpNames,
}
#[derive(Deserialize)]
struct McpNames {
    // Only keys survive deserialization. Ignore all other native configuration,
    // including provider values, server environments, custom instructions and paths.
    #[serde(default)]
    mcp_servers: BTreeMap<String, serde::de::IgnoredAny>,
    instructions: Option<serde::de::IgnoredAny>,
    developer_instructions: Option<serde::de::IgnoredAny>,
    model_instructions_file: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Inventory {
    data: Vec<ServerTools>,
    next_cursor: Option<String>,
}
#[derive(Deserialize)]
struct ServerTools {
    name: String,
    tools: BTreeMap<String, serde::de::IgnoredAny>,
}

impl Client {
    /// Public inventory names only. Tool descriptions, native errors and server
    /// configuration never enter the trusted service's status/audit projection.
    pub async fn mission_tool_names(
        &mut self,
        thread: &str,
    ) -> Result<BTreeMap<String, Vec<String>>> {
        super::projection::text(thread, 200)?;
        let inventory: Inventory = self
            .call(
                "mcpServerStatus/list",
                json!({"threadId":thread,"detail":"toolsAndAuthOnly","limit":65}),
            )
            .await?;
        if inventory.next_cursor.is_some() || inventory.data.len() > 65 {
            return Err(NativeFailure::ObservationLimit);
        }
        let mut result = BTreeMap::new();
        for server in inventory.data {
            super::projection::text(&server.name, 200)?;
            if server.tools.len() > 64 {
                return Err(NativeFailure::ObservationLimit);
            }
            for name in server.tools.keys() {
                super::projection::text(name, 200)?;
            }
            if result
                .insert(server.name, server.tools.into_keys().collect())
                .is_some()
            {
                return Err(NativeFailure::Contract);
            }
        }
        Ok(result)
    }

    pub(super) async fn mission_params(
        &mut self,
        options: &ThreadOptions,
        mut params: Value,
    ) -> Result<Value> {
        if options.mission.is_some() {
            // The released host loads global AGENTS independently of Thread's
            // project_doc_max_bytes, with no stdio opt-out. Never read/modify that
            // personal file or silently carry it into this bounded Mission.
            for name in ["AGENTS.md", "AGENTS.override.md"] {
                match std::fs::symlink_metadata(self.codex_home.join(name)) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    _ => return Err(NativeFailure::ProfileInstructions),
                }
            }
            // The released Linux sandbox re-execs this exact official binary.
            // :minimal does not include a deployment's /home or /opt executable;
            // allow only that file, not its parent directory or native HOME.
            let permission = params["permissions"]
                .as_str()
                .ok_or(NativeFailure::Configuration)?
                .to_owned();
            let binary = self.binary.to_str().ok_or(NativeFailure::Configuration)?;
            params["config"]["permissions"][permission]["filesystem"][binary] = json!("read");
            let native: Configuration = self
                .call(
                    "config/read",
                    json!({"includeLayers":false,"cwd":options.working_directory}),
                )
                .await?;
            if native.config.instructions.is_some()
                || native.config.developer_instructions.is_some()
                || native.config.model_instructions_file.is_some()
            {
                return Err(NativeFailure::ProfileInstructions);
            }
            if native.config.mcp_servers.len() > 64
                || native.config.mcp_servers.contains_key(MCP_NAME)
            {
                return Err(NativeFailure::Configuration);
            }
            for name in native.config.mcp_servers.keys() {
                super::projection::text(name, 200)?;
                params["config"]["mcp_servers"][name] = json!({"enabled":false});
            }
        }
        Ok(params)
    }
}
