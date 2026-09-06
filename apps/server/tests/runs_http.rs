//! Loopback HTTP against real Axum, private cookies, native crypto and PostgreSQL.
//! Scientific outcomes are not synthesized into qualification or delivery evidence.
#[path = "../../../crates/store/tests/support/mod.rs"]
mod domain_fixture;
mod support;
use axum::Router;
use chrono::{Duration, Utc};
use contracts::{lifecycle::*, runs::RunKind, DbCounter, Id, Revision, SchemaV1};
use reqwest::{Client, Response, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::lifecycle::*;
use support::*;

struct Http {
    url: String,
    client: Client,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Http {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Http {
    async fn start(app: Router) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap();
        Self { url, client, task }
    }
    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        cookie: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let request = self
            .client
            .request(method, format!("{}{}", self.url, path))
            .header("host", "research.example")
            .header("origin", "https://research.example");
        match cookie {
            Some(cookie) => request.header("cookie", cookie),
            None => request,
        }
    }
    async fn get(&self, path: &str, cookie: Option<&str>) -> Response {
        self.request(reqwest::Method::GET, path, cookie)
            .send()
            .await
            .unwrap()
    }
}
async fn authenticated(pool: PgPool) -> (Fixture, String) {
    let f = fixture(pool).await;
    let (enrollment, anonymous, native) = start(&f).await;
    let (reply, _) = confirm(&f, &enrollment, &anonymous, &native, true).await;
    assert_eq!(reply.status, axum::http::StatusCode::OK, "{}", reply.body);
    (f, reply.cookie.unwrap())
}
async fn admitted(pool: &PgPool, f: &Fixture, key: &str) -> contracts::runs::RunSnapshotV1 {
    let d = domain_fixture::fixture(pool, domain_fixture::budget()).await;
    sqlx::query("UPDATE app.runs SET state='FAILED',finished_at=clock_timestamp(),terminal_reason_code='FIXTURE_ENDED' WHERE id=$1").bind(d.run.as_uuid()).execute(pool).await.unwrap();
    let runtime = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'Native fixture','https://runtime.example','SYSTEM_CA','fixture',ARRAY['DATA_VALIDATE'],'1',true)").bind(runtime.as_uuid()).execute(pool).await.unwrap();
    f.store
        .enqueue_run(
            key,
            &RunSubmission {
                cycle_id: d.cycle,
                input_set_id: d.input_set,
                runtime_id: runtime,
                runtime_revision: Revision::INITIAL,
                kind: RunKind::DataValidate,
                limits: JobLimitsV1 {
                    schema_version: SchemaV1,
                    experiments: 1,
                    cpu_seconds: DbCounter::new(100).unwrap(),
                    wall_seconds: 3600,
                    memory_mib: 1024,
                    output_bytes: DbCounter::new(4096).unwrap(),
                },
            },
        )
        .await
        .unwrap()
        .resource
}
async fn json_reply(response: Response, expected: StatusCode) -> Value {
    let status = response.status();
    let value: Value = response.json().await.unwrap();
    assert_eq!(status, expected, "{value}");
    value
}
async fn one_event(response: &mut Response) -> String {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut text = String::new();
        while !text.contains("\n\n") {
            let bytes = response
                .chunk()
                .await
                .unwrap()
                .expect("event stream ended unexpectedly");
            text.push_str(std::str::from_utf8(&bytes).unwrap());
        }
        text
    })
    .await
    .expect("event must not depend on an in-memory notification")
}
fn frame_ids(text: &str) -> Vec<u64> {
    text.lines()
        .filter_map(|line| line.strip_prefix("id:"))
        .map(|id| id.trim().rsplit_once(':').unwrap().1.parse().unwrap())
        .collect()
}

