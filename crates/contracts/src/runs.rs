//! Operation categories carried by the immutable native file protocol.
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RunKind {
    DataValidate,
    AlphaEvaluate,
    PortfolioBuild,
    PortfolioSimulate,
    ForwardEvaluate,
}
