//! Execute a user crate's official typed backtest configuration without reimplementing it.
use anyhow::{ensure, Result};
use contracts::{
    native_strategy::{NativeConfiguredBacktestRequestV1, NativeConfiguredBacktestResultV1},
    SchemaV1,
};
use nautilus_backtest::{config::BacktestRunConfig, engine::BacktestEngine, node::BacktestNode};
use std::path::Path;

/// The user crate builds official multi-venue/data/engine configuration and installs its
/// official Strategy instances. The closure can retain its typed parameters without a registry.
pub struct ConfiguredNativeRun {
    pub config: BacktestRunConfig,
    pub install: Box<dyn FnOnce(&mut BacktestEngine) -> Result<()>>,
}

pub type ConfiguredRunFactory =
    fn(catalog_root: &Path, name: &str, config: &serde_json::Value) -> Result<ConfiguredNativeRun>;

fn checked_catalog_path(root: &Path, protocol: &str, path: &Path) -> Result<()> {
    ensure!(
        protocol == "file",
        "NATIVE_CONFIGURED_CATALOG_PROTOCOL_UNSUPPORTED"
    );
    let selected = path.canonicalize()?;
    ensure!(
        selected.starts_with(root),
        "NATIVE_CONFIGURED_CATALOG_OUTSIDE_BOUND_ROOT"
    );
    Ok(())
}

