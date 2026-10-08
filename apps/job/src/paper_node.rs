//! A single foreground native Paper node. Public market data is live; execution is
//! always the official Sandbox. This module never registers historical datasets.
use anyhow::{anyhow, ensure, Result};
use bigdecimal::ToPrimitive;
use contracts::{
    account_observation::{NativeAccountBindingV1, NATIVE_ACCOUNT_VERSION},
    delivery::{ForecastTargetPackageV2, HandoffStateV1, PackageOriginV1},
    forward::ForwardEnvironmentV1,
    portfolio::{AllocationTargetV1, NativeModelRefV1},
    science::{NativeAccountKind, NativeTargetPointV1},
    strategy_portfolio::{HandoffClaimViewV2, TargetPackageEnvelopeV2},
    DbCounter, Id, SchemaV1,
};
use nautilus_binance::{
    common::enums::{BinanceEnvironment, BinanceProductType},
    config::{BinanceDataClientConfig, BinanceInstrumentProviderConfig, BinanceSpotMarketDataMode},
    factories::BinanceDataClientFactory,
};
use nautilus_common::{
    enums::Environment,
    factories::{ClientConfig, DataClientFactory},
    logging::logger::LoggerConfig,
};
use nautilus_execution::models::{
    fee::{FeeModelAny, MakerTakerFeeModel},
    fill::{DefaultFillModel, FillModelAny},
};
use nautilus_live::node::{config::LiveDataEngineConfig, LiveNode, NodeRunMode};
use nautilus_model::{
    data::BarType,
    enums::{AccountType, OmsType},
    identifiers::{AccountId, ClientId, InstrumentId, StrategyId, TraderId, Venue},
    types::{Currency, Money},
};
use nautilus_portfolio::config::PortfolioConfig;
use nautilus_sandbox::{SandboxExecutionClientConfig, SandboxExecutionClientFactory};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    net::SocketAddr,
    path::{Path, PathBuf},
    rc::Rc,
    str::FromStr,
    time::Duration,
};
use tokio::sync::watch;

use crate::{
    native_node_observer::NativeNodeObserver,
    paper_service::{PaperState, PaperStatus},
    simulation::{paper_target_strategy, ReplayStatus},
};

pub const EXECUTION_CLIENT: &str = "QZ-PAPER";
pub const DATA_CLIENT: &str = "BINANCE-PUBLIC";

