//! Selection syntax only, never a substitute for original Store qualification.
use contracts::{portfolio::PortfolioBuildRequestV1, Id};
use serde_json::json;

#[test]
fn build_selection_keeps_exact_qualifications_weights_and_bounded_limits() {
    let value = json!({"schema_version":1,"cycle_id":Id::new(),"mandate_id":Id::new(),
        "input_set_id":Id::new(),"runtime_id":Id::new(),"expected_runtime_revision":"1",
        "current_weights_source":{"kind":"FORWARD_SNAPSHOT","snapshot_id":Id::new()},"environment":"PAPER",
        "members":[{"qualification_id":Id::new(),"ensemble_weight":"0.25"},
                   {"qualification_id":Id::new(),"ensemble_weight":"0.75"}],
        "limits":{"schema_version":1,"experiments":0,"cpu_seconds":"10","wall_seconds":10,"memory_mib":64,"output_bytes":"1024"}});
    let request: PortfolioBuildRequestV1 = serde_json::from_value(value.clone()).unwrap();
    domain::portfolio::build_selection(&request).unwrap();
    for case in 0..5 {
        let mut bad = request.clone();
        match case {
            0 => bad.members[1].qualification_id = bad.members[0].qualification_id,
            1 => bad.members[1].ensemble_weight = "0.74".parse().unwrap(),
            2 => {
                bad.members.pop();
            }
            3 => bad.limits.cpu_seconds = contracts::DbCounter::ZERO,
            _ => bad.members[0].ensemble_weight = "-0.25".parse().unwrap(),
        }
        assert!(domain::portfolio::build_selection(&bad).is_err());
    }
    let mut last = value.clone();
    last["current_weights_source"] = json!({"kind":"LAST_TARGET","candidate_id":Id::new()});
    domain::portfolio::build_selection(&serde_json::from_value(last.clone()).unwrap()).unwrap();
    last["current_weights_source"]["snapshot_id"] = json!(Id::new());
    assert!(serde_json::from_value::<PortfolioBuildRequestV1>(last).is_err());
    let mut invented = value;
    invented["current_cash_weight"] = "1".into();
    assert!(serde_json::from_value::<PortfolioBuildRequestV1>(invented).is_err());
}
