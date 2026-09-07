//! Operator-authored research contract. Saving a draft is not proof of native readiness.
use crate::{
    budget::{BudgetV1, StopRuleV1},
    research::DataPartition,
    DbCounter, Id, Revision, SchemaV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{PartialSchema, ToSchema};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TargetKind {
    Score,
    ExpectedReturn,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HorizonKind {
    FixedBars,
    FixedDuration,
    VariableInterval,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataAccess {
    MetadataOnly,
    ResearchRead,
    EvaluatorOnly,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BriefState {
    Draft,
    Frozen,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BriefContentV1 {
    pub hypothesis: String,
    pub economic_rationale: String,
    pub universe_version_id: Id,
    pub target_kind: TargetKind,
    pub horizon_kind: HorizonKind,
    pub horizon_value: Option<DbCounter>,
    pub base_currency: String,
    pub benchmark_ref: Option<Id>,
    pub evaluation_policy_id: Id,
    pub execution_assumptions_id: Id,
    pub budget: BudgetV1,
    pub stop_rule: StopRuleV1,
}

fn brief_content_variant(
    horizon_kind: HorizonKind,
    requires_horizon_value: bool,
) -> utoipa::openapi::schema::ObjectBuilder {
    use utoipa::openapi::schema::{AdditionalProperties, ObjectBuilder, Type};

    let horizon_kind = serde_json::to_value(horizon_kind)
        .expect("HorizonKind serialization is infallible for schema generation");
    let text = || {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .min_length(Some(1))
            .max_length(Some(8000))
    };
    let mut schema = ObjectBuilder::new()
        .schema_type(Type::Object)
        .additional_properties(Some(AdditionalProperties::FreeForm(false)))
        .property("hypothesis", text())
        .required("hypothesis")
        .property("economic_rationale", text())
        .required("economic_rationale")
        .property("universe_version_id", Id::schema())
        .required("universe_version_id")
        .property("target_kind", TargetKind::schema())
        .required("target_kind")
        .property(
            "horizon_kind",
            ObjectBuilder::new()
                .schema_type(Type::String)
                .enum_values(Some([horizon_kind])),
        )
        .required("horizon_kind")
        .property(
            "base_currency",
            ObjectBuilder::new()
                .schema_type(Type::String)
                .min_length(Some(3))
                .max_length(Some(3))
                .pattern(Some("^[A-Z]{3}$")),
        )
        .required("base_currency")
        .property("benchmark_ref", Option::<Id>::schema())
        .property("evaluation_policy_id", Id::schema())
        .required("evaluation_policy_id")
        .property("execution_assumptions_id", Id::schema())
        .required("execution_assumptions_id")
        .property("budget", BudgetV1::schema())
        .required("budget")
        .property("stop_rule", StopRuleV1::schema())
        .required("stop_rule");
    if requires_horizon_value {
        schema = schema
            .property(
                "horizon_value",
                crate::scalars::positive_db_counter_schema(),
            )
            .required("horizon_value");
    } else {
        schema = schema.property(
            "horizon_value",
            ObjectBuilder::new().schema_type(Type::Null),
        );
    }
    schema
}

impl utoipa::PartialSchema for BriefContentV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::OneOfBuilder;

        OneOfBuilder::new()
            .item(brief_content_variant(HorizonKind::FixedBars, true))
            .item(brief_content_variant(HorizonKind::FixedDuration, true))
            .item(brief_content_variant(HorizonKind::VariableInterval, false))
            .into()
    }
}

impl ToSchema for BriefContentV1 {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        // The manual union embeds these derived schemas, whose fields still
        // contain references. Collect them even when Brief is the only API root.
        schemas.push((BudgetV1::name().into_owned(), BudgetV1::schema()));
        BudgetV1::schemas(schemas);
        schemas.push((StopRuleV1::name().into_owned(), StopRuleV1::schema()));
        StopRuleV1::schemas(schemas);
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefBindingV1 {
    pub dataset_revision_id: Id,
    pub role: DataPartition,
    pub access_policy: DataAccess,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefCreate {
    pub schema_version: SchemaV1,
    pub content: BriefContentV1,
    #[schema(min_items = 1, max_items = 64)]
    pub bindings: Vec<BriefBindingV1>,
    pub supersedes_id: Option<Id>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefCreateIntent {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub request: BriefCreate,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefUpdate {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    pub content: BriefContentV1,
    #[schema(min_items = 1, max_items = 64)]
    pub bindings: Vec<BriefBindingV1>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BriefView {
    pub id: Id,
    pub project_id: Id,
    #[schema(minimum = 1, maximum = 2147483647)]
    pub version: u32,
    pub revision: Revision,
    pub state: BriefState,
    pub content: BriefContentV1,
    #[schema(min_items = 1, max_items = 64)]
    pub bindings: Vec<BriefBindingV1>,
    pub supersedes_id: Option<Id>,
    pub frozen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
