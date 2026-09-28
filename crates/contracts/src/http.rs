//! Shared HTTP error wire contract used by the server, native CLI and generated clients.
use crate::{Id, Revision};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    #[schema(minimum = 100, maximum = 599)]
    pub status: u16,
    pub code: String,
    pub detail: String,
    pub request_id: Id,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<Revision>,
    pub field_errors: Vec<FieldError>,
    pub safe_next_actions: Vec<String>,
}

/// Shared wire constraint for the idempotency header; authority remains server-side.
pub fn valid_idempotency_key(value: &str) -> bool {
    !value.is_empty()
        && value.is_ascii()
        && value.len() <= 200
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    #[test]
    fn idempotency_keys_preserve_visible_ascii_header_contract() {
        assert!(super::valid_idempotency_key("same intent 01"));
        assert!(super::valid_idempotency_key(&"a".repeat(200)));
        for invalid in [
            "",
            " leading",
            "trailing ",
            "line\nfeed",
            "tab\tvalue",
            "nonascii-意图",
            &"a".repeat(201),
        ] {
            assert!(!super::valid_idempotency_key(invalid));
        }
    }
}
