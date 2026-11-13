//! Read-only spot facts from the official unsigned Hyperliquid client. These are
//! observations, never an atomic reservation, executable balance or trade permit.
use anyhow::{anyhow, Result};
use nautilus_hyperliquid::http::client::HyperliquidHttpClient;
use nautilus_model::types::AccountBalance;
use serde_json::{json, Value};

#[derive(Clone)]
pub(crate) struct SpotObservationBinding {
    pub account_id: String,
    pub source_id: String,
    pub address: String,
    pub network: &'static str,
}
fn local_time_ns() -> Result<String> {
    Ok(chrono::Utc::now()
        .timestamp_nanos_opt()
        .ok_or_else(|| anyhow!("account_native_clock_range"))?
        .to_string())
}
fn project(
    binding: &SpotObservationBinding,
    started: &str,
    received: &str,
    balances: Option<&[AccountBalance]>,
    error: Option<String>,
) -> Value {
    json!({
        "kind":"NAUTILUS_SPOT_BALANCE_OBSERVATION", "observation_id":contracts::Id::new().to_string(), "account_id":binding.account_id,
        "source_id":binding.source_id, "account_address":binding.address, "network":binding.network,
        "request_started_ns":started, "received_ns":received, "exchange_timestamp_ns":null,
        "balances":balances, "error":error, "atomic_reservation":false,
        "trade_permission":false
    })
}
pub(crate) async fn read(
    http: &HyperliquidHttpClient,
    binding: &SpotObservationBinding,
) -> Result<Value> {
    let started = local_time_ns()?;
    // No signer or execution client. The address is copied from the explicitly
    // bound account configuration, never accepted from an incoming control payload.
    let result = http.request_spot_balances(&binding.address).await;
    let received = local_time_ns()?;
    Ok(match result {
        Ok(balances) => project(binding, &started, &received, Some(&balances), None),
        Err(error) => project(binding, &started, &received, None, Some(error.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::types::{Currency, Money};
    #[test]
    fn spot_observation_preserves_official_currencies_and_unknowns() {
        let binding = SpotObservationBinding {
            account_id: "BOUND-001".into(),
            source_id: "source".into(),
            address: "explicit-address".into(),
            network: "mainnet",
        };
        let balances = vec![AccountBalance::new(
            Money::new(10.0, Currency::EUR()),
            Money::new(2.0, Currency::EUR()),
            Money::new(8.0, Currency::EUR()),
        )];
        let observation = project(&binding, "100", "200", Some(&balances), None);
        assert_eq!(
            observation["balances"],
            serde_json::to_value(&balances).unwrap()
        );
        assert_eq!(observation["account_address"], "explicit-address");
        assert_eq!(observation["request_started_ns"], "100");
        assert_eq!(observation["received_ns"], "200");
        assert!(observation["exchange_timestamp_ns"].is_null());
        assert!(observation["free_usdc"].is_null());
        assert_eq!(observation["atomic_reservation"], false);
        assert_eq!(
            project(&binding, "100", "200", Some(&[]), None)["balances"],
            json!([])
        );
        let failed = project(&binding, "100", "200", None, Some("read failed".into()));
        assert!(failed["balances"].is_null());
        assert_eq!(failed["error"], "read failed");
    }
    #[tokio::test(flavor = "current_thread")]
    async fn official_unsigned_spot_client_reads_only_bound_local_info() -> Result<()> {
        use nautilus_hyperliquid::common::enums::HyperliquidEnvironment;
        use std::{
            io::{Read, Write},
            net::TcpListener,
            time::{Duration, Instant},
        };
        let valid = r#"{"balances":[{"coin":"USDC","token":0,"total":"100.25","hold":"10","entryNtl":"0"}]}"#;
        let zero =
            r#"{"balances":[{"coin":"USDC","token":0,"total":"0","hold":"0","entryNtl":"0"}]}"#;
        let damaged = r#"{"balances":[{"coin":"USDC","token":0,"total":"invalid","hold":"0","entryNtl":"0"}]}"#;
        for (body, outcome) in [(valid, "valid"), (zero, "empty"), (damaged, "error")] {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let address = listener.local_addr()?;
            listener.set_nonblocking(true)?;
            let server = std::thread::spawn(move || -> Result<Value> {
                let deadline = Instant::now() + Duration::from_secs(10);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) => return Err(error.into()),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                let mut bytes = Vec::new();
                let mut buffer = [0u8; 4096];
                let (header_end, length) = loop {
                    let count = stream.read(&mut buffer)?;
                    anyhow::ensure!(count > 0, "missing HTTP request");
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                        let header = std::str::from_utf8(&bytes[..end])?;
                        anyhow::ensure!(
                            header.starts_with("POST /info HTTP/1.1"),
                            "unexpected endpoint"
                        );
                        anyhow::ensure!(
                            !header.to_lowercase().contains("authorization:"),
                            "unexpected authorization"
                        );
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>())
                            })
                            .transpose()?
                            .ok_or_else(|| anyhow!("missing request length"))?;
                        break (end + 4, length);
                    }
                };
                while bytes.len() < header_end + length {
                    let count = stream.read(&mut buffer)?;
                    anyhow::ensure!(count > 0, "incomplete request");
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let request: Value =
                    serde_json::from_slice(&bytes[header_end..header_end + length])?;
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body)?;
                Ok(request)
            });
            let mut client = HyperliquidHttpClient::new(HyperliquidEnvironment::Testnet, 5, None)?;
            client.set_base_info_url(format!("http://{address}/info"));
            let binding = SpotObservationBinding {
                account_id: "TEST-001".into(),
                source_id: "test-source".into(),
                address: "0x0000000000000000000000000000000000000001".into(),
                network: "testnet",
            };
            let observation = read(&client, &binding).await?;
            let received = server
                .join()
                .map_err(|_| anyhow!("local server panicked"))??;
            assert_eq!(
                received,
                json!({"type":"spotClearinghouseState","user":binding.address})
            );
            if outcome == "error" {
                assert!(observation["balances"].is_null());
                assert!(observation["error"].as_str().is_some());
            } else {
                assert!(observation["error"].is_null());
                let expected = if outcome == "empty" {
                    vec![]
                } else {
                    vec![AccountBalance::new(
                        Money::new(100.25, Currency::USDC()),
                        Money::new(10.0, Currency::USDC()),
                        Money::new(90.25, Currency::USDC()),
                    )]
                };
                assert_eq!(observation["balances"], serde_json::to_value(expected)?);
            }
            assert!(observation["request_started_ns"].as_str().is_some());
            assert!(observation["received_ns"].as_str().is_some());
            assert!(observation["exchange_timestamp_ns"].is_null());
        }
        Ok(())
    }
}
