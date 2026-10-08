use super::*;
use contracts::Id;
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Instant,
};

const FIXTURE: &[u8] =
    include_bytes!("../../../tests/contracts/native-account-paper-snapshot.json");

#[test]
fn relay_parses_but_cannot_be_built_as_a_single_request() {
    use clap::Parser;
    #[derive(Parser)]
    struct TestParser {
        #[command(flatten)]
        client: super::super::Arguments,
    }
    let parsed = TestParser::try_parse_from([
        "client",
        "forward",
        "accounts",
        "relay",
        "--input",
        "file-that-must-never-be-opened-as-a-single-request.ndjson",
        "--follow",
    ])
    .unwrap();
    assert!(matches!(
        parsed.client.command.request_for(true),
        Err(Failure::Input)
    ));
}

#[test]
fn prepared_account_request_keeps_original_bytes_and_native_values() {
    let original = format!("  {}\r\n", std::str::from_utf8(FIXTURE).unwrap().trim());
    let (request, observation) = prepare(original.as_bytes()).unwrap();
    assert_eq!(request.body.as_deref(), Some(original.as_bytes()));
    assert_eq!(
        observation,
        serde_json::from_slice::<AccountObservationSubmitV1>(FIXTURE).unwrap()
    );
    assert_eq!(request.status, 201);
    assert!(!request.operator);
    assert!(!request.requires_idempotency_key());
    let plan = preview::inspect(&request, "http://localhost:9", false, None, None).unwrap();
    assert_eq!(plan["requires_idempotency_key"], false);
    assert_eq!(plan["body_bytes"], original.len());
    assert_eq!(plan["body_redacted"], true);
    assert_eq!(plan["request_sent"], false);
    assert!(!plan.to_string().contains("SIM-001"));
}

#[tokio::test]
async fn all_other_write_routes_still_require_the_same_key_in_preview_and_sender() {
    use reqwest::Method;
    let (connection, listener, _) = connection();
    listener.set_nonblocking(true).unwrap();
    for (method, route) in [
        (Method::POST, "/api/v2/forward/weights"),
        (Method::POST, "/api/v2/projects"),
        (Method::PATCH, "/api/v2/forward/account-observations"),
        (Method::DELETE, "/api/v2/forward/account-observations"),
        (Method::POST, "/api/v2/forward/account-observations/other"),
    ] {
        let (mut request, _) = prepare(FIXTURE).unwrap();
        request.method = method;
        request.route = route.into();
        assert!(request.requires_idempotency_key());
        assert!(matches!(
            preview::inspect(&request, "http://localhost:9", false, None, None),
            Err(Failure::IdempotencyRequired)
        ));
        assert!(matches!(
            connection.send(&request, None, None).await,
            Err(Failure::IdempotencyRequired)
        ));
        let plan = preview::inspect(
            &request,
            "http://localhost:9",
            false,
            Some("same-command-intent"),
            None,
        )
        .unwrap();
        assert_eq!(plan["requires_idempotency_key"], true);
    }
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn native_report_preview_remains_a_json_read() {
    use super::super::commands::{Command, Experiment};
    let request = Command::Experiment(Experiment::Result {
        id: Id::new().to_string(),
    })
    .request()
    .unwrap();
    let plan = preview::inspect(&request, "http://localhost:9", false, None, None).unwrap();
    assert_eq!(plan["output"], "json");
    assert_eq!(plan["method"], "GET");
    assert_eq!(plan["requires_idempotency_key"], false);
}

#[test]
fn retained_input_waits_for_complete_line_and_preserves_replay_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.ndjson");
    let split = FIXTURE.len() / 2;
    fs::write(&path, &FIXTURE[..split]).unwrap();
    let mut input = RetainedInput::open(&path).unwrap();
    assert!(input.next(true).unwrap().is_none());
    let mut writer = OpenOptions::new().append(true).open(&path).unwrap();
    writer.write_all(&FIXTURE[split..]).unwrap();
    let line = input.next(true).unwrap().unwrap();
    assert_eq!(line, FIXTURE);
    assert!(input.next(true).unwrap().is_none());
    let replay = RetainedInput::open(&path)
        .unwrap()
        .next(false)
        .unwrap()
        .unwrap();
    assert_eq!(prepare(&line).unwrap().1, prepare(&replay).unwrap().1);
    assert_eq!(prepare(&line).unwrap().0.body.unwrap(), FIXTURE);
    assert_eq!(fs::read(path).unwrap(), FIXTURE);
}