#[sqlx::test(migrations = "../../migrations")]
async fn loopback_reads_and_cancel_use_real_auth_origin_revision_and_receipts(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let run = admitted(&pool, &f, "http").await;
    let http = Http::start(f.app.clone()).await;
    let path = format!("/api/v2/runs/{}", run.id);
    json_reply(http.get(&path, None).await, StatusCode::UNAUTHORIZED).await;
    let current = json_reply(http.get(&path, Some(&cookie)).await, StatusCode::OK).await;
    assert_eq!(current["id"], run.id.to_string());
    assert_eq!(current["last_event_seq"], "1");
    for query in ["limit=0", "limit=101", "limit=65536", "unknown=1"] {
        json_reply(
            http.get(&format!("/api/v2/runs?{query}"), Some(&cookie))
                .await,
            StatusCode::UNPROCESSABLE_ENTITY,
        )
        .await;
    }
    let body = json!({"schema_version":1,"expected_revision":run.revision});
    let mut response = http
        .request(
            reqwest::Method::POST,
            &format!("{path}/cancel"),
            Some(&cookie),
        )
        .header("idempotency-key", "cancel")
        .header("origin", "https://evil.example")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    response = http
        .request(
            reqwest::Method::POST,
            &format!("{path}/cancel"),
            Some(&cookie),
        )
        .header("idempotency-key", "stale")
        .json(&json!({"schema_version":1,"expected_revision":"1"}))
        .send()
        .await
        .unwrap();
    json_reply(response, StatusCode::CONFLICT).await;
    for replayed in [false, true] {
        let response = http
            .request(
                reqwest::Method::POST,
                &format!("{path}/cancel"),
                Some(&cookie),
            )
            .header("idempotency-key", "cancel")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        let value = json_reply(response, StatusCode::ACCEPTED).await;
        assert_eq!(value["resource"]["state"], "CANCELLED");
        assert_eq!(value["replayed"], replayed);
    }
    let original = &pool;
    let messages: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(run.id.to_string())
            .fetch_one(original)
            .await
            .unwrap();
    assert_eq!(
        messages, 1,
        "HTTP cancellation does not falsely acknowledge the Worker queue"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn sse_terminal_replay_drains_all_native_batches_and_resumes_exactly(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let run = admitted(&pool, &f, "many-events").await;
    let msg = f
        .store
        .read_run_messages(10, 20)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == run.id)
        .unwrap();
    let ClaimResult::Leased(mut lease) = f.store.claim_run(&msg, "worker", 30).await.unwrap()
    else {
        panic!("lease")
    };
    for n in 0..20 {
        sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
        let ClaimResult::Leased(next) = f
            .store
            .claim_run(&msg, &format!("worker-{n}"), 30)
            .await
            .unwrap()
        else {
            panic!("takeover")
        };
        lease = next;
    }
    let http = Http::start(f.app.clone()).await;
    let path = format!("/api/v2/runs/{}", run.id);
    let current = json_reply(http.get(&path, Some(&cookie)).await, StatusCode::OK).await;
    let response = http
        .request(
            reqwest::Method::POST,
            &format!("{path}/cancel"),
            Some(&cookie),
        )
        .header("idempotency-key", "cancel")
        .json(&json!({"schema_version":1,"expected_revision":current["revision"]}))
        .send()
        .await
        .unwrap();
    json_reply(response, StatusCode::ACCEPTED).await;
    let time: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let result = f
        .store
        .accept_run_terminal(
            run.id,
            &lease.fence,
            &TerminalObservation {
                schema_version: SchemaV1,
                external_job_id: lease.external_job_id,
                outcome: NativeOutcome::ConfirmedAbsent,
                manifest_artifact_id: None,
                failure_class: None,
                failure_code: None,
                observed_at: time,
            },
        )
        .await
        .unwrap();
    let last = result.resource.last_event_seq.get();
    assert!(last > 16);
    let response = http.get(&format!("{path}/events"), Some(&cookie)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers()["content-type"]
        .to_str()
        .unwrap()
        .starts_with("text/event-stream"));
    let text = response.text().await.unwrap();
    assert_eq!(frame_ids(&text), (1..=last).collect::<Vec<_>>());
    assert!(!text.contains("fixture-credential"));
    let text = http
        .request(
            reqwest::Method::GET,
            &format!("{path}/events"),
            Some(&cookie),
        )
        .header("last-event-id", format!("{}:17", run.id))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(frame_ids(&text), (18..=last).collect::<Vec<_>>());
    let response = http
        .request(
            reqwest::Method::GET,
            &format!("{path}/events"),
            Some(&cookie),
        )
        .header("last-event-id", format!("{}:{last}", run.id))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().await.unwrap(), "");
}

