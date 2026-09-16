//! Real PostgreSQL/file transactions with controlled catalog/probe observations, not REAL qualification.
#[path = "../../crates/store/tests/support/data.rs"]
pub mod data;
#[path = "execution_models.rs"]
mod models;
#[path = "runtime_observation.rs"]
mod observation;
use contracts::{
    execution_assumptions::ExecutionAssumptionsCreateV1,
    research::{DataPartition, InputItemV1, InputPurpose, InputSetCreate},
    runtime::{RuntimeProbeOutcomeV1, RuntimeVenueV1},
    science::{NativeAccountKind, NativeFeeRateV1, NativeSimulationSettingsV1},
    SchemaV1,
};
use sqlx::PgPool;

pub async fn prepare(
    pool: &PgPool,
    f: data::Fixture,
) -> (data::Fixture, ExecutionAssumptionsCreateV1) {
    let mut metadata = data::catalog_fixture::metadata();
    metadata.universe.instrument_definitions[0]["CurrencyPair"]["quote_currency"] = "USD".into();
    metadata.universe.instrument_definitions[0]["CurrencyPair"]["maker_fee"] = "0.001".into();
    metadata.universe.instrument_definitions[0]["CurrencyPair"]["taker_fee"] = "0.002".into();
    let ticket = data::ticket(&f, "dataset", &data::request(&f)).await;
    let dataset = data::complete(&f, ticket, serde_json::to_vec(&metadata).unwrap())
        .await
        .unwrap()
        .resource;
    let input = f
        .store
        .create_input_set(
            &f.actor,
            "input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.project,
                purpose: InputPurpose::Discovery,
                decision_cutoff: metadata.available_through,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset.id,
                    role: DataPartition::Discovery,
                }],
            },
        )
        .await
        .unwrap()
        .resource;
    sqlx::query("UPDATE app.runtime_integrations SET allowed_capabilities=ARRAY['PORTFOLIO_SIMULATE','DATA_VALIDATE'] WHERE id=$1")
        .bind(f.runtime.id.as_uuid()).execute(pool).await.unwrap();
    let mut cap = observation::configured_capabilities(pool, f.runtime.id).await;
    cap.engine_versions
        .insert("simulation-models".into(), "1".into());
    cap.engine_versions
        .insert("nautilus".into(), "0.63.0".into());
    cap.engine_versions
        .insert("bar-notional".into(), "1".into());
    cap.engine_versions
        .insert("portfolio-rolling-liquidity".into(), "1".into());
    cap.artifact_schemas
        .push(contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.data_quality".into(),
            version: "1".into(),
        });
    cap.venues = vec![RuntimeVenueV1 {
        venue: "SIM".into(),
        instrument_classes: vec!["CurrencyPair".into()],
        data_kinds: vec![contracts::runtime::RuntimeDataKind::Bar],
        expiry_and_settlement: false,
    }];
    let revision = observation::publish(
        pool,
        f.runtime.id,
        RuntimeProbeOutcomeV1::Available {
            capabilities: Box::new(cap),
        },
        chrono::Duration::seconds(60),
    )
    .await;
    let request = ExecutionAssumptionsCreateV1 {
        schema_version: SchemaV1,
        project_id: f.project,
        runtime_id: f.runtime.id,
        expected_runtime_revision: revision,
        input_set_id: input.header.id,
        dataset_revision_id: dataset.id,
        settlement_rule_ref: "controlled-spot-settlement".into(),
        bar_liquidity: None,
        rolling_liquidity: None,
        settings: NativeSimulationSettingsV1 {
            schema_version: SchemaV1,
            base_currency: "USD".into(),
            starting_capital: "1000".parse().unwrap(),
            account_kind: NativeAccountKind::Margin,
            leverage: "1".parse().unwrap(),
            fill_model: models::fill(),
            fee_model: models::fee(),
            latency_model: models::latency(1),
            snapshot_interval_ms: 1000,
            exposure_tolerance: "0.000001".parse().unwrap(),
            fee_rates: vec![NativeFeeRateV1 {
                instrument_id: "EUR/USD.SIM".into(),
                maker: "0.001".parse().unwrap(),
                taker: "0.002".parse().unwrap(),
            }],
        },
    };
    (f, request)
}
