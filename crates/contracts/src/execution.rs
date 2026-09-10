//! Fixed native job operations. No caller-selected process, environment, mount or host path.
use crate::{
    portfolio::AllocationInputV1,
    runtime::RuntimeArtifactSchemaV1,
    runtime_jobs::RuntimeOutputV1,
    science::{NativeBarSelectionV1, NativeForecastRequestV1, NativeSimulationRequestV1},
    DbCounter, Id, SchemaV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeDatasetSelectionV1 {
    pub dataset_revision_id: Id,
    pub selection: NativeBarSelectionV1,
}

/// Compilation is a DATA_VALIDATE preparation run with no dataset mounts.
/// Its successful immutable MODEL becomes an input of a separate scientific run.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "operation",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum NativeTaskParametersV1 {
    CompileModel {
        schema_version: SchemaV1,
        code_artifact_id: Id,
    },
    ValidateData {
        schema_version: SchemaV1,
        #[schema(min_items = 1, max_items = 256)]
        selections: Vec<NativeDatasetSelectionV1>,
    },
    EvaluateAlpha {
        schema_version: SchemaV1,
        dataset_revision_id: Id,
        model_artifact_id: Id,
        request: NativeForecastRequestV1,
    },
    BuildPortfolio {
        schema_version: SchemaV1,
        request: Box<AllocationInputV1>,
    },
    SimulatePortfolio {
        schema_version: SchemaV1,
        dataset_revision_id: Id,
        request: NativeSimulationRequestV1,
    },
}

impl NativeTaskParametersV1 {
    pub fn job_kind(&self) -> crate::runs::RunKind {
        use crate::runs::RunKind;
        match self {
            Self::CompileModel { .. } | Self::ValidateData { .. } => RunKind::DataValidate,
            Self::EvaluateAlpha { .. } => RunKind::AlphaEvaluate,
            Self::BuildPortfolio { .. } => RunKind::PortfolioBuild,
            Self::SimulatePortfolio { .. } => RunKind::PortfolioSimulate,
        }
    }
    pub fn output_schemas(&self) -> Vec<RuntimeArtifactSchemaV1> {
        let names: &[&str] = match self {
            Self::CompileModel { .. } => &["qz.wasm_model", "qz.model_compilation"],
            Self::ValidateData { .. } => &["qz.data_quality"],
            Self::EvaluateAlpha { .. } => &["qz.native_forecast"],
            Self::BuildPortfolio { .. } => &["qz.native_allocation"],
            Self::SimulatePortfolio { .. } => &["qz.native_simulation"],
        };
        names
            .iter()
            .map(|name| RuntimeArtifactSchemaV1 {
                name: (*name).to_owned(),
                version: "1".into(),
            })
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeJobOutputIndexV1 {
    pub schema_version: SchemaV1,
    #[schema(min_items = 1, max_items = 64)]
    pub artifacts: Vec<RuntimeOutputV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeDatasetQualityV1 {
    pub dataset_revision_id: Id,
    pub selection: NativeBarSelectionV1,
    pub row_count: DbCounter,
    #[schema(min_items = 1, max_items = 256)]
    pub instrument_ids: Vec<String>,
    pub first_event_ns: DbCounter,
    pub last_event_ns: DbCounter,
    pub available_through_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeDataQualityReportV1 {
    pub schema_version: SchemaV1,
    pub native_version: String,
    pub checked_at: DateTime<Utc>,
    #[schema(min_items = 1, max_items = 256)]
    pub datasets: Vec<NativeDatasetQualityV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeModelCompilationV1 {
    pub schema_version: SchemaV1,
    pub code_artifact_id: Id,
    pub model_storage_ref: Id,
    pub rustc_version: String,
    pub target: String,
    pub abi: String,
    pub module_bytes: DbCounter,
}

/// An artifact download retains the original native JSON document, not an extra
/// envelope. Its exact variant is bound by the immutable result manifest.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum NativeJsonOutputV1 {
    ModelCompilation(Box<NativeModelCompilationV1>),
    DataQuality(Box<NativeDataQualityReportV1>),
    Forecast(Box<crate::science::NativeForecastResultV1>),
    Allocation(Box<crate::portfolio::AllocationResultV1>),
    Simulation(Box<crate::science::NativeSimulationResultV1>),
}
