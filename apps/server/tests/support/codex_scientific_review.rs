//! A controlled Reviewer decision only after reading original native scientific inputs.
//! This is a Provider fixture, not an alternate review engine or scientific evaluator.
use super::tool_output::exec_part;
use contracts::{evidence::Decision, Id};
use serde_json::{json, Value};

pub struct OriginalScience {
    pub experiment: Id,
    pub alpha: Id,
    pub code: Id,
    pub parameters: Id,
    pub source: String,
    pub parameter_document: Value,
    pub context_fields: Value,
    pub metric_fields: Value,
    pub decision: Decision,
}

pub struct Review {
    original: OriginalScience,
    calls: usize,
    last_call: Option<String>,
    session: Option<u64>,
    output: String,
    observed: Option<Value>,
}

impl Review {
    pub fn new(original: OriginalScience) -> Self {
        assert!(matches!(
            original.decision,
            Decision::Pass | Decision::Reject
        ));
        Self {
            original,
            calls: 0,
            last_call: None,
            session: None,
            output: String::new(),
            observed: None,
        }
    }

    pub fn observed(&self) -> &Value {
        self.observed
            .as_ref()
            .expect("the independent native Thread must read original inputs")
    }

    pub fn next(&mut self, request: &Value, input: &str) -> Value {
        assert!(input.contains("QZ_MISSION_REVIEW_V1"));
        for forbidden in [
            "QZ_MISSION_INITIAL_V1",
            "QZ_MISSION_RESULT_V1",
            "QZ_NATIVE_FIRST_REPLY",
            "QZ_NATIVE_SCIENTIFIC_CONCLUSION",
            "qz2.",
        ] {
            assert!(!input.contains(forbidden));
        }
        assert!(self.calls < 8 && self.observed.is_none());
        let call = format!("native-original-review-{}", self.calls);
        self.calls += 1;
        let item = if let Some(previous) = &self.last_call {
            let result = request["input"]
                .as_array()
                .unwrap()
                .iter()
                .rev()
                .find(|item| {
                    item["type"] == "function_call_output" && item["call_id"] == previous.as_str()
                })
                .expect("the exact native read call must return");
            let text = result["output"].as_str().expect("native exec result");
            let (session, fragment) = exec_part(text);
            assert!(self.output.len() + fragment.len() <= 256 * 1024);
            self.output.push_str(fragment);
            if let Some(session) = session {
                if let Some(original) = self.session {
                    assert_eq!(session, original);
                }
                self.session = Some(session);
                json!({"type":"function_call","name":"write_stdin","call_id":call,
                    "arguments":json!({"session_id":session,"chars":"","yield_time_ms":1000,
                        "max_output_tokens":12000}).to_string()})
            } else {
                let (source, remainder) = self
                    .output
                    .split_once("\nQZ_ORIGINAL_PARAMETERS\n")
                    .expect("original code and parameters boundary");
                let (parameters, context) = remainder
                    .split_once("\nQZ_ORIGINAL_VALIDATION\n")
                    .expect("original parameters and validation boundary");
                assert_eq!(source, self.original.source);
                let parameters: Value = serde_json::from_str(parameters).unwrap();
                assert_eq!(parameters, self.original.parameter_document);
                let context: Value = serde_json::from_str(context).unwrap();
                assert_eq!(
                    context["experiment_id"],
                    self.original.experiment.to_string()
                );
                assert_eq!(context["alpha_version_id"], self.original.alpha.to_string());
                for (field, expected) in self.original.context_fields.as_object().unwrap() {
                    assert_eq!(
                        &context[field], expected,
                        "original review context field {field}"
                    );
                }
                let metrics: Vec<_> = context["metrics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|metric| {
                        metric["metric_code"] == "PEARSON_IC" && metric["scope"] == "asset:0/fold:0"
                    })
                    .collect();
                assert_eq!(metrics.len(), 1);
                for (field, expected) in self.original.metric_fields.as_object().unwrap() {
                    assert_eq!(
                        &metrics[0][field], expected,
                        "original native metric field {field}"
                    );
                }
                for excluded in [
                    "calibration",
                    "points",
                    "forecast",
                    "conversation",
                    "credentials",
                ] {
                    assert!(context.get(excluded).is_none());
                }
                self.observed = Some(context);
                let answer = json!({
                    "schema_version":1,
                    "alpha_version_id":self.original.alpha,
                    "decision":self.original.decision,
                    "reasons":[format!(
                        "Controlled independent decision after reading the original code, parameters and native Validation {}; this engineered fixture is not market evidence or qualification.",
                        self.original.context_fields["validation_evaluation_id"].as_str().unwrap()
                    )]
                });
                json!({"type":"message","role":"assistant","id":"native-independent-review",
                    "content":[{"type":"output_text","text":answer.to_string()}]})
            }
        } else {
            assert!(request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "exec_command"));
            // All interpolated names are canonical IDs obtained from the original
            // selection/records. The Worker owns materialization, not this fixture.
            let directory = format!("review-{}", self.original.experiment);
            let command = format!(
                "cat {directory}/{} && printf '\\nQZ_ORIGINAL_PARAMETERS\\n' && cat {directory}/{} && printf '\\nQZ_ORIGINAL_VALIDATION\\n' && cat {directory}/{}",
                self.original.code, self.original.parameters, self.original.alpha
            );
            json!({"type":"function_call","name":"exec_command","call_id":call,
                "arguments":json!({"cmd":command,"login":false,"yield_time_ms":1000,
                    "max_output_tokens":12000}).to_string()})
        };
        self.last_call = Some(call);
        item
    }
}
