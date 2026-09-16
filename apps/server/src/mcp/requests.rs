//! Tool-specific envelopes; inner domain schemas come from the same native DTOs.
use contracts::{artifacts::ResearchArtifactKind, experiments::ExperimentProposalV1, SchemaV1};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;

fn native<T: utoipa::PartialSchema>() -> schemars::Schema {
    let value = serde_json::to_value(T::schema()).expect("native contract schema serializes");
    schemars::Schema::try_from(value).expect("native contract schema is an object")
}
fn version(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    native::<SchemaV1>()
}
fn kind(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    native::<ResearchArtifactKind>()
}
fn proposal(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    native::<ExperimentProposalV1>()
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFileRequest {
    #[schemars(schema_with = "version")]
    pub schema_version: SchemaV1,
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
    pub proposal: ExperimentProposalV1,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn references(value: &Value) -> bool {
        match value {
            Value::Object(map) => map.contains_key("$ref") || map.values().any(references),
            Value::Array(values) => values.iter().any(references),
            _ => false,
        }
    }

    #[test]
    fn proposal_schema_is_the_same_inline_native_contract_not_a_parallel_dto() {
        let value = serde_json::to_value(schemars::schema_for!(ProposalRequest)).unwrap();
        assert_eq!(value["additionalProperties"], false);
        let mut actual = value["properties"]["proposal"].clone();
        // Schemars may describe the containing property, but the actual structure
        // and native ID/version bounds must be identical and self-contained.
        actual.as_object_mut().unwrap().remove("description");
        assert_eq!(
            actual,
            serde_json::to_value(<ExperimentProposalV1 as utoipa::PartialSchema>::schema())
                .unwrap()
        );
        assert!(!references(&value));
        assert!(actual["properties"].get("outcome").is_none());
        assert!(actual["properties"].get("author_run_id").is_none());
    }

    #[test]
    fn file_submission_accepts_only_native_version_kind_and_relative_name_intent() {
        let value = json!({"schema_version":1,"kind":"CODE","workspace_relative_path":"alpha.rs","idempotency_key":"one"});
        assert!(serde_json::from_value::<ArtifactFileRequest>(value.clone()).is_ok());
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
        for field in [
            "schema_version",
            "kind",
            "workspace_relative_path",
            "idempotency_key",
        ] {
            let mut invalid = value.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<ArtifactFileRequest>(invalid).is_err());
        }
        let mut invalid = value;
        invalid["kind"] = json!("PACKAGE");
        assert!(serde_json::from_value::<ArtifactFileRequest>(invalid).is_err());
    }
}