/// Deliberately one venue/product. No execution credentials or arbitrary adapter
/// class names can be supplied through this configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PaperDataSource {
    BinanceSpotPublic { bar_interval_seconds: u32 },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaperLatencyModel {
    /// Native Sandbox has no StaticLatencyModel configuration seam.
    NativeWallClock,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperConfig {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub downstream_id: Id,
    pub trader_id: String,
    pub account_id: String,
    pub market_capability_version: String,
    /// Required only to cross-check original Native TargetDecision packages.
    /// Forecast settings are always read from the immutable V2 claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_assumptions: Option<contracts::execution_assumptions::ExecutionAssumptionsViewV1>,
    pub source: PaperDataSource,
    pub paper_latency: PaperLatencyModel,
    /// A new segment for this invocation. Existing files are never overwritten.
    pub observations_file: PathBuf,
    /// Stable private runtime volume; must be reused across invocations and outputs.
    pub claim_state_directory: PathBuf,
    /// Existing local downstream authentication; this command creates no token.
    pub credential_file: PathBuf,
    /// Existing trusted reverse proxy exposes the configured downstream endpoint.
    pub bind: SocketAddr,
}

#[derive(Clone, Debug, Serialize)]
pub struct PaperPreflight {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub downstream_id: Id,
    pub handoff_id: Id,
    pub release_id: Id,
    pub execution_environment: &'static str,
    pub native_account_model: &'static str,
    pub restart_policy: &'static str,
    pub claim_replay_scope: &'static str,
    pub market_data_source: &'static str,
    pub market_time_basis: &'static str,
    pub native_version: &'static str,
    pub instrument_ids: Vec<String>,
    pub bar_types: Vec<String>,
    pub fee_basis: &'static str,
    pub latency_basis: &'static str,
    pub connects_market_data: bool,
    pub execution_supported: bool,
    pub execution_blocker: Option<&'static str>,
}

impl PaperConfig {
    fn validate_runtime(&self) -> Result<()> {
        ensure!(self.bind.ip().is_loopback(), "PAPER_BIND_LOOPBACK_REQUIRED");
        ensure!(
            self.claim_state_directory.is_absolute(),
            "PAPER_STABLE_CLAIM_STATE_REQUIRED"
        );
        ensure!(
            !self.market_capability_version.is_empty(),
            "PAPER_MARKET_CAPABILITY"
        );
        TraderId::new_checked(&self.trader_id)?;
        ensure!(
            AccountId::new_checked(&self.account_id)?.get_issuer() == Venue::from("BINANCE"),
            "PAPER_ACCOUNT_VENUE"
        );
        ensure!(
            !self.observations_file.as_os_str().is_empty()
                && self.observations_file != self.credential_file,
            "PAPER_OBSERVATION_PATH"
        );
        let PaperDataSource::BinanceSpotPublic {
            bar_interval_seconds,
        } = self.source;
        ensure!(
            (1..=60).contains(&bar_interval_seconds),
            "PAPER_BAR_INTERVAL"
        );
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_runtime()?;
        let assumptions = self
            .execution_assumptions
            .as_ref()
            .ok_or_else(|| anyhow!("PAPER_NATIVE_ASSUMPTIONS_REQUIRED"))?;
        ensure!(
            assumptions.project_id == self.project_id,
            "PAPER_ASSUMPTIONS_PROJECT"
        );
        domain::portfolio::simulation_settings(&assumptions.settings)?;
        ensure!(
            assumptions.settings.account_kind == NativeAccountKind::Margin
                && assumptions.settings.leverage.as_decimal() == &bigdecimal::BigDecimal::from(1),
            "PAPER_MARGIN_LEVERAGE_ONE_REQUIRED"
        );
        ensure!(
            matches!(
                assumptions.settings.fee_model,
                NativeModelRefV1::NautilusMakerTaker { .. }
            ),
            "PAPER_SPOT_MAKER_TAKER_REQUIRED"
        );
        let PaperDataSource::BinanceSpotPublic {
            bar_interval_seconds,
        } = self.source;
        ensure!(
            (1..=60).contains(&bar_interval_seconds),
            "PAPER_BAR_INTERVAL"
        );
        // Public instrument discovery has no account-specific fee source. The
        // fixed adapter supplies its published 0.001 maker/taker model instead.
        let fallback: contracts::DecimalValue = "0.001".parse().map_err(anyhow::Error::msg)?;
        ensure!(
            assumptions
                .settings
                .fee_rates
                .iter()
                .all(|rate| rate.maker == fallback && rate.taker == fallback),
            "PAPER_PUBLIC_FEE_MODEL_MISMATCH"
        );
        Ok(())
    }
}

fn nanos(value: chrono::DateTime<chrono::Utc>) -> Result<DbCounter> {
    DbCounter::new(u64::try_from(
        value
            .timestamp_nanos_opt()
            .ok_or_else(|| anyhow!("PAPER_TIME_RANGE"))?,
    )?)
    .map_err(anyhow::Error::msg)
}

/// An explicit model initial condition is the only Forecast source which may
/// request new Paper funding. Zero weights alone never select this branch.
/// This validates original facts; a server consume receipt is still required
/// before any native account is constructed.
pub(crate) fn forecast_initial_account(
    package: &ForecastTargetPackageV2,
) -> Result<Option<&contracts::strategy_portfolio::FreshPaperCashV1>> {
    use contracts::{portfolio::CandidateWeightsSourceV1, science::PortfolioWeightsSourceV1};
    let weights = &package.current_weights;
    let Some(root) = weights.paper_initialization.as_ref() else {
        ensure!(
            package.environment_origin == PackageOriginV1::Real
                && !matches!(
                    weights.source,
                    PortfolioWeightsSourceV1::PaperInitialCapital { .. }
                )
                && package.current_weights_source != CandidateWeightsSourceV1::PaperInitialCapital,
            "PAPER_INITIAL_CAPITAL_ORIGIN_OR_ROOT"
        );
        return Ok(None);
    };
    ensure!(
        package.environment_origin == PackageOriginV1::Synthetic
            && package.source.build_environment == ForwardEnvironmentV1::Paper,
        "PAPER_INITIAL_CAPITAL_REQUIRES_SYNTHETIC_PAPER"
    );
    domain::portfolio::paper_account_scope(&root.trader_id, &root.account_id)?;
    TraderId::new_checked(&root.trader_id)?;
    AccountId::new_checked(&root.account_id)?;
    let PortfolioWeightsSourceV1::PaperInitialCapital { account_start } = &weights.source else {
        if let PortfolioWeightsSourceV1::ForwardSnapshot { downstream_id, .. } = &weights.source {
            ensure!(
                *downstream_id == root.downstream_id,
                "PAPER_INITIAL_CAPITAL_SCOPE"
            );
        }
        return Ok(None);
    };
    ensure!(
        package.current_weights_source == CandidateWeightsSourceV1::PaperInitialCapital
            && root.artifact_id == package.source.current_weights_artifact_id
            && root.downstream_id == account_start.downstream_id
            && root.trader_id == account_start.trader_id
            && root.account_id == account_start.account_id
            && account_start.execution_assumptions_id == package.cost_assumption_ref
            && account_start.base_currency == package.base_currency
            && account_start.base_currency == package.execution_settings.base_currency
            && account_start.base_currency == weights.base_currency
            && account_start.starting_capital == package.capital_assumption
            && account_start.starting_capital == package.execution_settings.starting_capital
            && account_start.starting_capital.is_positive()
            && weights.cash_weight.as_decimal() == &bigdecimal::BigDecimal::from(1)
            && !weights.weights.is_empty()
            && weights
                .weights
                .iter()
                .all(|weight| weight.weight.as_decimal() == &bigdecimal::BigDecimal::from(0))
            && weights.asof_ns == package.forward_dataset.selection.decision_cutoff_ns
            && weights.available_ns == package.forward_dataset.selection.decision_cutoff_ns,
        "PAPER_INITIAL_CAPITAL_ORIGINAL_BINDING"
    );
    Ok(Some(account_start))
}

pub(crate) fn forecast_initialization_blocker(package: &ForecastTargetPackageV2) -> &'static str {
    if package.current_weights.paper_initialization.is_some() {
        "PAPER_FORECAST_CONTINUATION_UNSUPPORTED"
    } else {
        "PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED"
    }
}

