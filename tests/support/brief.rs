//! Relational authoring fixture only; immutable metadata is not real native evidence.
#![allow(dead_code)]
use super::research_support::ResearchFixture;
use contracts::{
    brief::*,
    research::{DataPartition, InputPurpose},
    Id, SchemaV1,
};
use store::{authority::Actor, Store};
pub fn example() -> BriefCreate {
    serde_json::from_str(include_str!("../contracts/research-brief.json")).unwrap()
}
pub async fn request(store: &Store, actor: &Actor, data: &ResearchFixture) -> BriefCreateIntent {
    let input = store
        .create_input_set(
            actor,
            &Id::new().to_string(),
            &data.input(InputPurpose::Validation),
        )
        .await
        .unwrap()
        .resource;
    let policy = store
        .create_evaluation_policy(actor, &Id::new().to_string(), &data.policy(input.header.id))
        .await
        .unwrap()
        .resource;
    let mut request = example();
    request.content.universe_version_id = data.universe;
    request.content.evaluation_policy_id = policy.id;
    request.content.execution_assumptions_id = data.assumptions;
    request.bindings = vec![
        BriefBindingV1 {
            dataset_revision_id: data.discovery,
            role: DataPartition::Discovery,
            access_policy: DataAccess::ResearchRead,
        },
        BriefBindingV1 {
            dataset_revision_id: data.sealed,
            role: DataPartition::Sealed,
            access_policy: DataAccess::EvaluatorOnly,
        },
    ];
    BriefCreateIntent {
        schema_version: SchemaV1,
        project_id: data.project,
        request,
    }
}
