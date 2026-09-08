//! Native HTTP transport regressions; the existing runtime validator is unchanged.
use crate::access::idempotency_key;
use axum::http::{HeaderMap, HeaderValue};

fn accepts(bytes: &[u8]) -> bool {
    let Ok(value) = HeaderValue::from_bytes(bytes) else {
        return false;
    };
    let mut headers = HeaderMap::new();
    headers.insert("idempotency-key", value);
    idempotency_key(&headers).is_ok()
}

#[test]
fn idempotency_header_has_exact_printable_ascii_and_length_bounds() {
    for byte in 0..=u8::MAX {
        assert_eq!(accepts(&[byte]), (b'!'..=b'~').contains(&byte));
    }
    assert!(accepts(b"a b"));
    assert!(accepts(&[b'x'; 200]));
    assert!(!accepts(&[b'x'; 201]));
    for bytes in [
        b"".as_slice(),
        b" a",
        b"a ",
        b"a\tB",
        b"a\n",
        b"a\r",
        b"a\x7f",
        "中文".as_bytes(),
    ] {
        assert!(!accepts(bytes));
    }
}

#[test]
fn idempotency_header_rejects_missing_and_duplicate_values() {
    let mut headers = HeaderMap::new();
    assert!(idempotency_key(&headers).is_err());
    headers.append("idempotency-key", HeaderValue::from_static("one"));
    headers.append("idempotency-key", HeaderValue::from_static("two"));
    assert!(idempotency_key(&headers).is_err());
}