/// Validate only the immutable Forecast claim. No owner-authored assumptions,
/// metadata path, research read scope, or fresh-cash account substitutes for it.
/// This is a source preflight, not a claim that a research weight snapshot can
/// initialize a new native account.
pub(crate) fn forecast_instruments(
    package: &ForecastTargetPackageV2,
    venue: Venue,
) -> Result<Vec<nautilus_model::instruments::InstrumentAny>> {
    use contracts::{
        catalogs::DataRevisionPolicy,
        portfolio::CandidateWeightsSourceV1,
        research::{DataOrigin, DataPartition, PitStatus},
        science::PortfolioWeightsSourceV1,
    };
    let frozen = &package.forward_dataset;
    let weights = &package.current_weights;
    let settings = &package.execution_settings;
    let asof = nanos(package.asof)?.get();
    let from = nanos(package.valid_from)?.get();
    let until = nanos(package.valid_until)?.get();
    forecast_initial_account(package)?;
    ensure!(
        frozen.origin == DataOrigin::Real
            && frozen.partition == DataPartition::Forward
            && frozen.data_kind == contracts::runtime::RuntimeDataKind::Bar
            && frozen.pit_status == PitStatus::Verified
            && frozen.revision_policy == DataRevisionPolicy::AsKnownThen,
        "PAPER_FORECAST_REQUIRES_REAL_PIT_FORWARD_SOURCE"
    );
    ensure!(
        package.source.forward_dataset_revision_id == frozen.dataset_revision_id
            && package.source.forward_metadata_artifact_id == frozen.native_metadata_artifact_id
            && package
                .input_revision_refs
                .contains(&frozen.dataset_revision_id)
            && [
                package.source.build_parameters_artifact_id,
                package.source.build_report_artifact_id,
                package.source.current_weights_artifact_id,
                package.source.forward_metadata_artifact_id
            ]
            .iter()
            .all(|id| package.provenance_artifact_refs.contains(id))
            && package.qualification_refs.len() >= 2
            && !package.evaluation_refs.is_empty(),
        "PAPER_FORECAST_ORIGINAL_SOURCE_BINDING"
    );
    ensure!(
        package.asof <= package.valid_from
            && package.valid_from < package.valid_until
            && package.valid_until > chrono::Utc::now()
            && weights.asof_ns.get() <= asof
            && weights.asof_ns <= weights.available_ns
            && weights.available_ns.get() <= asof
            && until <= weights.valid_until_ns.get()
            && package.base_currency == weights.base_currency
            && package.base_currency == settings.base_currency
            && package.capital_assumption == settings.starting_capital
            && package.exposure_tolerance == settings.exposure_tolerance,
        "PAPER_FORECAST_ORIGINAL_WEIGHTS_AND_SETTINGS"
    );
    ensure!(
        matches!(
            (package.current_weights_source, &weights.source),
            (
                CandidateWeightsSourceV1::ForwardSnapshot,
                PortfolioWeightsSourceV1::ForwardSnapshot { .. }
            ) | (
                CandidateWeightsSourceV1::LastTarget,
                PortfolioWeightsSourceV1::LastTarget { .. }
            ) | (
                CandidateWeightsSourceV1::PaperInitialCapital,
                PortfolioWeightsSourceV1::PaperInitialCapital { .. }
            )
        ),
        "PAPER_FORECAST_ORIGINAL_WEIGHTS_SOURCE"
    );
    domain::portfolio::simulation_settings(settings)?;
    ensure!(
        frozen.event_start < frozen.event_end
            && frozen.event_start <= frozen.available_through
            && frozen.row_count.get() > 0
            && frozen.selection.event_start_ns < frozen.selection.event_end_ns
            && frozen.selection.event_end_ns <= frozen.selection.decision_cutoff_ns
            && frozen.selection.decision_cutoff_ns.get() <= asof
            && !frozen.selection.bar_types.is_empty(),
        "PAPER_FORECAST_ORIGINAL_DATASET_CLOCK"
    );
    let definitions = domain::catalogs::instrument_versions(&frozen.instrument_definitions)?;
    let mut required = BTreeSet::new();
    let mut current = BTreeSet::new();
    let mut total = weights.cash_weight.as_decimal().clone();
    for weight in &weights.weights {
        ensure!(
            weight.currency == package.base_currency
                && current.insert(weight.instrument_id.as_str()),
            "PAPER_FORECAST_CURRENT_WEIGHTS"
        );
        required.insert(weight.instrument_id.as_str());
        total += weight.weight.as_decimal();
    }
    ensure!(
        !current.is_empty()
            && (total - bigdecimal::BigDecimal::from(1)).abs()
                <= *package.exposure_tolerance.as_decimal(),
        "PAPER_FORECAST_CURRENT_WEIGHTS"
    );
    let mut targets = BTreeSet::new();
    let mut total = package.cash_weight.as_decimal().clone();
    for target in &package.targets {
        ensure!(
            target.currency == package.base_currency
                && targets.insert(target.instrument_id.as_str()),
            "PAPER_FORECAST_TARGETS"
        );
        required.insert(target.instrument_id.as_str());
        total += target.target_weight.as_decimal();
    }
    ensure!(
        !targets.is_empty()
            && (total - bigdecimal::BigDecimal::from(1)).abs()
                <= *package.exposure_tolerance.as_decimal(),
        "PAPER_FORECAST_TARGETS"
    );
    // Include every frozen asset, not just positive final targets. Existing
    // holdings and zero targets must not disappear from the preflight.
    for bar in &frozen.selection.bar_types {
        let bar = BarType::from_str(bar)?;
        ensure!(
            bar.instrument_id().venue == venue,
            "PAPER_FORECAST_SOURCE_VENUE"
        );
        required.insert(
            definitions
                .get_key_value(bar.instrument_id().to_string().as_str())
                .ok_or_else(|| anyhow!("PAPER_FORECAST_FROZEN_DEFINITION_MISSING"))?
                .0,
        );
    }
    ensure!(
        required.iter().all(|id| definitions.contains_key(id)),
        "PAPER_FORECAST_FROZEN_DEFINITION_MISSING"
    );
    // Store freezes the full original asset closure. Preserve every definition,
    // including an original asset with zero current and zero target weight.
    required.extend(definitions.keys().copied());
    let mut instruments = Vec::new();
    for id in required {
        let parsed: InstrumentId = id.parse()?;
        ensure!(parsed.venue == venue, "PAPER_FORECAST_SOURCE_VENUE");
        let (class, original) = domain::catalogs::instrument_version_at(
            definitions
                .get(id)
                .ok_or_else(|| anyhow!("PAPER_FORECAST_FROZEN_DEFINITION_MISSING"))?,
            from,
        )?;
        ensure!(
            venue != Venue::from("POLYMARKET") || class == "BinaryOption",
            "PAPER_HOST_BINARY_DEFINITION_REQUIRED"
        );
        instruments.push(serde_json::from_value(
            serde_json::json!({(class): original}),
        )?);
    }
    Ok(instruments)
}

