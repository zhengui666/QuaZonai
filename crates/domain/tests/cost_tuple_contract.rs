use contracts::budget::{BudgetV1, StopRuleV1};
use serde_json::Value;

#[test]
fn shared_cost_tuples_preserve_native_admission_and_explicit_capability_failure() {
    let brief: Value =
        serde_json::from_str(include_str!("../../../tests/contracts/research-brief.json")).unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../../../tests/contracts/cost-tuples.json")).unwrap();
    let stop: StopRuleV1 = serde_json::from_value(brief["content"]["stop_rule"].clone()).unwrap();
    for case in cases {
        let mut budget = brief["content"]["budget"].as_object().unwrap().clone();
        for field in ["cost_enforcement", "max_cost_decimal", "cost_currency"] {
            budget.remove(field);
        }
        budget.extend(case["tuple"].as_object().unwrap().clone());
        let accepted = serde_json::from_value::<BudgetV1>(Value::Object(budget))
            .is_ok_and(|budget| domain::admission::validate_budget(&budget, &stop).is_ok());
        assert_eq!(accepted, case["admissible"].as_bool().unwrap(), "{case}");
    }
}
