//! Exact official Codex subprocess compatibility. Empty HOME means no account,
//! inference or OAuth completion is claimed by these native protocol checks.
#![cfg(feature = "native-codex")]
use server::codex_native::{Client, Launch, ThreadOptions};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

fn launch(root: &std::path::Path) -> Launch {
    let binary = PathBuf::from(
        std::env::var_os("CODEX_NATIVE_BIN")
            .expect("native-codex acceptance requires the pinned binary"),
    );
    Launch {
        binary,
        home: root.to_path_buf(),
        codex_home: root.to_path_buf(),
        working_directory: root.to_path_buf(),
        executable_path: std::env::var_os("PATH").unwrap_or_default(),
        native_environment: BTreeMap::new(),
        custom_provider: None,
    }
}

#[tokio::test]
async fn official_account_operations_respect_native_login_policy() {
    use server::codex_native::{LoginCancellationStatus, NativeFailure};
    let root = tempfile::tempdir().unwrap();
    // The released binary ignores the debug-only login issuer override. Exercise
    // its real policy/absence behavior offline, not a simulated OAuth success.
    std::fs::write(
        root.path().join("config.toml"),
        "cli_auth_credentials_store = \"file\"\nforced_login_method = \"api\"\n",
    )
    .unwrap();
    let mut client = Client::start(launch(root.path())).await.unwrap();
    assert!(matches!(
        client.device_login().await,
        Err(NativeFailure::Rejected(-32600))
    ));
    assert_eq!(
        client
            .cancel_login("00000000-0000-4000-8000-000000000001")
            .await
            .unwrap()
            .status,
        LoginCancellationStatus::NotFound
    );
    assert!(client.account().await.unwrap().account.is_none());
    assert!(!root.path().join("auth.json").exists());
    client.logout().await.unwrap();
    assert!(client.account().await.unwrap().account.is_none());
    assert!(!root.path().join("auth.json").exists());
    client.close().await.unwrap();
}

#[tokio::test]
async fn official_stdio_initialization_catalog_and_default_thread_are_native_not_mocked() {
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::start(launch(root.path())).await.unwrap();
    let account = client.account().await.unwrap();
    assert!(
        account.account.is_none(),
        "isolated native acceptance must not inherit a real account"
    );
    let models = client.models().await.unwrap();
    assert!(!models.is_empty());
    let mut options = ThreadOptions::read_only(root.path().to_path_buf());
    options.ephemeral = true;
    let thread = client.start_thread(&options).await.unwrap();
    assert!(!thread.model.is_empty());
    assert!(!thread.model_provider.is_empty());
    let model = models
        .iter()
        .find(|model| model.model == thread.model)
        .expect("actual native effective model missing from complete catalog");
    options.reasoning_effort = Some(model.default_reasoning_effort.clone());
    let configured = client.start_thread(&options).await.unwrap();
    assert_eq!(configured.reasoning_effort, options.reasoning_effort);
    assert_ne!(configured.thread.id, thread.thread.id);
    assert!(!client.is_closed());
    let _ = client
        .observations(Duration::from_millis(10))
        .await
        .unwrap();
    client.close().await.unwrap();
    assert!(!root.path().join("auth.json").exists());
}

#[tokio::test]
async fn native_empty_thread_is_not_misrepresented_as_a_resumable_persisted_session() {
    let root = tempfile::tempdir().unwrap();
    let mut client = Client::start(launch(root.path())).await.unwrap();
    let options = ThreadOptions::read_only(root.path().to_path_buf());
    let started = client.start_thread(&options).await.unwrap();
    // Upstream rust-v0.144.4 tests/suite/v2/thread_resume.rs explicitly rejects
    // resume before the first user message materializes native rollout storage.
    // Preserve that observation instead of creating a replacement thread/history.
    assert!(matches!(
        client.resume_thread(&started.thread.id, &options).await,
        Err(server::codex_native::NativeFailure::Rejected(-32600))
    ));
    assert!(!client.is_closed());
    assert!(client.account().await.unwrap().account.is_none());
    client.close().await.unwrap();
}

#[path = "support/codex_responses.rs"]
mod responses;

#[tokio::test]
async fn native_completed_turn_survives_process_restart_and_results_return_to_the_same_thread() {
    let root = tempfile::tempdir().unwrap();
    let provider = responses::Provider::start(root.path()).await;
    let options = ThreadOptions::read_only(root.path().to_path_buf());
    let mut first = Client::start(launch(root.path())).await.unwrap();
    assert!(first.account().await.unwrap().account.is_none());
    let thread = first.start_thread(&options).await.unwrap();
    assert_eq!(thread.model_provider, "local_fixture");
    let initial = first
        .start_turn(
            "first-controlled-send",
            &thread.thread.id,
            responses::FIRST_PROMPT,
        )
        .await
        .unwrap();
    let initial_usage = responses::completed(&mut first, &thread.thread.id, &initial.id).await;
    assert_eq!(initial_usage.total, 12);
    assert_eq!(provider.request_count(), 1);
    first.close().await.unwrap();

    // A new real official process restores its own native storage. The test does
    // not read, manufacture, edit or export canonical history files or items.
    let mut second = Client::start(launch(root.path())).await.unwrap();
    let resumed = second
        .resume_thread(&thread.thread.id, &options)
        .await
        .unwrap();
    assert_eq!(resumed.thread.id, thread.thread.id);
    let recovered = second.turns(&thread.thread.id).await.unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].id, initial.id);
    assert_eq!(
        recovered[0].status,
        server::codex_native::TurnStatus::Completed
    );
    let next = second
        .start_turn(
            "second-controlled-send",
            &thread.thread.id,
            responses::SECOND_PROMPT,
        )
        .await
        .unwrap();
    let cumulative = responses::completed(&mut second, &thread.thread.id, &next.id).await;
    assert_eq!(cumulative.since(initial_usage).unwrap().total, 12);
    let turns = second.turns(&thread.thread.id).await.unwrap();
    assert_eq!(turns.len(), 2);
    assert!(turns
        .iter()
        .all(|turn| turn.status == server::codex_native::TurnStatus::Completed));
    assert_eq!(provider.request_count(), 2);
    assert!(provider.saw_previous_context());
    second.close().await.unwrap();
    assert!(!root.path().join("auth.json").exists());
}