pub fn run(
    root: &Path,
    request: NativeConfiguredBacktestRequestV1,
    factory: Option<ConfiguredRunFactory>,
) -> Result<NativeConfiguredBacktestResultV1> {
    let factory =
        factory.ok_or_else(|| anyhow::anyhow!("NATIVE_CONFIGURED_RUN_REQUIRES_USER_RUNNER"))?;
    let root = root.canonicalize()?;
    let ConfiguredNativeRun { config, install } = factory(&root, &request.name, &request.config)?;
    // Validate the returned configuration's real local catalog paths, not just the path
    // supplied to the factory. This does not sandbox arbitrary trusted user Rust code.
    for data in config.data() {
        checked_catalog_path(
            &root,
            data.catalog().fs_protocol(),
            Path::new(data.catalog().path()),
        )?;
    }
    let id = config.id().to_string();
    let omit_canonical = config.dispose_on_completion();
    let mut node = BacktestNode::builder(vec![config]).build()?;
    let result = (|| {
        node.build()?;
        let engine = node
            .get_engine_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("NATIVE_CONFIGURED_ENGINE_UNAVAILABLE"))?;
        install(engine)?;
        // Official actors/controllers may register strategies during start, quote or timer
        // callbacks. Do not impose a pre-run strategy-count gate on a typed configuration.
        let results = node.run()?;
        let canonical_result = if omit_canonical {
            None
        } else {
            node.get_engine(&id)
                .map(|engine| {
                    engine
                        .get_canonical_result()
                        .map(|result| result.as_value().clone())
                })
                .transpose()?
        };
        let equity_unavailable = if omit_canonical {
            Some("OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG")
        } else if results.is_empty() {
            Some("OFFICIAL_RUN_RESULT_UNAVAILABLE")
        } else if node.get_engine(&id).is_none() {
            Some("NATIVE_CONFIGURED_ENGINE_UNAVAILABLE")
        } else {
            None
        };
        let equity_performance = if equity_unavailable.is_none() {
            node.get_engine(&id)
                .map(crate::native_strategy::performance::from_engine)
        } else {
            None
        };
        Ok(NativeConfiguredBacktestResultV1 {
            schema_version: SchemaV1,
            request,
            native_version: crate::NAUTILUS_VERSION.into(),
            native_results: serde_json::to_value(results)?,
            canonical_result,
            equity_performance,
            equity_performance_unavailable_reason: equity_unavailable.map(str::to_owned),
            canonical_unavailable_reason: omit_canonical
                .then(|| "OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG".into()),
        })
    })();
    node.dispose();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_factory_checks_actual_catalog_path_and_protocol() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let bound = root.path().canonicalize().unwrap();
        checked_catalog_path(&bound, "file", root.path()).unwrap();
        assert!(checked_catalog_path(&bound, "file", other.path()).is_err());
        assert!(checked_catalog_path(&bound, "s3", root.path()).is_err());
        #[cfg(unix)]
        {
            let link = root.path().join("outside");
            std::os::unix::fs::symlink(other.path(), &link).unwrap();
            assert!(checked_catalog_path(&bound, "file", &link).is_err());
        }
    }

    #[test]
    fn official_controller_can_create_a_strategy_during_actor_start() {
        use nautilus_backtest::config::BacktestVenueConfig;
        use nautilus_common::{
            actor::{DataActor, DataActorConfig, DataActorCore},
            nautilus_actor,
        };
        use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
        use nautilus_model::{
            enums::{AccountType, BookType, OmsType},
            identifiers::{ActorId, StrategyId},
        };
        use nautilus_system::Controller;
        use nautilus_trading::{
            nautilus_strategy,
            strategy::{StrategyConfig, StrategyCore},
        };
        use std::sync::atomic::{AtomicBool, Ordering};
        static STARTED: AtomicBool = AtomicBool::new(false);

        #[derive(Debug)]
        struct DeferredOwner {
            core: DataActorCore,
            controller: Controller,
        }
        #[derive(Debug)]
        struct LateStrategy {
            core: StrategyCore,
        }
        nautilus_strategy!(LateStrategy);
        impl DataActor for LateStrategy {
            fn on_start(&mut self) -> Result<()> {
                STARTED.store(true, Ordering::SeqCst);
                Ok(())
            }
        }
        nautilus_actor!(DeferredOwner);
        impl DataActor for DeferredOwner {
            fn on_start(&mut self) -> Result<()> {
                self.controller.create_strategy(
                    LateStrategy {
                        core: StrategyCore::new(StrategyConfig {
                            strategy_id: Some(StrategyId::from("LATE-001")),
                            order_id_tag: Some("001".into()),
                            ..Default::default()
                        }),
                    },
                    true,
                )?;
                Ok(())
            }
        }
        fn factory(_: &Path, name: &str, value: &serde_json::Value) -> Result<ConfiguredNativeRun> {
            ensure!(name == "actor-owner", "OWNER_RUN_UNKNOWN");
            let venue = BacktestVenueConfig::builder()
                .name("SIM")
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Margin)
                .book_type(BookType::L1_MBP)
                .starting_balances(vec!["1000 USD".into()])
                .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
                .build()?;
            let config = BacktestRunConfig::builder()
                .venues(vec![venue])
                .data(Vec::new())
                .start(1u64.into())
                .end(2u64.into())
                .raise_exception(true)
                .dispose_on_completion(value["dispose"].as_bool().unwrap_or(false))
                .build()?;
            Ok(ConfiguredNativeRun {
                config,
                install: Box::new(|engine| {
                    let controller = Controller::new(engine.kernel().trader.clone(), None);
                    engine.add_actor(DeferredOwner {
                        controller,
                        core: DataActorCore::new(DataActorConfig {
                            actor_id: Some(ActorId::from("DEFERRED-001")),
                            ..Default::default()
                        }),
                    })?;
                    assert!(engine.kernel().trader.borrow().strategy_ids().is_empty());
                    Ok(())
                }),
            })
        }
        let root = tempfile::tempdir().unwrap();
        let request = NativeConfiguredBacktestRequestV1 {
            schema_version: SchemaV1,
            name: "actor-owner".into(),
            config: serde_json::Value::Null,
        };
        STARTED.store(false, Ordering::SeqCst);
        let result = run(root.path(), request.clone(), Some(factory)).unwrap();
        assert!(STARTED.load(Ordering::SeqCst));
        assert_eq!(result.native_results.as_array().unwrap().len(), 1);
        let equity = result.equity_performance.as_ref().unwrap();
        assert_eq!(equity.source, "NAUTILUS_PORTFOLIO_SNAPSHOTS");
        assert!(equity.statistics.iter().all(|stat| stat.value.is_none()));
        assert!(result.equity_performance_unavailable_reason.is_none());
        let mut disposed_request = request.clone();
        disposed_request.config = serde_json::json!({"dispose":true});
        let disposed = run(root.path(), disposed_request, Some(factory)).unwrap();
        assert!(disposed.canonical_result.is_none());
        assert!(disposed.equity_performance.is_none());
        assert_eq!(
            disposed.equity_performance_unavailable_reason.as_deref(),
            Some("OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG")
        );
        let mut unknown = request;
        unknown.name = "unknown-owner".into();
        assert_eq!(
            run(root.path(), unknown, Some(factory))
                .unwrap_err()
                .to_string(),
            "OWNER_RUN_UNKNOWN"
        );
    }

    #[test]
    fn stock_runner_has_no_implicit_configured_strategy() {
        let request = NativeConfiguredBacktestRequestV1 {
            schema_version: SchemaV1,
            name: "owner::multi".into(),
            config: serde_json::json!({"amount": "0.000000000000000001"}),
        };
        assert_eq!(
            run(Path::new("/unused"), request, None)
                .unwrap_err()
                .to_string(),
            "NATIVE_CONFIGURED_RUN_REQUIRES_USER_RUNNER"
        );
    }
}
