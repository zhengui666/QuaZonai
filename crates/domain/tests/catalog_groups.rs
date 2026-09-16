//! Temporal classification fixtures, not historical market or PIT evidence.
#[path = "../../../tests/support/catalog_metadata.rs"]
mod support;
use contracts::portfolio::GroupBoundV1;
use domain::catalogs::{metadata, portfolio_groups};

#[test]
fn original_group_membership_requires_known_unique_causal_classification() {
    let original = support::metadata();
    metadata(&original, support::instant(600)).unwrap();
    let bound = vec![GroupBoundV1 {
        group_id: "fixture-group".into(),
        min: "0".parse().unwrap(),
        max: "1".parse().unwrap(),
    }];
    let instrument = original.universe.membership[0].instrument_id.clone();
    let at = support::count(100_000_000_000);
    let resolve = |universe: &contracts::catalogs::NativeUniverseV1| {
        portfolio_groups(universe, std::slice::from_ref(&instrument), &bound, at)
    };
    assert_eq!(
        resolve(&original.universe).unwrap(),
        vec![vec!["fixture-group"]]
    );
    for case in 0..8 {
        let mut universe = original.universe.clone();
        match case {
            0 => universe.membership[0].groups = None,
            1 => universe.membership[0].groups = Some(vec![]),
            2 => universe.membership[0].available_at = support::instant(101),
            3 => universe.membership[0].valid_from = support::instant(101),
            4 => universe.membership[0].valid_until = Some(support::instant(100)),
            5 => {
                let mut overlapping = universe.membership[0].clone();
                overlapping.valid_from = support::instant(1);
                universe.membership.push(overlapping);
            }
            6 => universe.selection_asof = support::instant(101),
            _ => universe.coverage_end = support::instant(99),
        }
        assert!(resolve(&universe).is_err(), "case {case}");
        assert_eq!(
            portfolio_groups(&universe, std::slice::from_ref(&instrument), &[], at).unwrap(),
            vec![Vec::<String>::new()]
        );
    }
    let mut universe = original.universe.clone();
    universe.membership[0].valid_from = support::instant(100);
    assert!(resolve(&universe).is_ok());
    let mut second = universe.membership[0].clone();
    second.instrument_id = "SECOND.SIM".into();
    second.groups = Some(vec![]);
    universe.membership.push(second);
    let ids = vec![instrument, "SECOND.SIM".into()];
    assert_eq!(
        portfolio_groups(&universe, &ids, &bound, at).unwrap()[1],
        Vec::<String>::new()
    );
    universe.membership[1].groups = None;
    assert!(portfolio_groups(&universe, &ids, &bound, at).is_err());
    for groups in [
        vec!["duplicate".into(); 2],
        vec!["".into()],
        vec!["x".repeat(121)],
        vec!["group".into(); 65],
    ] {
        let mut value = original.clone();
        value.universe.membership[0].groups = Some(groups);
        assert!(metadata(&value, support::instant(600)).is_err());
    }
}
