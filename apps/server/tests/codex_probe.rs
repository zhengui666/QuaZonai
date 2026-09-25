//! Real official App Server and deployment probe, with an isolated native home.
//! A deliberately stalled MCP must never start during a settings observation.
#![cfg(feature = "native-codex")]
use contracts::{codex::*, Id, SchemaV1};
use server::codex_profiles::{CodexDeployment, CodexDeploymentBinding, CodexDeploymentConfig};
use std::{path::PathBuf, time::Duration};
use store::codex_profiles::CodexProfileSnapshot;

// Reuse the existing loopback provider; turn/history helpers are not used here.
#[allow(dead_code)]
#[path = "support/codex_responses.rs"]
mod responses;

#[tokio::test]
async fn status_refresh_skips_stalled_mcp_and_preserves_native_model_settings() {
    let root = tempfile::tempdir().unwrap();
    let provider = responses::Provider::start(root.path()).await;
    let configuration_path = root.path().join("config.toml");
    let marker = root.path().join("mcp-was-started");
    let configuration = format!(
        "service_tier = \"fast\"\n{}\n[projects.{}]\ntrust_level = \"trusted\"\n\
         [mcp_servers.slow_status_probe]\ncommand = \"/bin/sh\"\n\
         args = [\"-c\", 'printf started > \"$1\"; exec sleep 60', \"probe\", {}]\n\
         enabled = true\nrequired = true\nstartup_timeout_sec = 60\n",
        std::fs::read_to_string(&configuration_path).unwrap(),
        serde_json::to_string(root.path()).unwrap(),
        serde_json::to_string(&marker).unwrap(),
    );
    std::fs::write(&configuration_path, &configuration).unwrap();
    let deployment = CodexDeployment::new(CodexDeploymentConfig {
        schema_version: SchemaV1,
        binary: PathBuf::from(std::env::var_os("CODEX_NATIVE_BIN").unwrap()),
        executable_path: std::env::var("PATH").unwrap(),
        bindings: vec![CodexDeploymentBinding {
            reference: "status-probe".into(),
            label: "Status probe".into(),
            profile_origin: ProfileOrigin::OperatorMount,
            home: root.path().to_path_buf(),
            codex_home: root.path().to_path_buf(),
            working_directory: root.path().to_path_buf(),
            environment_names: vec![],
        }],
    })
    .unwrap();
    let mut snapshot = CodexProfileSnapshot {
        profile: serde_json::from_value(serde_json::json!({
            "id":Id::new(),"name":"Status probe","home_binding":"status-probe",
            "profile_origin":"OPERATOR_MOUNT","connection_mode":"SYSTEM",
            "model_settings":{
                "schema_version":1,"use_default_model_settings":true,
                "saved_model":null,"saved_reasoning_effort":null,"saved_fast_mode":false
            },
            "revision":"1","created_at":chrono::Utc::now(),"updated_at":chrono::Utc::now()
        }))
        .unwrap(),
    };
    for iteration in 0..4 {
        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(Duration::from_secs(10), deployment.probe(&snapshot))
            .await
            .expect("a status refresh must not wait for the 60-second MCP startup");
        eprintln!("native status refresh {iteration}: {:?}", started.elapsed());
        let CodexProbeOutcomeV1::Available {
            account,
            effective,
            native_default_model,
            models,
            ..
        } = outcome
        else {
            panic!("native status refresh failed: {outcome:?}");
        };
        assert!(!account.requires_openai_auth);
        assert!(account.authentication_kind.is_none());
        assert_eq!(effective.provider, "local_fixture");
        assert_eq!(native_default_model.as_deref(), Some("gpt-5.4"));
        if snapshot.profile.model_settings.use_default_model_settings {
            assert_eq!(effective.model, "gpt-5.4");
            assert_eq!(effective.service_tier.as_deref(), Some("priority"));
            let selected = models
                .iter()
                .find(|model| {
                    model.capability.model != effective.model
                        && !model.capability.hidden
                        && !model.capability.supported_reasoning_efforts.is_empty()
                        && model.service_tiers.iter().any(|tier| tier.id == "priority")
                })
                .expect("the native catalog must advertise another selectable model");
            let settings = &mut snapshot.profile.model_settings;
            settings.use_default_model_settings = false;
            settings.saved_model = Some(selected.capability.model.clone());
            settings.saved_reasoning_effort =
                Some(selected.capability.default_reasoning_effort.clone());
        } else {
            assert_eq!(
                effective.service_tier.as_deref(),
                Some(if iteration == 1 {
                    "default"
                } else {
                    "priority"
                })
            );
            assert_eq!(
                Some(effective.model),
                snapshot.profile.model_settings.saved_model
            );
            assert_eq!(
                effective.reasoning_effort,
                snapshot.profile.model_settings.saved_reasoning_effort
            );
            snapshot.profile.model_settings.saved_fast_mode = true;
            if iteration == 2 {
                snapshot.profile.model_settings.use_default_model_settings = true;
            }
        }
        assert!(
            !marker.exists(),
            "a status-only probe launched an MCP process"
        );
        assert_eq!(
            provider.request_count(),
            0,
            "status refresh started inference"
        );
        assert_eq!(
            std::fs::read_to_string(&configuration_path).unwrap(),
            configuration
        );
        assert!(!root.path().join("auth.json").exists());
    }
}
