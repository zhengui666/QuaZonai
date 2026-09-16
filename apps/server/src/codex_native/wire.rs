//! Small bounded JSONL transport for the native App Server, not an Agent loop.
//! Unselected payload fields remain RawValue/Serde IgnoredAny and are never logged.
use super::{projection, NativeFailure, Observation, Result, MAX_FRAME};
use serde::{Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use std::{collections::VecDeque, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(super) enum RequestId {
    Text(String),
    Number(i64),
}

#[derive(Deserialize)]
struct NativeError {
    code: i64,
}

#[derive(Deserialize)]
struct Frame {
    id: Option<RequestId>,
    method: Option<String>,
    params: Option<Box<RawValue>>,
    result: Option<Box<RawValue>>,
    error: Option<NativeError>,
}

pub(super) struct Wire<R, W> {
    reader: BufReader<R>,
    writer: W,
    // Kept across cancellation-safe fill_buf calls. A bounded poll timeout must
    // not discard a partial notification or join it to the next frame.
    partial: Vec<u8>,
    observations: VecDeque<Observation>,
    closed: bool,
    // Left set when an RPC future is dropped: its write may already be on the
    // wire, so no subsequent request may consume or reuse its correlation.
    in_flight: bool,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Wire<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            partial: Vec::new(),
            observations: VecDeque::new(),
            closed: false,
            in_flight: false,
        }
    }

    pub fn closed(&self) -> bool {
        self.closed || self.in_flight
    }

    pub async fn shutdown(&mut self) {
        self.closed = true;
        let _ = tokio::time::timeout(Duration::from_secs(2), self.writer.shutdown()).await;
    }

    pub fn invalidate(&mut self) {
        self.closed = true;
    }

    pub fn take_observations(&mut self) -> Vec<Observation> {
        self.observations.drain(..).collect()
    }

    async fn send<T: Serialize>(&mut self, document: &T) -> Result<()> {
        if self.closed {
            return Err(NativeFailure::Closed);
        }
        let mut bytes = serde_json::to_vec(document).map_err(|_| NativeFailure::Contract)?;
        if bytes.len() >= MAX_FRAME {
            return Err(NativeFailure::FrameLimit);
        }
        bytes.push(b'\n');
        self.writer
            .write_all(&bytes)
            .await
            .map_err(|_| NativeFailure::Unavailable)?;
        self.writer
            .flush()
            .await
            .map_err(|_| NativeFailure::Unavailable)
    }

    async fn frame(&mut self) -> Result<Frame> {
        if self.closed {
            return Err(NativeFailure::Closed);
        }
        loop {
            let available = self
                .reader
                .fill_buf()
                .await
                .map_err(|_| NativeFailure::Unavailable)?;
            if available.is_empty() {
                return Err(NativeFailure::Unavailable);
            }
            let newline = available.iter().position(|byte| *byte == b'\n');
            let length = newline.map_or(available.len(), |index| index + 1);
            if length > MAX_FRAME.saturating_sub(self.partial.len()) {
                return Err(NativeFailure::FrameLimit);
            }
            self.partial.extend_from_slice(&available[..length]);
            self.reader.consume(length);
            if newline.is_some() {
                let frame =
                    serde_json::from_slice(&self.partial).map_err(|_| NativeFailure::Contract);
                self.partial.clear();
                return frame;
            }
        }
    }

    async fn notification_or_request(&mut self, frame: Frame) -> Result<()> {
        let method = frame.method.ok_or(NativeFailure::Correlation)?;
        if method.len() > 200 || frame.result.is_some() || frame.error.is_some() {
            return Err(NativeFailure::Contract);
        }
        if let Some(id) = frame.id {
            // The native engine, not this adapter, runs tools. Dynamic-tool,
            // escalation, attestation and interactive requests are not silently
            // approved, proxied to arbitrary URLs or implemented as a second loop.
            let pending_rpc = self.in_flight;
            self.in_flight = true;
            self.send(&serde_json::json!({
                "id": id,
                "error": {"code": -32601, "message": "Client method is not supported"}
            }))
            .await?;
            self.in_flight = pending_rpc;
            return Ok(());
        }
        let Some(observation) = projection::notification(&method, frame.params.as_deref())? else {
            return Ok(());
        };
        if let Observation::Usage {
            thread_id, turn_id, ..
        } = &observation
        {
            if let Some(index) = self.observations.iter().position(|prior| {
                matches!(prior,
                Observation::Usage { thread_id: old_thread, turn_id: old_turn, .. }
                    if old_thread == thread_id && old_turn == turn_id)
            }) {
                self.observations.remove(index);
            }
        }
        if self.observations.len() >= 128 {
            return Err(NativeFailure::ObservationLimit);
        }
        self.observations.push_back(observation);
        Ok(())
    }

    pub async fn notify(&mut self, method: &'static str) -> Result<()> {
        if self.closed() {
            return Err(NativeFailure::Closed);
        }
        self.in_flight = true;
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            self.send(&serde_json::json!({"method":method})),
        )
        .await;
        self.in_flight = false;
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                self.closed = true;
                Err(error)
            }
            Err(_) => {
                self.closed = true;
                Err(NativeFailure::Unavailable)
            }
        }
    }

    pub async fn request(
        &mut self,
        id: RequestId,
        method: &'static str,
        params: Value,
        timeout: Duration,
    ) -> Result<Box<RawValue>> {
        if self.closed() {
            return Err(NativeFailure::Closed);
        }
        self.in_flight = true;
        let operation = async {
            self.send(&serde_json::json!({"id":id,"method":method,"params":params}))
                .await?;
            for _ in 0..4096 {
                let frame = self.frame().await?;
                if frame.method.is_some() {
                    self.notification_or_request(frame).await?;
                    continue;
                }
                if frame.id.as_ref() != Some(&id) || frame.params.is_some() {
                    return Err(NativeFailure::Correlation);
                }
                return match (frame.result, frame.error) {
                    (Some(result), None) => Ok(result),
                    (None, Some(error)) => Err(NativeFailure::Rejected(error.code)),
                    _ => Err(NativeFailure::Contract),
                };
            }
            Err(NativeFailure::ObservationLimit)
        };
        let result = match tokio::time::timeout(timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(NativeFailure::Unavailable),
        };
        self.in_flight = false;
        if result
            .as_ref()
            .is_err_and(|error| !matches!(error, NativeFailure::Rejected(_)))
        {
            self.closed = true;
        }
        result
    }

    pub async fn poll(&mut self, wait: Duration) -> Result<Vec<Observation>> {
        if self.closed() {
            return Err(NativeFailure::Closed);
        }
        if !self.observations.is_empty() {
            return Ok(self.take_observations());
        }
        let operation = async {
            for _ in 0..4096 {
                let frame = self.frame().await?;
                self.notification_or_request(frame).await?;
                if !self.observations.is_empty() {
                    return Ok(self.take_observations());
                }
            }
            Err(NativeFailure::ObservationLimit)
        };
        match tokio::time::timeout(wait, operation).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => {
                self.closed = true;
                Err(error)
            }
            // frame() retains all partial bytes. Polling is read-only and this
            // ordinary idle timeout is not an incomplete RPC or process failure.
            Err(_) => Ok(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncReadExt};

    #[tokio::test]
    async fn response_correlation_preserves_native_errors_without_their_sensitive_text() {
        let (input, mut native_output) = duplex(8192);
        let (output, mut native_input) = duplex(8192);
        let mut wire = Wire::new(input, output);
        let native = async {
            let mut reader = BufReader::new(&mut native_input);
            let mut request = String::new();
            reader.read_line(&mut request).await.unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            native_output.write_all(serde_json::to_vec(&serde_json::json!({
                "id":request["id"],"error":{"code":-32000,"message":"SECRET_NATIVE_ERROR_SENTINEL"}
            })).unwrap().as_slice()).await.unwrap();
            native_output.write_all(b"\n").await.unwrap();
        };
        let (result, ()) = tokio::join!(
            wire.request(
                RequestId::Text("native-1".into()),
                "account/read",
                serde_json::json!({}),
                Duration::from_secs(1)
            ),
            native
        );
        assert!(matches!(result, Err(NativeFailure::Rejected(-32000))));
        assert!(!wire.closed());
        assert!(!result.err().unwrap().to_string().contains("SENTINEL"));
    }

    #[tokio::test]
    async fn partial_idle_notification_is_retained_and_unknown_reasoning_payload_is_not_projected()
    {
        let (input, mut native_output) = duplex(8192);
        let (output, _native_input) = duplex(8192);
        let mut wire = Wire::new(input, output);
        native_output.write_all(b"{\"method\":\"item/reasoning/textDelta\",\"params\":{\"delta\":\"HIDDEN_SENTINEL\"}}\n{\"method\":\"account/login/completed\",\"params\":{").await.unwrap();
        assert!(wire
            .poll(Duration::from_millis(5))
            .await
            .unwrap()
            .is_empty());
        assert!(!wire.closed());
        native_output
            .write_all(b"\"loginId\":\"login-1\",\"success\":true,\"error\":null}}\n")
            .await
            .unwrap();
        let observations = wire.poll(Duration::from_secs(1)).await.unwrap();
        assert!(
            matches!(&observations[..], [Observation::LoginCompleted { login_id, success:true }] if login_id == "login-1")
        );
        assert!(!format!("{observations:?}").contains("HIDDEN_SENTINEL"));
    }

    #[tokio::test]
    async fn wrong_id_duplicate_binding_and_oversized_frames_invalidate_the_connection() {
        for document in [
            b"{\"id\":\"other\",\"result\":{}}\n".to_vec(),
            b"{\"id\":\"a\",\"id\":\"a\",\"result\":{}}\n".to_vec(),
            vec![b'x'; MAX_FRAME + 1],
        ] {
            let (input, mut native_output) = duplex(MAX_FRAME + 2);
            let (output, _native_input) = duplex(1024);
            let mut wire = Wire::new(input, output);
            native_output.write_all(&document).await.unwrap();
            let result = wire
                .request(
                    RequestId::Text("a".into()),
                    "account/read",
                    serde_json::json!({}),
                    Duration::from_secs(1),
                )
                .await;
            assert!(result.is_err());
            assert!(wire.closed());
        }
    }

    #[tokio::test]
    async fn dropping_an_inflight_rpc_permanently_prevents_a_second_send() {
        let (input, mut native_output) = duplex(8192);
        let (output, native_input) = duplex(8192);
        let mut wire = Wire::new(input, output);
        let mut reader = BufReader::new(native_input);
        {
            let request = wire.request(
                RequestId::Text("reserved-send".into()),
                "turn/start",
                serde_json::json!({}),
                Duration::from_secs(1),
            );
            tokio::pin!(request);
            let mut line = String::new();
            tokio::select! {
                result = &mut request => panic!("native request unexpectedly completed: {}", result.is_ok()),
                _ = reader.read_line(&mut line) => {}
            }
            assert!(!line.is_empty());
            // Drop the pending future after the native side actually received it.
        }
        assert!(wire.closed());
        native_output
            .write_all(b"{\"id\":\"reserved-send\",\"result\":{}}\n")
            .await
            .unwrap();
        assert!(matches!(
            wire.request(
                RequestId::Text("new-send".into()),
                "turn/start",
                serde_json::json!({}),
                Duration::from_secs(1)
            )
            .await,
            Err(NativeFailure::Closed)
        ));
        let mut line = String::new();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), reader.read_line(&mut line))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn unexpected_server_requests_are_rejected_without_running_a_tool() {
        let (input, mut native_output) = duplex(8192);
        let (output, mut native_input) = duplex(8192);
        let mut wire = Wire::new(input, output);
        native_output.write_all(b"{\"id\":7,\"method\":\"item/commandExecution/requestApproval\",\"params\":{\"command\":\"untrusted\"}}\n").await.unwrap();
        assert!(wire
            .poll(Duration::from_millis(5))
            .await
            .unwrap()
            .is_empty());
        let mut bytes = vec![0; 1024];
        let count = native_input.read(&mut bytes).await.unwrap();
        let response: Value = serde_json::from_slice(&bytes[..count]).unwrap();
        assert_eq!(response["id"], 7);
        assert_eq!(response["error"]["code"], -32601);
        assert!(response.get("result").is_none());
    }
}
