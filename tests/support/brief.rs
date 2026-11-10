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
    serde_json::from_str(r###"{"schema_version":1,"content":{"hypothesis":"A bounded, preregistered relative-value hypothesis.","economic_rationale":"Test whether the stated economic exposure persists out of sample.","universe_version_id":"0198c407-0200-7000-8000-000000000001","target_kind":"SCORE","horizon_kind":"FIXED_BARS","horizon_value":"5","base_currency":"USD","benchmark_ref":null,"evaluation_policy_id":"0198c407-0200-7000-8000-000000000002","execution_assumptions_id":"0198c407-0200-7000-8000-000000000003","budget":{"schema_version":1,"max_experiments":20,"max_parallel_runs":2,"max_turns_per_mission":5,"max_repair_turns":1,"max_wall_seconds":3600,"max_cpu_seconds":"600","max_memory_mib":1024,"max_output_bytes":"1048576","max_cycles_per_day":2,"min_cycle_interval_seconds":60,"max_tokens":"10000","max_cost_decimal":null,"cost_currency":null,"cost_enforcement":"UNAVAILABLE"},"stop_rule":{"schema_version":1,"stop_on_qualified_count":2,"stop_on_budget":true,"stop_on_no_improvement_trials":5,"stop_on_invalid_data":true}},"bindings":[{"dataset_revision_id":"0198c407-0200-7000-8000-000000000004","role":"DISCOVERY","access_policy":"RESEARCH_READ"},{"dataset_revision_id":"0198c407-0200-7000-8000-000000000005","role":"SEALED","access_policy":"EVALUATOR_ONLY"}],"supersedes_id":null}"###).unwrap()
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