#[test]
fn partial_finite_input_and_invalid_envelopes_are_explicit_failures() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.ndjson");
    fs::write(&path, &FIXTURE[..FIXTURE.len() - 1]).unwrap();
    assert!(matches!(
        RetainedInput::open(&path).unwrap().next(false),
        Err(Failure::AccountStreamIncomplete)
    ));
    assert!(prepare(b"\n").is_err());
    let mut unknown: Value = serde_json::from_slice(FIXTURE).unwrap();
    unknown["invented_field"] = json!(true);
    assert!(prepare(&serde_json::to_vec(&unknown).unwrap()).is_err());
}

#[test]
fn truncation_and_path_replacement_never_reset_the_reader() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.ndjson");
    fs::write(&path, FIXTURE).unwrap();
    let mut input = RetainedInput::open(&path).unwrap();
    input.next(true).unwrap().unwrap();
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(0)
        .unwrap();
    assert!(matches!(
        input.next(true),
        Err(Failure::AccountStreamChanged)
    ));
    fs::write(&path, FIXTURE).unwrap();
    let mut input = RetainedInput::open(&path).unwrap();
    fs::rename(&path, directory.path().join("old-segment.ndjson")).unwrap();
    fs::write(&path, FIXTURE).unwrap();
    assert!(matches!(
        input.next(true),
        Err(Failure::AccountStreamChanged)
    ));
}

#[test]
#[cfg(unix)]
fn fifo_path_replacement_and_open_race_fail_within_a_bounded_subprocess() {
    use std::process::{Command, Stdio};
    const CHILD: &str = "QUAZONAI_ACCOUNT_FIFO_TEST_CHILD";
    if let Some(marker) = std::env::var_os(CHILD) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retained.ndjson");
        let retained = directory.path().join("original.ndjson");
        fs::write(&path, FIXTURE).unwrap();
        let mut input = RetainedInput::open(&path).unwrap();
        assert_eq!(input.next(true).unwrap().unwrap(), FIXTURE);
        // Metadata succeeds on the original file. Replace it before the actual
        // open stage to exercise the check/open race without timing guesses.
        assert!(fs::metadata(&path).unwrap().is_file());
        fs::rename(&path, &retained).unwrap();
        assert!(Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success());
        assert!(open_regular_file(&path).is_err());
        assert!(matches!(
            input.next(true),
            Err(Failure::AccountStreamChanged)
        ));
        assert!(matches!(RetainedInput::open(&path), Err(Failure::Input)));
        assert_eq!(fs::read(retained).unwrap(), FIXTURE);
        fs::write(marker, b"passed").unwrap();
        return;
    }
    let test = concat!(
        module_path!(),
        "::fifo_path_replacement_and_open_race_fail_within_a_bounded_subprocess"
    );
    // module_path includes the crate name; libtest's exact name starts below it.
    let (_, test) = test.split_once("::").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("child-completed");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, &marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "FIFO replacement child failed");
            assert_eq!(
                fs::read(&marker).unwrap(),
                b"passed",
                "child test did not run"
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("FIFO replacement blocked the retained-file reader");
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let start = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                // Accepted sockets can inherit the listener's nonblocking mode.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(start.elapsed() < Duration::from_secs(5), "no relay request");
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept: {error}"),
        }
    }
}

fn receive(stream: &mut TcpStream) -> (String, Vec<u8>) {
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
        assert!(headers.len() < 16 * 1024);
    }
    let headers = String::from_utf8(headers).unwrap();
    assert!(headers.starts_with("POST /api/v2/forward/account-observations HTTP/1.1\r\n"));
    let length: usize = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .unwrap()
        .parse()
        .unwrap();
    let mut body = vec![0; length];
    stream.read_exact(&mut body).unwrap();
    (headers, body)
}