/// This checks the original accepted claim's local binding. Authority still
/// belongs to Q's existing authenticated claim transaction, never to this file.
pub fn preflight_envelope(
    config: &PaperConfig,
    claim: &HandoffClaimViewV2,
) -> Result<PaperPreflight> {
    config.validate_runtime()?;
    let handoff = &claim.handoff;
    let package = domain::delivery::package_delivery(&claim.package);
    match &claim.package {
        TargetPackageEnvelopeV2::Forecast(p) => {
            forecast_instruments(p, Venue::from("BINANCE"))?;
            let fallback: contracts::DecimalValue = "0.001".parse().map_err(anyhow::Error::msg)?;
            ensure!(
                matches!(
                    p.execution_settings.fee_model,
                    NativeModelRefV1::NautilusMakerTaker { .. }
                ) && p
                    .execution_settings
                    .fee_rates
                    .iter()
                    .all(|rate| rate.maker == fallback && rate.taker == fallback),
                "PAPER_PUBLIC_FEE_MODEL_MISMATCH"
            );
            ensure!(
                p.execution_settings.account_kind == NativeAccountKind::Margin
                    && p.execution_settings.leverage.as_decimal()
                        == &bigdecimal::BigDecimal::from(1),
                "PAPER_MARGIN_LEVERAGE_ONE_REQUIRED"
            );
        }
        TargetPackageEnvelopeV2::TargetDecision(p) => {
            config.validate()?;
            let assumptions = config
                .execution_assumptions
                .as_ref()
                .ok_or_else(|| anyhow!("PAPER_NATIVE_ASSUMPTIONS_REQUIRED"))?;
            ensure!(
                p.execution_environment == ForwardEnvironmentV1::Paper
                    && p.account_start.downstream_id == config.downstream_id
                    && p.account_start.trader_id == config.trader_id
                    && p.account_start.account_id == config.account_id
                    && p.account_start.base_currency == assumptions.settings.base_currency
                    && p.account_start.starting_capital == assumptions.settings.starting_capital
                    && p.account_start.execution_assumptions_id == assumptions.id
                    && p.cost_assumption_ref == assumptions.id
                    && serde_json::to_value(&p.execution_settings)?
                        == serde_json::to_value(&assumptions.settings)?,
                "PAPER_FROZEN_ACCOUNT_BINDING"
            );
        }
    }
    let settings = match &claim.package {
        TargetPackageEnvelopeV2::Forecast(p) => &p.execution_settings,
        TargetPackageEnvelopeV2::TargetDecision(p) => &p.execution_settings,
    };
    let now = chrono::Utc::now();
    ensure!(
        handoff.environment == ForwardEnvironmentV1::Paper
            && handoff.state == HandoffStateV1::Claimed
            && handoff.project_id == config.project_id
            && handoff.downstream_id == config.downstream_id
            && handoff.release_id == package.release_id
            && handoff.project_id == package.project_id
            && handoff.candidate_id == package.candidate_id
            && handoff.mandate_id == package.mandate_id
            && handoff
                .external_claim_id
                .as_ref()
                .is_some_and(|id| !id.is_empty()
                    && id.trim() == id
                    && !id.chars().any(char::is_control))
            && handoff
                .claimed_at
                .is_some_and(|at| at >= handoff.offered_at && at <= now && at < handoff.expires_at)
            && handoff.acknowledged_at.is_none(),
        "PAPER_CLAIM_BINDING"
    );
    ensure!(
        package.asof <= package.valid_from
            && package.valid_from < package.valid_until
            && package.valid_until > now
            && package
                .compatible_market_capabilities
                .contains(&config.market_capability_version)
            && *package.base_currency == settings.base_currency
            && *package.capital_assumption == settings.starting_capital
            && *package.exposure_tolerance == settings.exposure_tolerance,
        "PAPER_PACKAGE_BINDING"
    );
    ensure!(!package.targets.is_empty(), "PAPER_TARGET_COUNT");
    let mut unique = BTreeSet::new();
    let mut total = package.cash_weight.as_decimal().clone();
    let mut instrument_ids = Vec::new();
    let mut bar_types = Vec::new();
    let PaperDataSource::BinanceSpotPublic {
        bar_interval_seconds,
    } = config.source;
    for target in package.targets {
        let instrument: InstrumentId = target.instrument_id.parse()?;
        ensure!(
            instrument.venue == Venue::from("BINANCE")
                && !instrument.symbol.as_str().contains('-')
                && target.currency == *package.base_currency
                && target.target_weight.is_nonnegative()
                && unique.insert(target.instrument_id.clone()),
            "PAPER_SPOT_TARGET_UNSUPPORTED"
        );
        total += target.target_weight.as_decimal();
        instrument_ids.push(target.instrument_id.clone());
        let kind = format!(
            "{}-{}-SECOND-MID-INTERNAL",
            instrument, bar_interval_seconds
        );
        BarType::from_str(&kind)?;
        bar_types.push(kind);
    }
    ensure!(
        package.cash_weight.is_nonnegative()
            && (total - bigdecimal::BigDecimal::from(1)).abs()
                <= *package.exposure_tolerance.as_decimal()
            && settings.fee_rates.len() == instrument_ids.len()
            && settings
                .fee_rates
                .iter()
                .all(|rate| unique.contains(&rate.instrument_id)),
        "PAPER_TARGET_CAPITAL"
    );
    Ok(PaperPreflight {
        schema_version: SchemaV1,
        project_id: config.project_id,
        downstream_id: config.downstream_id,
        handoff_id: handoff.id,
        release_id: package.release_id,
        execution_environment: "PAPER_SANDBOX",
        native_account_model: "MARGIN_LEVERAGE_ONE",
        restart_policy: "FRESH_ACCOUNT_AND_SESSION_NO_RESTORE",
        claim_replay_scope: "DURABLE_RUNTIME_PROJECT_ADAPTER_CLAIM",
        market_data_source: "BINANCE_SPOT_PUBLIC_JSON",
        market_time_basis:
            "NATIVE_ADAPTER_TIMESTAMPS_WITH_RECEIVE_TIME_FALLBACK_WHEN_EXCHANGE_TIME_ABSENT",
        native_version: NATIVE_ACCOUNT_VERSION,
        instrument_ids,
        bar_types,
        fee_basis:
            "native public instrument default 0.001; simulated fees, not account commission rates",
        latency_basis:
            "native wall clock; Sandbox does not apply the historical StaticLatencyModel",
        connects_market_data: false,
        execution_supported: matches!(claim.package, TargetPackageEnvelopeV2::TargetDecision(_)),
        execution_blocker: matches!(claim.package, TargetPackageEnvelopeV2::Forecast(_))
            .then_some("PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED"),
    })
}

