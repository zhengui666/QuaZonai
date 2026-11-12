//! Native catalog identity discovery remains available after retiring factor scans.
#[path = "support/factors_catalog.rs"]
mod factors_catalog;
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
use factors_catalog::mixed_catalog;
use job::catalog::catalog_factor_scope;

#[test]
fn private_catalog_scope_matches_original_metadata_and_cli() {
    let (root, request) = mixed_catalog(0.0, false);
    let scope = catalog_factor_scope(root.path()).unwrap();
    let mut expected = request.forecast.selection.bar_types.clone();
    expected.sort();
    let mut actual = scope.clone();
    actual.sort();
    assert_eq!(actual, expected);
    let discovered = native::command(
        &[
            "catalog-factor-scope".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
        ],
        &serde_json::json!({}),
    );
    assert!(discovered.status.success());
    assert_eq!(
        serde_json::from_slice::<Vec<String>>(&discovered.stdout).unwrap(),
        scope
    );
}
