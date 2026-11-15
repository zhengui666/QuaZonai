//! CI-only public adapter boundary test. No native source patches, no user keys,
//! no WebSocket connection, no exchange actions, and no claim of reconnect coverage.
#![cfg(feature = "account-runtime")]
use nautilus_common::{cache::Cache, clients::ExecutionClient};
use nautilus_hyperliquid::{
    common::{
        consts::{HYPERLIQUID_CLIENT_ID, HYPERLIQUID_VENUE},
        enums::{HyperliquidEnvironment, HyperliquidProductType},
    },
    config::HyperliquidExecutionClientConfig,
    execution::HyperliquidExecutionClient,
};
use nautilus_live::ExecutionClientCore;
use nautilus_model::{
    enums::{AccountType, OmsType},
    identifiers::{AccountId, TraderId},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    io::{Read, Write},
    net::TcpListener,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

const ACCOUNT: &str = "0x2222222222222222222222222222222222222222";
// Public, disposable fixture key. Never used to sign or submit any action.
const FIXTURE_KEY: &str = "0000000000000000000000000000000000000000000000000000000000000001";
struct InfoFixture {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<thread::JoinHandle<()>>,
}
impl InfoFixture {
    fn new(order_count: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let capture = requests.clone();
        let stopped = stop.clone();
        let server = thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("fixture accept failed: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut data = Vec::new();
                let mut chunk = [0; 4096];
                let (header_end, length) = loop {
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&chunk[..n]);
                    assert!(data.len() < 1_048_576);
                    if let Some(end) = data.windows(4).position(|v| v == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&data[..end]);
                        assert!(
                            headers.starts_with("POST /info "),
                            "unexpected network operation: {headers}"
                        );
                        let length = headers
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().unwrap())
                            })
                            .expect("fixture requires bounded request");
                        assert!(length < 1_048_576);
                        break (end + 4, length);
                    }
                };
                while data.len() < header_end + length {
                    let n = stream.read(&mut chunk).unwrap();
                    assert!(n > 0);
                    data.extend_from_slice(&chunk[..n]);
                }
                let request: Value =
                    serde_json::from_slice(&data[header_end..header_end + length]).unwrap();
                capture.lock().unwrap().push(request.clone());
                let response = match request["type"].as_str().unwrap() {
                    "historicalOrders" => Value::Array((0..order_count).map(|index| json!({"order":{"coin":"BTC","side":"B","limitPx":"100","sz":"1","oid":index+1,"timestamp":1000,"origSz":"1"},"status":"canceled","statusTimestamp":1001})).collect()),
                    "userFills" | "frontendOpenOrders" => json!([]),
                    "perpDexs" => json!([null]),
                    "clearinghouseState" => json!({"assetPositions":[]}),
                    kind => panic!("unexpected Info method: {kind}"),
                };
                let body = serde_json::to_vec(&response).unwrap();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        Self {
            url,
            requests,
            stop,
            server: Some(server),
        }
    }
}
impl Drop for InfoFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.server.take().unwrap().join().unwrap();
    }
}

#[test]
fn public_mass_status_marks_saturated_order_history_incomplete() {
    if std::env::var_os("QZ_ISOLATED_HISTORY_FIXTURE").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "public_mass_status_marks_saturated_order_history_incomplete",
                "--nocapture",
            ])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("QZ_ISOLATED_HISTORY_FIXTURE", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated fixture failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_mass_status());
}
async fn run_mass_status() {
    for count in [0, 1999, 2000] {
        let fixture = InfoFixture::new(count);
        let core = ExecutionClientCore::new(
            TraderId::from("TESTER-COVERAGE"),
            *HYPERLIQUID_CLIENT_ID,
            *HYPERLIQUID_VENUE,
            OmsType::Netting,
            AccountId::from("HYPERLIQUID-COVERAGE"),
            AccountType::Margin,
            None,
            Rc::new(RefCell::new(Cache::default())),
        );
        let config = HyperliquidExecutionClientConfig {
            private_key: Some(FIXTURE_KEY.to_owned().into()),
            account_address: Some(ACCOUNT.to_owned()),
            environment: HyperliquidEnvironment::Testnet,
            product_types: vec![HyperliquidProductType::Perp],
            base_url_http: Some(format!("{}/info", fixture.url)),
            base_url_exchange: Some(format!("{}/forbidden-exchange", fixture.url)),
            base_url_ws: Some(fixture.url.replacen("http://", "ws://", 1)),
            http_timeout_secs: 2,
            ..Default::default()
        };
        let client = HyperliquidExecutionClient::new(core, config).unwrap();
        let mass = tokio::time::timeout(Duration::from_secs(10), client.generate_mass_status(None))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(mass.reports_complete(), count < 2000);
        assert!(mass.order_reports().is_empty());
        assert!(mass.fill_reports().is_empty());
        assert!(mass.position_reports().is_empty());
        let requests = fixture.requests.lock().unwrap();
        assert!(requests.iter().any(|r| r["type"] == "historicalOrders"));
        assert!(requests.iter().any(|r| r["type"] == "userFills"));
        for request in requests.iter().filter(|r| r["type"] != "perpDexs") {
            assert_eq!(
                request["user"], ACCOUNT,
                "query must use explicit account, not fixture signer"
            );
        }
        // Server accepts only POST /info and the five read methods above. Any
        // exchange/WS/bootstrap operation fails instead of reaching the Internet.
    }
}