pub fn data_config(preflight: &PaperPreflight) -> BinanceDataClientConfig {
    BinanceDataClientConfig {
        product_type: BinanceProductType::Spot,
        environment: BinanceEnvironment::Live,
        spot_market_data_mode: BinanceSpotMarketDataMode::Json,
        api_key: None,
        api_secret: None,
        instrument_provider: BinanceInstrumentProviderConfig {
            load_all: false,
            load_ids: Some(preflight.instrument_ids.clone()),
            query_commission_rates: false,
            ..Default::default()
        },
        ..Default::default()
    }
}

pub fn data_engine_config() -> LiveDataEngineConfig {
    LiveDataEngineConfig {
        time_bars_build_with_no_updates: false,
        time_bars_skip_first_non_full_bar: true,
        time_bars_timestamp_on_close: true,
        validate_data_sequence: true,
        ..Default::default()
    }
}

pub struct PaperNode {
    node: LiveNode,
    observer: Option<NativeNodeObserver>,
    replay: Rc<RefCell<ReplayStatus>>,
    valid_until: chrono::DateTime<chrono::Utc>,
}

impl PaperNode {
    /// Constructing this object does not connect the data client. Only run does.
    pub fn build_envelope(config: &PaperConfig, claim: &HandoffClaimViewV2) -> Result<Self> {
        let checked = preflight_envelope(config, claim)?;
        Self::build_from_factory(
            config,
            claim,
            Box::new(BinanceDataClientFactory::new()),
            Box::new(data_config(&checked)),
        )
    }

    /// Engineering acceptance only: substitutes current synthetic data at the
    /// official data-factory boundary. Sandbox and target execution stay unchanged.
    #[cfg(feature = "native-paper-test")]
    pub fn build_with_data_factory_envelope(
        config: &PaperConfig,
        claim: &HandoffClaimViewV2,
        factory: Box<dyn DataClientFactory>,
        source: Box<dyn ClientConfig>,
    ) -> Result<Self> {
        Self::build_from_factory(config, claim, factory, source)
    }