fn respond(stream: &mut TcpStream, status: u16, media: &str, body: &Value) {
    let bytes = serde_json::to_vec(body).unwrap();
    write!(
        stream,
        "HTTP/1.1 {status} Result\r\nContent-Type: {media}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        bytes.len()
    ).unwrap();
    stream.write_all(&bytes).unwrap();
}

fn receipt(body: &[u8], replayed: bool) -> Value {
    json!({
        "replayed": replayed,
        "resource": {
            "id": Id::new(), "source_id": Id::new(), "downstream_id": Id::new(),
            "observation": serde_json::from_slice::<Value>(body).unwrap(),
            "gap_before": false, "received_at": "2026-10-02T00:00:00Z"
        }
    })
}

fn connection() -> (Connection, TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let token = integrations::authentication::format_machine_token(
        Id::new(),
        &integrations::authentication::random_capability(),
    )
    .unwrap();
    let connection = Connection::connect(
        format!("http://{}", listener.local_addr().unwrap()),
        token.clone(),
        false,
        None,
    )
    .unwrap();
    (connection, listener, token)
}

#[tokio::test]
async fn unknown_delivery_retries_the_same_authenticated_request_then_replays_on_restart() {
    let (connection, listener, token) = connection();
    let server = thread::spawn(move || {
        let mut requests = vec![];
        let accepted = receipt(FIXTURE, true);
        for attempt in 0..3 {
            let mut stream = accept(&listener);
            let (headers, body) = receive(&mut stream);
            assert!(headers.contains(&format!("authorization: Bearer {token}\r\n")));
            assert!(!headers.contains("idempotency-key:"));
            requests.push(body.clone());
            // First request is accepted, but its HTTP response is lost. Only the
            // same envelope can safely be retried; later replay remains identical.
            if attempt != 0 {
                respond(&mut stream, 201, "application/json", &accepted);
            }
        }
        requests
    });
    let (request, expected) = prepare(FIXTURE).unwrap();
    let original = submit(&connection, &request, &expected, Some(2), Duration::ZERO)
        .await
        .unwrap();
    assert!(original.replayed);
    let (replay_request, replay_expected) = prepare(FIXTURE).unwrap();
    let replay = submit(
        &connection,
        &replay_request,
        &replay_expected,
        Some(1),
        Duration::ZERO,
    )
    .await
    .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(&replay.resource).unwrap(),
        serde_json::to_value(&original.resource).unwrap()
    );
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(requests[0], FIXTURE);
}

#[tokio::test]
async fn mismatched_receipt_stops_and_permanent_rejection_is_not_retried() {
    for mismatch in [true, false] {
        let (connection, listener, _) = connection();
        let server = thread::spawn(move || {
            let mut stream = accept(&listener);
            let (_, body) = receive(&mut stream);
            if mismatch {
                let mut value = receipt(&body, false);
                value["resource"]["observation"]["observed_at_ns"] = json!("1");
                respond(&mut stream, 201, "application/json", &value);
            } else {
                respond(
                    &mut stream,
                    409,
                    "application/problem+json",
                    &json!({
                        "type": "urn:quazonai:problem:native_identity_conflict",
                        "title": "Conflict", "status": 409, "code": "native_identity_conflict",
                        "detail": "Changed envelope", "request_id": Id::new(),
                        "retryable": false, "field_errors": [], "safe_next_actions": []
                    }),
                );
            }
            listener
        });
        let (request, expected) = prepare(FIXTURE).unwrap();
        let result = submit(&connection, &request, &expected, Some(3), Duration::ZERO).await;
        if mismatch {
            assert!(matches!(result, Err(Failure::Contract)));
        } else {
            assert!(matches!(result, Err(Failure::Rejected(_))));
        }
        let listener = server.join().unwrap();
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[tokio::test]
async fn retryable_problem_exhaustion_is_bounded_and_retains_original_input() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.ndjson");
    fs::write(&path, FIXTURE).unwrap();
    let (connection, listener, _) = connection();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let mut stream = accept(&listener);
            let (_, body) = receive(&mut stream);
            assert_eq!(body, FIXTURE);
            respond(
                &mut stream,
                503,
                "application/problem+json",
                &json!({
                    "type": "urn:quazonai:problem:unavailable", "title": "Unavailable",
                    "status": 503, "code": "unavailable", "detail": "Try the same envelope",
                    "request_id": Id::new(), "retryable": true,
                    "field_errors": [], "safe_next_actions": []
                }),
            );
        }
        listener
    });
    let line = RetainedInput::open(&path)
        .unwrap()
        .next(false)
        .unwrap()
        .unwrap();
    let (request, expected) = prepare(&line).unwrap();
    assert!(matches!(
        submit(&connection, &request, &expected, Some(2), Duration::ZERO).await,
        Err(Failure::Rejected(problem)) if problem.retryable && problem.status == 503
    ));
    assert_eq!(
        server.join().unwrap().accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read(path).unwrap(), FIXTURE);
}

