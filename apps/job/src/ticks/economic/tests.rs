use super::*;
use serde_json::json;

#[test]
fn optional_funding_pair_is_bound_to_source_window_before_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let request = crate::ticks::tests::request();
    assert!(funding_sources(dir.path(), &request)
        .unwrap()
        .is_empty());
    let quotes = dir.path().join("spot-valuation.jsonl");
    std::fs::write(&quotes, "{}\n").unwrap();
    assert_eq!(
        funding_sources(dir.path(), &request).unwrap(),
        vec![("spot-valuation.jsonl".into(), 3)]
    );
    std::fs::remove_file(&quotes).unwrap();
    let books = dir.path().join("book-snapshots.source.json");
    std::fs::write(&books, "{}\n").unwrap();
    assert_eq!(
        funding_sources(dir.path(), &request).unwrap(),
        vec![("book-snapshots.source.json".into(), 3)]
    );
    std::fs::remove_file(&books).unwrap();
    std::fs::write(dir.path().join("funding-rates.jsonl"), "").unwrap();
    assert!(funding_sources(dir.path(), &request).is_err());
    let mut manifest = json!({"source_revision":request.input.source_revision,"start_ns":"0","end_ns":request.input.event_end_ns,"instruments":[],"rows":0,"missing_instruments":[]});
    let path = dir.path().join("funding-rates.manifest.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert_eq!(
        funding_sources(dir.path(), &request)
            .unwrap()
            .len(),
        2
    );
    std::fs::write(&quotes, "{}\n").unwrap();
    assert!(funding_sources(dir.path(), &request)
        .unwrap()
        .contains(&("spot-valuation.jsonl".into(), 3)));
    std::fs::write(&books, "{}\n").unwrap();
    assert!(funding_sources(dir.path(), &request)
        .unwrap()
        .contains(&("book-snapshots.source.json".into(), 3)));
    #[cfg(unix)]
    {
        std::fs::remove_file(&quotes).unwrap();
        std::os::unix::fs::symlink(dir.path().join("funding-rates.jsonl"), &quotes).unwrap();
        assert!(funding_sources(dir.path(), &request).is_err());
        std::fs::remove_file(&quotes).unwrap();
    }
    let mut padded = serde_json::to_vec(&manifest).unwrap();
    padded.resize(padded.len() + 16 * 1024 * 1024, b' ');
    std::fs::write(&path, &padded).unwrap();
    let sources = funding_sources(dir.path(), &request).unwrap();
    assert_eq!(sources[1].1, padded.len() as u64);
    manifest["end_ns"] = json!("1");
    std::fs::write(path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    assert!(funding_sources(dir.path(), &request).is_err());
}

#[test]
fn context_prices_preserve_exact_decimals_and_missing_values() {
    assert_eq!(context_decimal("").unwrap(), None);
    assert_eq!(context_decimal("1e-8").unwrap(), Some(Decimal::new(1, 8)));
    assert_eq!(context_decimal("0.0000000200").unwrap(), Some(Decimal::new(2, 8)));
    for invalid in ["0", "-1", "NaN", "Infinity"] {
        assert!(context_decimal(invalid).is_err());
    }
}