    fn build_from_factory(
        config: &PaperConfig,
        claim: &HandoffClaimViewV2,
        factory: Box<dyn DataClientFactory>,
        source: Box<dyn ClientConfig>,
    ) -> Result<Self> {
        let checked = preflight_envelope(config, claim)?;
        ensure!(
            matches!(claim.package, TargetPackageEnvelopeV2::TargetDecision(_)),
            "PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED"
        );
        let package = domain::delivery::package_delivery(&claim.package);
        let TargetPackageEnvelopeV2::TargetDecision(original) = &claim.package else {
            unreachable!("Forecast initialization was rejected before native construction")
        };
        let settings = &original.execution_settings;
        let (fill, _) = domain::portfolio::simulation_models(settings)?;
        let currency = Currency::from_str(&settings.base_currency)?;
        let mut node = LiveNode::builder(
            TraderId::new_checked(&config.trader_id)?,
            Environment::Sandbox,
        )?
        .with_logging(LoggerConfig {
            bypass_logging: true,
            ..Default::default()
        })
        .with_load_state(false)
        .with_save_state(false)
        .with_reconciliation(false)
        .with_delay_post_stop_secs(0)
        .with_delay_shutdown_secs(0)
        .with_data_engine_config(data_engine_config())
        .with_portfolio_config(PortfolioConfig {
            snapshot_interval_ms: Some(u64::from(settings.snapshot_interval_ms)),
            ..Default::default()
        })
        .add_data_client(Some(DATA_CLIENT.into()), factory, source)?
        .add_simulated_exec_client(
            Some(EXECUTION_CLIENT.into()),
            Box::new(SandboxExecutionClientFactory::new()),
            Box::new(SandboxExecutionClientConfig {
                account_id: AccountId::new_checked(&config.account_id)?,
                venue: Venue::from("BINANCE"),
                oms_type: OmsType::Netting,
                account_type: AccountType::Margin,
                base_currency: Some(currency),
                starting_balances: vec![Money::from_str(&format!(
                    "{} {}",
                    settings.starting_capital.as_decimal().to_plain_string(),
                    settings.base_currency
                ))
                .map_err(anyhow::Error::msg)?],
                fee_model: Some(FeeModelAny::MakerTaker(MakerTakerFeeModel)),
                fill_model: Some(FillModelAny::Default(DefaultFillModel::new(
                    fill.prob_fill_on_limit
                        .as_decimal()
                        .to_f64()
                        .ok_or_else(|| anyhow!("PAPER_FILL_RANGE"))?,
                    fill.prob_slippage
                        .as_decimal()
                        .to_f64()
                        .ok_or_else(|| anyhow!("PAPER_FILL_RANGE"))?,
                    Some(fill.random_seed.get()),
                )?)),
                // Quotes drive native matching. Internal bars only schedule targets.
                bar_execution: false,
                trade_execution: false,
                liquidity_consumption: true,
                ..Default::default()
            }),
        )?
        .build()?;
        let point = NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: nanos(package.valid_from)?,
            valid_until_ns: nanos(package.valid_until)?,
            targets: package
                .targets
                .iter()
                .map(|target| AllocationTargetV1 {
                    instrument_id: target.instrument_id.clone(),
                    weight: target.target_weight.clone(),
                    currency: target.currency.clone(),
                })
                .collect(),
            cash_weight: package.cash_weight.clone(),
        };
        let (mut strategy, replay) = paper_target_strategy(
            settings.clone(),
            Vec::new(),
            checked
                .bar_types
                .iter()
                .map(|kind| BarType::from_str(kind))
                .collect::<Result<Vec<_>, _>>()?,
            point,
            StrategyId::from("QZ-TARGET-001"),
            ClientId::from(EXECUTION_CLIENT),
        )?;
        if let TargetPackageEnvelopeV2::TargetDecision(p) = &claim.package {
            strategy.require_fresh_paper_account(p.account_start.clone())?;
            strategy.require_strategy_constraints(
                p.constraints_summary.clone(),
                p.base_currency.clone(),
                p.exposure_tolerance.clone(),
            );
        }
        node.add_strategy(strategy)?;
        let binding = NativeAccountBindingV1 {
            schema_version: SchemaV1,
            project_id: config.project_id,
            environment: ForwardEnvironmentV1::Paper,
            native_trader_id: node.trader_id().to_string(),
            native_session_id: node.instance_id().to_string(),
            native_account_id: config.account_id.clone(),
            native_version: NATIVE_ACCOUNT_VERSION.into(),
        };
        let observer = NativeNodeObserver::attach(
            &node,
            binding,
            ClientId::from(EXECUTION_CLIENT),
            &config.observations_file,
            1024,
        )?;
        Ok(Self {
            node,
            observer: Some(observer),
            replay,
            valid_until: package.valid_until,
        })
    }

    pub fn session_id(&self) -> String {
        self.node.instance_id().to_string()
    }

    pub async fn run(
        mut self,
        mut stop: watch::Receiver<bool>,
        status: watch::Sender<PaperStatus>,
    ) -> Result<()> {
        let observer = self
            .observer
            .take()
            .ok_or_else(|| anyhow!("PAPER_OBSERVER_MISSING"))?;
        let handle = self.node.handle();
        let replay = self.replay.clone();
        let data_engine = self.node.kernel().data_engine.clone();
        let valid_until = self.valid_until;
        let mut native_started = false;
        let mut requested_stop = false;
        let mut failed = None;
        let (native_done, mut done) = watch::channel(false);
        let monitor = async {
            let mut timer = tokio::time::interval(Duration::from_millis(100));
            let mut heartbeat_at = tokio::time::Instant::now();
            let signal = stop_signal();
            tokio::pin!(signal);
            loop {
                tokio::select! {
                    _ = timer.tick() => {},
                    _ = done.changed() => { break; },
                    _ = &mut signal, if !requested_stop => { requested_stop = true; handle.stop(); },
                    result = stop.changed(), if !requested_stop => {
                        if result.is_err() || *stop.borrow() { requested_stop = true; handle.stop(); }
                    }
                }
                if *stop.borrow() && !requested_stop {
                    requested_stop = true;
                    handle.stop();
                }
                if handle.is_running() && !native_started && replay.borrow().failure.is_none() {
                    native_started = true;
                    publish_state(&status, PaperState::Running, None);
                }
                if heartbeat_at.elapsed() >= Duration::from_secs(10) {
                    if observer.heartbeat().is_err() {
                        failed = Some("PAPER_OBSERVATION_FAILED");
                        handle.stop();
                    }
                    heartbeat_at = tokio::time::Instant::now();
                }
                if replay.borrow().failure.is_some() {
                    failed = Some("PAPER_TARGET_FAILED");
                    handle.stop();
                }
                let consumed = replay.borrow().consumed;
                if status.borrow().target_points_consumed != consumed {
                    status.send_modify(|current| current.target_points_consumed = consumed);
                }
                if handle.is_running() && !requested_stop && !data_engine.borrow().check_connected()
                {
                    failed = Some("PAPER_DATA_DISCONNECTED");
                    handle.stop();
                }
                if chrono::Utc::now() >= valid_until {
                    requested_stop = true;
                    handle.stop();
                }
                if handle.state() == nautilus_live::node::NodeState::Stopped {
                    break;
                }
            }
        };
        observer.heartbeat()?;
        let native = async {
            let result = self.node.run_with_mode(NodeRunMode::Hosted).await;
            native_done.send_replace(true);
            result
        };
        let (native_result, ()) = tokio::join!(native, monitor);
        let retained = observer.finish();
        if replay.borrow().failure.is_some() {
            failed = Some("PAPER_TARGET_FAILED");
        }
        let consumed = replay.borrow().consumed;
        status.send_modify(|current| current.target_points_consumed = consumed);
        if native_result.is_err()
            || retained.is_err()
            || failed.is_some()
            || (!native_started && !requested_stop)
        {
            publish_state(
                &status,
                PaperState::Failed,
                Some(failed.unwrap_or("PAPER_NATIVE_FAILED")),
            );
            return Err(anyhow!("PAPER_NATIVE_FAILED"));
        }
        publish_state(
            &status,
            PaperState::Stopped,
            if consumed == 0 {
                Some("PAPER_TARGET_NOT_APPLIED")
            } else if requested_stop {
                None
            } else {
                Some("PAPER_NATIVE_STOPPED")
            },
        );
        Ok(())
    }
}

