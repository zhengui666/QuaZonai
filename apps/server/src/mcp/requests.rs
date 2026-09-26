//! Tool-specific envelopes; inner domain schemas come from the same native DTOs.
use super::Failure;
use contracts::{artifacts::ResearchArtifactKind, experiments::ExperimentProposalV1, Id, SchemaV1};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;
use serde_json::{json, Map, Value};

fn native<T: utoipa::PartialSchema>() -> schemars::Schema {
    let value = serde_json::to_value(T::schema()).expect("native contract schema serializes");
    schemars::Schema::try_from(value).expect("native contract schema is an object")
}
fn kind(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    native::<ResearchArtifactKind>()
}
fn proposal(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut schema = native::<ExperimentProposalV1>();
    let object = schema.as_object_mut().unwrap();
    for field in ["schema_version", "cycle_id"] {
        object["properties"].as_object_mut().unwrap().remove(field);
        object["required"]
            .as_array_mut()
            .unwrap()
            .retain(|required| required != field);
    }
    schema
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundReadRequest {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFileRequest {
    #[schemars(schema_with = "kind")]
    pub kind: ResearchArtifactKind,
    #[schemars(length(min = 1, max = 512))]
    pub workspace_relative_path: String,
    #[schemars(length(min = 1, max = 200))]
    pub idempotency_key: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposalRequest {
    #[schemars(length(min = 1, max = 200))]
    pub idempotency_key: String,
    #[schemars(schema_with = "proposal")]
    pub proposal: Map<String, Value>,
}

impl ProposalRequest {
    pub fn into_native(mut self, cycle_id: Id) -> Result<(String, ExperimentProposalV1), Failure> {
        // The model chooses research inputs; only the launcher supplies binding
        // and version. Reject even matching overrides instead of ignoring them.
        for (field, value) in [
            ("schema_version", json!(SchemaV1)),
            ("cycle_id", json!(cycle_id)),
        ] {
            if self.proposal.insert(field.to_owned(), value).is_some() {
                return Err(Failure::Contract);
            }
        }
        let proposal =
            serde_json::from_value(Value::Object(self.proposal)).map_err(|_| Failure::Contract)?;
        Ok((self.idempotency_key, proposal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn references(value: &Value) -> bool {
        match value {
            Value::Object(map) => map.contains_key("$ref") || map.values().any(references),
            Value::Array(values) => values.iter().any(references),
            _ => false,
        }
    }

    #[test]
    fn proposal_schema_is_native_contract_without_launcher_fields() {
        let value = serde_json::to_value(schemars::schema_for!(ProposalRequest)).unwrap();
        assert_eq!(value["additionalProperties"], false);
        let mut actual = value["properties"]["proposal"].clone();
        // All model-controlled fields retain their native, inline schema.
        actual.as_object_mut().unwrap().remove("description");
        let mut expected =
            serde_json::to_value(<ExperimentProposalV1 as utoipa::PartialSchema>::schema())
                .unwrap();
        for field in ["schema_version", "cycle_id"] {
            expected["properties"]
                .as_object_mut()
                .unwrap()
                .remove(field);
        }
        expected["required"] = json!([
            "family_id",
            "hypothesis",
            "expected_failure_modes",
            "proposal_artifact_id",
            "parameter_artifact_id"
        ]);
        assert_eq!(actual, expected);
        assert!(!references(&value));
        assert!(actual["properties"].get("outcome").is_none());
        assert!(actual["properties"].get("author_run_id").is_none());
    }

    #[test]
    fn bound_reads_accept_only_an_empty_object() {
        let schema = serde_json::to_value(schemars::schema_for!(BoundReadRequest)).unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["type"], "object");
        assert!(serde_json::from_value::<BoundReadRequest>(json!({})).is_ok());
        for field in ["brief_id", "run_id", "project_id", "unknown"] {
            assert!(serde_json::from_value::<BoundReadRequest>(json!({field:Id::new()})).is_err());
        }
    }

    #[test]
    fn proposal_binding_preserves_native_choices_and_rejects_injection_and_invalid_ids() {
        let value = json!({"idempotency_key":"one","proposal":{
            "family_id":Id::new(),"parent_experiment_id":Id::new(),
            "hypothesis":"premise","expected_failure_modes":"failure",
            "proposal_artifact_id":Id::new(),"parameter_artifact_id":Id::new(),
            "code_artifact_id":Id::new()
        }});
        let cycle = Id::new();
        let (key, proposal) = serde_json::from_value::<ProposalRequest>(value.clone())
            .unwrap()
            .into_native(cycle)
            .unwrap();
        assert_eq!(key, "one");
        let mut expected = value["proposal"].clone();
        expected["cycle_id"] = json!(cycle);
        expected["schema_version"] = json!(1);
        assert_eq!(serde_json::to_value(proposal).unwrap(), expected);
        for (field, injected) in [
            ("cycle_id", json!(cycle)),
            ("cycle_id", json!(Id::new())),
            ("schema_version", json!(1)),
            ("schema_version", Value::Null),
            ("project_id", json!(Id::new())),
            ("author_run_id", json!(Id::new())),
            ("outcome", json!("PASS")),
            ("unknown", json!(true)),
            ("family_id", json!("../../secret")),
            ("family_id", json!("550e8400-e29b-41d4-a716-446655440000")),
            ("proposal_artifact_id", Value::Null),
        ] {
            let mut invalid = value.clone();
            invalid["proposal"][field] = injected;
            assert!(matches!(
                serde_json::from_value::<ProposalRequest>(invalid)
                    .unwrap()
                    .into_native(cycle),
                Err(Failure::Contract)
            ));
        }
        let mut missing = value;
        missing["proposal"]
            .as_object_mut()
            .unwrap()
            .remove("family_id");
        assert!(serde_json::from_value::<ProposalRequest>(missing)
            .unwrap()
            .into_native(cycle)
            .is_err());
    }

    #[test]
    fn file_submission_accepts_only_native_kind_and_relative_name_intent() {
        let value =
            json!({"kind":"CODE","workspace_relative_path":"alpha.rs","idempotency_key":"one"});
        assert!(serde_json::from_value::<ArtifactFileRequest>(value.clone()).is_ok());
        let mut version_override = value.clone();
        version_override["schema_version"] = json!(1);
        assert!(serde_json::from_value::<ArtifactFileRequest>(version_override).is_err());
        for field in [
            "project_id",
            "run_id",
            "workspace_root",
            "token",
            "origin",
            "content",
        ] {
            let mut invalid = value.clone();
            invalid[field] = json!("injected");
            assert!(serde_json::from_value::<ArtifactFileRequest>(invalid).is_err());
        }
        for field in ["kind", "workspace_relative_path", "idempotency_key"] {
            let mut invalid = value.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ArtifactFileRequest>(invalid).is_err());
        }
        let mut invalid = value;
        invalid["kind"] = json!("PACKAGE");
        assert!(serde_json::from_value::<ArtifactFileRequest>(invalid).is_err());
    }
}