#[sqlx::test(migrations = "../../migrations")]
async fn sse_cursors_fail_before_headers_and_disconnect_does_not_cancel(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let run = admitted(&pool, &f, "cursor").await;
    let http = Http::start(f.app.clone()).await;
    let path = format!("/api/v2/runs/{}/events", run.id);
    for (cursor, status) in [
        (format!("{}:2", run.id), StatusCode::GONE),
        (format!("{}:01", run.id), StatusCode::UNPROCESSABLE_ENTITY),
        (format!("{}:1", Id::new()), StatusCode::UNPROCESSABLE_ENTITY),
        (
            format!("{}:9223372036854775808", run.id),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        let response = http
            .request(reqwest::Method::GET, &path, Some(&cookie))
            .header("last-event-id", cursor)
            .send()
            .await
            .unwrap();
        json_reply(response, status).await;
    }
    let mut response = http.get(&path, Some(&cookie)).await;
    let text = one_event(&mut response).await;
    assert_eq!(frame_ids(&text), vec![1]);
    drop(response);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let state: String = sqlx::query_scalar("SELECT state FROM app.runs WHERE id=$1")
        .bind(run.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "QUEUED");
    let mut again = http.get(&path, Some(&cookie)).await;
    assert_eq!(frame_ids(&one_event(&mut again).await), vec![1]);
}

#[sqlx::test(migrations = "../../migrations")]
async fn sse_notices_revoked_authority_without_leaking_future_events(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let run = admitted(&pool, &f, "revocation").await;
    let http = Http::start(f.app.clone()).await;
    let mut stream = http
        .get(&format!("/api/v2/runs/{}/events", run.id), Some(&cookie))
        .await;
    assert_eq!(frame_ids(&one_event(&mut stream).await), vec![1]);
    let response = http
        .request(reqwest::Method::POST, "/api/v2/auth/logout", Some(&cookie))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let text = one_event(&mut stream).await;
    assert!(text.contains("reset-required"));
    assert!(frame_ids(&text).is_empty());
    assert!(!text.contains("Postgres"));
    assert!(stream.chunk().await.unwrap().is_none());
    let state: String = sqlx::query_scalar("SELECT state FROM app.runs WHERE id=$1")
        .bind(run.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(state, "QUEUED");
}

#[sqlx::test(migrations = "../../migrations")]
async fn machine_run_scopes_are_checked_by_real_bearer_crypto_and_project_binding(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let own = admitted(&pool, &f, "own").await;
    let other = admitted(&pool, &f, "other").await;
    let http = Http::start(f.app.clone()).await;
    let response=http.request(reqwest::Method::POST,"/api/v2/machine-principals",Some(&cookie)).header("idempotency-key","principal").json(&json!({"schema_version":1,"name":"Run reader","kind":"CLI","project_id":own.project_id,"downstream_id":null,"enabled":true})).send().await.unwrap();
    let p = json_reply(response, StatusCode::CREATED).await;
    let response=http.request(reqwest::Method::POST,&format!("/api/v2/machine-principals/{}/credentials",p["resource"]["id"].as_str().unwrap()),Some(&cookie)).header("idempotency-key","credential").json(&json!({"schema_version":1,"scope_codes":["RUN_READ"],"expires_at":Utc::now()+Duration::hours(1)})).send().await.unwrap();
    let c = json_reply(response, StatusCode::CREATED).await;
    let token = c["token"].as_str().unwrap();
    let value = json_reply(
        http.request(reqwest::Method::GET, "/api/v2/runs", None)
            .bearer_auth(token)
            .send()
            .await
            .unwrap(),
        StatusCode::OK,
    )
    .await;
    assert!(value["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|run| run["project_id"] == own.project_id.to_string()));
    for path in [
        format!("/api/v2/runs/{}", other.id),
        format!("/api/v2/runs/{}/events", other.id),
    ] {
        json_reply(
            http.request(reqwest::Method::GET, &path, None)
                .bearer_auth(token)
                .send()
                .await
                .unwrap(),
            StatusCode::NOT_FOUND,
        )
        .await;
    }
    json_reply(
        http.request(
            reqwest::Method::POST,
            &format!("/api/v2/runs/{}/cancel", own.id),
            None,
        )
        .bearer_auth(token)
        .header("idempotency-key", "forbidden")
        .json(&json!({"schema_version":1,"expected_revision":own.revision}))
        .send()
        .await
        .unwrap(),
        StatusCode::FORBIDDEN,
    )
    .await;
    // A separately issued RUN_CANCEL capability succeeds through native crypto;
    // possessing only RUN_READ above never implied write authority.
    let response=http.request(reqwest::Method::POST,&format!("/api/v2/machine-principals/{}/credentials",p["resource"]["id"].as_str().unwrap()),Some(&cookie)).header("idempotency-key","canceller").json(&json!({"schema_version":1,"scope_codes":["RUN_CANCEL"],"expires_at":Utc::now()+Duration::hours(1)})).send().await.unwrap();
    let canceller = json_reply(response, StatusCode::CREATED).await;
    let response = http
        .request(
            reqwest::Method::POST,
            &format!("/api/v2/runs/{}/cancel", own.id),
            None,
        )
        .bearer_auth(canceller["token"].as_str().unwrap())
        .header("idempotency-key", "allowed")
        .json(&json!({"schema_version":1,"expected_revision":own.revision}))
        .send()
        .await
        .unwrap();
    let result = json_reply(response, StatusCode::ACCEPTED).await;
    assert_eq!(result["resource"]["state"], "CANCELLED");
}

#[sqlx::test(migrations = "../../migrations")]
async fn sse_connections_are_bounded_and_disconnected_permits_are_released(pool: PgPool) {
    let (f, cookie) = authenticated(pool.clone()).await;
    let run = admitted(&pool, &f, "bounded").await;
    let http = Http::start(f.app.clone()).await;
    let path = format!("/api/v2/runs/{}/events", run.id);
    let mut streams = Vec::new();
    for _ in 0..32 {
        let response = http.get(&path, Some(&cookie)).await;
        assert_eq!(response.status(), StatusCode::OK);
        streams.push(response);
    }
    let response = http.get(&path, Some(&cookie)).await;
    let body = json_reply(response, StatusCode::TOO_MANY_REQUESTS).await;
    assert_eq!(body["code"], "STREAM_LIMIT");
    drop(streams);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let response = http.get(&path, Some(&cookie)).await;
            if response.status() == StatusCode::OK {
                break;
            }
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let status: String = sqlx::query_scalar("SELECT state FROM app.runs WHERE id=$1")
        .bind(run.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "QUEUED");
}