pub fn publish_state(status: &watch::Sender<PaperStatus>, state: PaperState, reason: Option<&str>) {
    status.send_modify(|current| {
        current.state = state;
        current.reason_code = reason.map(str::to_owned);
        current.updated_at = chrono::Utc::now();
    });
}

/// Local configuration files only. Credentials never appear in an output DTO.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let file = std::fs::File::open(path)?;
    ensure!(file.metadata()?.is_file(), "PAPER_INPUT_FILE");
    Ok(serde_json::from_reader(file)?)
}

pub fn read_credential(path: &Path) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    ensure!(metadata.is_file(), "PAPER_CREDENTIAL_FILE");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o077 == 0,
            "PAPER_CREDENTIAL_PERMISSIONS"
        );
    }
    let mut bytes = std::fs::read(path)?;
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    ensure!(
        !bytes.is_empty() && bytes.iter().all(|b| b.is_ascii_graphic()),
        "PAPER_CREDENTIAL_FORMAT"
    );
    Ok(bytes)
}

#[derive(clap::Args)]
pub struct Arguments {
    #[command(subcommand)]
    command: Operation,
}

#[derive(clap::Subcommand)]
enum Operation {
    /// Check local claim/source bindings. Opens no network connection or node.
    Preflight {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        claim: PathBuf,
    },
    /// Serve one fresh Paper account/session with durable cross-process lifecycle replay.
    /// No position restore. New claims start public market data with declared capital.
    /// Configuration requires the same persistent claim_state_directory on every restart.
    Serve {
        #[arg(long)]
        config: PathBuf,
    },
    /// Apply an original Q claim to an already running local foreground Paper host.
    Apply {
        #[arg(long)]
        origin: String,
        #[arg(long)]
        credential_file: PathBuf,
        #[arg(long)]
        claim: PathBuf,
    },
    /// Read native lifecycle status and the Paper model/source limitations.
    Status {
        #[arg(long)]
        origin: String,
        #[arg(long)]
        credential_file: PathBuf,
    },
    /// Request native shutdown. A stopping request is not confirmation of shutdown.
    Stop {
        #[arg(long)]
        origin: String,
        #[arg(long)]
        credential_file: PathBuf,
    },
}

