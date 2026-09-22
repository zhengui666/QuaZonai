//! Local wire-level request preview. Never opens credentials, files or a connection.
use super::{commands, watch, Failure, Result};
use contracts::Id;
use reqwest::{header, Method};
use serde_json::{json, Value};

pub(super) fn inspect(
    request: &commands::Request,
    origin: &str,
    development_http: bool,
    key: Option<&str>,
    grant: Option<&str>,
) -> Result<Value> {
    crate::WebPolicy::new(origin, ([127, 0, 0, 1], 0).into(), development_http)
        .map_err(|_| Failure::Configuration)?;
    let writes = request.method != Method::GET;
    if writes {
        let key = key.ok_or(Failure::IdempotencyRequired)?;
        let mut headers = header::HeaderMap::new();
        headers.insert(
            "idempotency-key",
            header::HeaderValue::from_str(key).map_err(|_| Failure::Input)?,
        );
        // Same native key validator used by the real sender, not a preview rule.
        crate::access::idempotency_key(&headers).map_err(|_| Failure::Input)?;
    }
    if let Some(grant) = grant {
        let _: Id = grant.to_owned().try_into().map_err(|_| Failure::Input)?;
    }
    let output = match &request.output {
        commands::Output::Json(_) => "json",
        commands::Output::Binary { .. } => "bytes",
        commands::Output::Events { run, after, .. } => {
            watch::cursor(*run, after.as_deref())?;
            "ndjson"
        }
    };
    // Deliberately omit every body and all header/connection values. There is no
    // field-name redaction list that could miss a future credential-bearing DTO.
    Ok(json!({
        "schema_version": 1,
        "preview": true,
        "request_sent": false,
        "authorization_checked": false,
        "server_state_checked": false,
        "method": request.method.as_str(),
        "route": request.route,
        "query": request.query,
        "expected_http_status": request.status,
        "output": output,
        "body_bytes": request.body.as_ref().map_or(0, Vec::len),
        "body_redacted": true,
        "requires_idempotency_key": writes,
        "requires_operator_grant": request.operator,
        "operator_grant_supplied": grant.is_some()
    }))
}