#[test]
fn retained_records_cross_the_former_size_boundary_without_losing_replay_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.ndjson");
    let mut original = vec![b' '; 2 * 1024 * 1024 + 1];
    original.extend_from_slice(FIXTURE);
    // Exercise a partial record crossing the old limit before its newline arrives.
    let split = original.len() - 1;
    fs::write(&path, &original[..split]).unwrap();
    let mut input = RetainedInput::open(&path).unwrap();
    assert!(input.next(true).unwrap().is_none());
    OpenOptions::new().append(true).open(&path).unwrap()
        .write_all(&original[split..]).unwrap();
    let record = input.next(true).unwrap().unwrap();
    assert_eq!(record, original);
    let (request, observation) = prepare(&record).unwrap();
    assert_eq!(request.body.unwrap(), original);
    assert_eq!(observation, serde_json::from_slice::<AccountObservationSubmitV1>(FIXTURE).unwrap());
    assert!(input.next(false).unwrap().is_none());
    assert_eq!(fs::read(path).unwrap(), original);
}

#[tokio::test]
async fn large_original_account_record_and_receipt_keep_the_same_replay_identity() {
    let (connection, listener, _) = connection();
    let mut original = vec![b' '; 2 * 1024 * 1024 + 1];
    original.extend_from_slice(FIXTURE);
    let expected_bytes = original.clone();
    let server = thread::spawn(move || {
        let mut stream = accept(&listener);
        let (_, body) = receive(&mut stream);
        assert_eq!(body, expected_bytes);
        let mut reply = serde_json::to_vec(&receipt(&body, false)).unwrap();
        reply.resize(2 * 1024 * 1024 + 16 * 1024 + 1, b' ');
        write!(stream, "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.len()).unwrap();
        stream.write_all(&reply).unwrap();
    });
    let (request, expected) = prepare(&original).unwrap();
    let received = submit(&connection, &request, &expected, Some(1), Duration::ZERO).await.unwrap();
    assert_eq!(received.resource.observation, expected);
    server.join().unwrap();
}

#[test]
fn relay_attempts_are_absent_by_default_and_not_capped_at_one_hundred() {
    use clap::Parser;
    #[derive(Parser)]
    struct TestParser { #[command(flatten)] relay: Arguments }
    let request = TestParser::try_parse_from(["relay", "--input", "never-opened.ndjson"]).unwrap();
    assert_eq!(request.relay.max_attempts, None);
    let request = TestParser::try_parse_from(["relay", "--input", "never-opened.ndjson", "--max-attempts", "101"]).unwrap();
    assert_eq!(request.relay.max_attempts, Some(101));
    assert!(TestParser::try_parse_from(["relay", "--input", "never-opened.ndjson", "--max-attempts", "0"]).is_err());
}

#[tokio::test]
async fn absent_attempt_cap_retries_past_the_old_default_without_changing_the_envelope() {
    let (connection, listener, _) = connection();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for attempt in 0..7 {
            let mut stream = accept(&listener);
            let (_, body) = receive(&mut stream);
            requests.push(body);
            if attempt == 6 {
                respond(&mut stream, 201, "application/json", &receipt(FIXTURE, true));
            }
        }
        requests
    });
    let (request, expected) = prepare(FIXTURE).unwrap();
    assert!(submit(&connection, &request, &expected, None, Duration::ZERO).await.unwrap().replayed);
    let requests = server.join().unwrap();
    assert_eq!(requests.len(), 7);
    assert!(requests.iter().all(|bytes| bytes == FIXTURE));
}