fn output(value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    writeln!(stdout)?;
    stdout.flush()?;
    Ok(())
}

fn read_claim(path: &Path) -> Result<HandoffClaimViewV2> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Claim {
        Receipt(contracts::control::CommandResult<HandoffClaimViewV2>),
        Body(HandoffClaimViewV2),
    }
    Ok(match read_json(path)? {
        Claim::Receipt(receipt) => receipt.resource,
        Claim::Body(claim) => claim,
    })
}

pub(crate) async fn stop_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = signal.recv() => {},
            },
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn serve(config: PaperConfig) -> Result<()> {
    use crate::paper_service::{start_control, PaperApplyError};
    config.validate_runtime()?;
    let mut control = start_control(
        config.bind,
        read_credential(&config.credential_file)?,
        config.market_capability_version.clone(),
        &config.claim_state_directory,
    )
    .await?;
    output(&serde_json::json!({
        "state": "idle", "bind": control.local_addr,
        "environment": "PAPER", "market_data_connected": false,
        "observations_file": config.observations_file,
    }))?;
    let request = tokio::select! {
        request = control.requests.recv() => request,
        _ = stop_signal() => None,
        _ = control.stop.changed() => None,
    };
    let native_result = if let Some(request) = request {
        match PaperNode::build_envelope(&config, &request.claim) {
            Ok(node) => {
                control
                    .status
                    .send_modify(|status| status.native_session_id = Some(node.session_id()));
                let _ = request.reply.send(Ok(()));
                node.run(control.stop.clone(), control.status.clone()).await
            }
            Err(error) => {
                let unsupported =
                    error.to_string() == "PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED";
                let code = if unsupported {
                    "PAPER_FORECAST_ACCOUNT_INITIALIZATION_UNSUPPORTED"
                } else {
                    "PAPER_INVALID_CLAIM_OR_CONFIGURATION"
                };
                publish_state(&control.status, PaperState::Failed, Some(code));
                let _ = request.reply.send(Err(if unsupported {
                    PaperApplyError::UnsupportedForecastInitialization
                } else {
                    PaperApplyError::InvalidClaim
                }));
                Err(anyhow!(code))
            }
        }
    } else {
        publish_state(&control.status, PaperState::Stopped, None);
        Ok(())
    };
    if native_result.is_err() && control.status.borrow().state != PaperState::Failed {
        publish_state(
            &control.status,
            PaperState::Failed,
            Some("PAPER_NATIVE_FAILED"),
        );
    }
    let retained = control.retain_terminal_observation().await;
    output(&control.status.borrow().clone())?;
    // Final observation is also retained by the native observer. The short
    // bounded drain lets an already-issued status/stop request see final state.
    tokio::time::sleep(Duration::from_secs(2)).await;
    control.shutdown.send_replace(true);
    control.task.await??;
    retained?;
    native_result
}

pub(crate) async fn control_request(
    origin: &str,
    credential_file: &Path,
    operation: &str,
    claim: Option<HandoffClaimViewV2>,
) -> Result<()> {
    let mut url = reqwest::Url::parse(origin)?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("PAPER_CONTROL_ORIGIN"))?;
    let address = host.trim_matches(['[', ']']).parse::<std::net::IpAddr>();
    ensure!(
        url.scheme() == "http"
            && address.is_ok_and(|address| address.is_loopback())
            && url.username().is_empty()
            && url.password().is_none()
            && (url.path().is_empty() || url.path() == "/")
            && url.query().is_none()
            && url.fragment().is_none(),
        "PAPER_CONTROL_LOOPBACK_ORIGIN_REQUIRED"
    );
    url.set_path(&format!("/downstream/v1/{operation}"));
    let credential = read_credential(credential_file)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()?;
    let request = if let Some(claim) = claim {
        client.post(url).json(&claim)
    } else if operation == "stop" {
        client.post(url)
    } else {
        client.get(url)
    };
    let response = request
        .bearer_auth(std::str::from_utf8(&credential)?)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "PAPER_CONTROL_REQUEST_REJECTED"
    );
    let status: serde_json::Value = response.json().await?;
    output(&status)
}

pub fn run(arguments: Arguments) -> Result<()> {
    let command = match arguments.command {
        Operation::Preflight { config, claim } => {
            return output(&preflight_envelope(
                &read_json(&config)?,
                &read_claim(&claim)?,
            )?);
        }
        command => command,
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            match command {
                Operation::Preflight { .. } => unreachable!(),
                Operation::Serve { config } => serve(read_json(&config)?).await,
                Operation::Apply {
                    origin,
                    credential_file,
                    claim,
                } => {
                    control_request(
                        &origin,
                        &credential_file,
                        "targets",
                        Some(read_claim(&claim)?),
                    )
                    .await
                }
                Operation::Status {
                    origin,
                    credential_file,
                } => control_request(&origin, &credential_file, "status", None).await,
                Operation::Stop {
                    origin,
                    credential_file,
                } => control_request(&origin, &credential_file, "stop", None).await,
            }
        })
}
