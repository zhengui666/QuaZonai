//! Execute a user crate's official typed backtest configuration without reimplementing it.
use anyhow::{Result, ensure};
use contracts::{
    SchemaV1,
    native_strategy::{NativeConfiguredBacktestRequestV1, NativeConfiguredBacktestResultV1},
};
use nautilus_backtest::{
    config::BacktestRunConfig, engine::BacktestEngine, node::BacktestNode, result::BacktestResult,
};
use std::path::{Path, PathBuf};

#[cfg(feature = "hf-catalog")]
use nautilus_persistence::{
    backend::{extend_catalog_factories, parquet::catalog::ParquetDataCatalog},
    catalog::{
        factory::{CatalogFactory, validate_catalog_params},
        traits::CatalogReader,
    },
    config::{CatalogBackendType, DataCatalogConfig},
};
#[cfg(feature = "hf-catalog")]
use std::{collections::{BTreeMap, btree_map::Entry}, sync::{Arc, Mutex}};

#[cfg(feature = "hf-catalog")]
const HF_BACKEND: &str = "QuaZonaiHf";

/// The user crate builds official multi-venue/data/engine configuration and installs its
/// official Strategy instances. The closure can retain its typed parameters without a registry.
pub struct ConfiguredNativeRun {
    pub config: BacktestRunConfig,
    pub install: Box<dyn FnOnce(&mut BacktestEngine) -> Result<()>>,
}

pub type ConfiguredRunFactory =
    fn(catalog_root: &Path, name: &str, config: &serde_json::Value) -> Result<ConfiguredNativeRun>;

/// Optional completion is outside the legacy struct so existing owner literals remain valid.
pub struct CompletedConfiguredNativeRun {
    pub run: ConfiguredNativeRun,
    pub after_success:
        Option<Box<dyn FnOnce(&mut BacktestNode, &str, &[BacktestResult]) -> Result<()>>>,
}
impl From<ConfiguredNativeRun> for CompletedConfiguredNativeRun {
    fn from(run: ConfiguredNativeRun) -> Self {
        Self {
            run,
            after_success: None,
        }
    }
}
impl ConfiguredNativeRun {
    /// Invoked only after the matching native run succeeds, after native clear_data
    /// but before QZ reads canonical output and disposes the still-present engine.
    /// A returned result may be stopped: the owner checks native outcome for natural EOF.
    pub fn with_completion(
        self,
        hook: impl FnOnce(&mut BacktestNode, &str, &[BacktestResult]) -> Result<()> + 'static,
    ) -> CompletedConfiguredNativeRun {
        CompletedConfiguredNativeRun {
            run: self,
            after_success: Some(Box::new(hook)),
        }
    }
}
pub type CompletedConfiguredRunFactory =
    fn(&Path, &str, &serde_json::Value) -> Result<CompletedConfiguredNativeRun>;
#[derive(Clone, Copy)]
pub(crate) enum ConfiguredFactory {
    Legacy(ConfiguredRunFactory),
    Completion(CompletedConfiguredRunFactory),
}
impl ConfiguredFactory {
    fn build(
        self,
        root: &Path,
        name: &str,
        config: &serde_json::Value,
    ) -> Result<CompletedConfiguredNativeRun> {
        match self {
            Self::Legacy(factory) => factory(root, name, config).map(Into::into),
            Self::Completion(factory) => factory(root, name, config),
        }
    }
}

fn checked_catalog_path(roots: &[PathBuf], protocol: &str, path: &Path) -> Result<()> {
    ensure!(
        protocol == "file",
        "NATIVE_CONFIGURED_CATALOG_PROTOCOL_UNSUPPORTED"
    );
    let selected = path.canonicalize()?;
    ensure!(
        roots.iter().any(|root| !crate::catalog::is_hf_research_catalog(root)
            && selected.starts_with(root)),
        "NATIVE_CONFIGURED_CATALOG_OUTSIDE_BOUND_ROOT"
    );
    Ok(())
}

// Carry an admitted URI to the existing owner factory without normalizing it as a path.
fn bound_catalog_root(root: &Path) -> Result<PathBuf> {
    if crate::catalog::is_hf_research_catalog(root) {
        #[cfg(feature = "hf-catalog")]
        {
            let uri = root
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("NATIVE_HF_CATALOG_INVALID"))?;
            crate::hf_catalog::validate_uri(uri)?;
            return Ok(root.to_owned());
        }
        #[cfg(not(feature = "hf-catalog"))]
        anyhow::bail!("NATIVE_HF_CATALOG_UNAVAILABLE");
    }
    Ok(root.canonicalize()?)
}

#[cfg(feature = "hf-catalog")]
fn is_bound_hf_uri(roots: &[PathBuf], uri: &str) -> bool {
    roots.iter().any(|root| {
        crate::catalog::is_hf_research_catalog(root) && root.to_str() == Some(uri)
    })
}

#[cfg(feature = "hf-catalog")]
fn checked_hf_catalog(roots: &[PathBuf], config: &DataCatalogConfig) -> Result<bool> {
    let is_hf_backend = matches!(config.catalog_backend(),
        CatalogBackendType::External(name) if name == HF_BACKEND);
    let is_hf_path = crate::catalog::is_hf_research_catalog(Path::new(config.path()));
    if !is_hf_backend && !is_hf_path && config.fs_protocol() != "hf" {
        return Ok(false);
    }
    ensure!(
        is_hf_backend && is_hf_path && config.fs_protocol() == "hf",
        "NATIVE_CONFIGURED_HF_REQUIRES_EXPLICIT_BACKEND"
    );
    ensure!(
        is_bound_hf_uri(roots, config.path()),
        "NATIVE_CONFIGURED_CATALOG_OUTSIDE_BOUND_ROOT"
    );
    Ok(true)
}

#[cfg(feature = "hf-catalog")]
fn configured_hf_factory(roots: Vec<PathBuf>) -> CatalogFactory {
    // Each native open gets its own session, while immutable discovery is shared
    // by all stream and instrument opens for this URI in this execution.
    let open = Mutex::new(BTreeMap::<String, ParquetDataCatalog>::new());
    Arc::new(move |connect| {
        ensure!(
            is_bound_hf_uri(&roots, &connect.uri),
            "NATIVE_CONFIGURED_CATALOG_OUTSIDE_BOUND_ROOT"
        );
        ensure!(
            connect.storage_options.is_none(),
            "NATIVE_HF_CATALOG_ANONYMOUS_ONLY"
        );
        validate_catalog_params(HF_BACKEND, connect.params.as_ref(), &[])?;
        ensure!(
            connect.batch_size != Some(0),
            "NATIVE_HF_CATALOG_INVALID_BATCH_SIZE"
        );
        let mut open = open
            .lock()
            .map_err(|_| anyhow::anyhow!("NATIVE_HF_CATALOG_CACHE_UNAVAILABLE"))?;
        let catalog = match open.entry(connect.uri.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(crate::hf_catalog::open(&connect.uri)?),
        };
        let default_batch_size = catalog.batch_size;
        if let Some(batch_size) = connect.batch_size {
            catalog.batch_size = batch_size;
        }
        let session = catalog.fork_query_catalog();
        catalog.batch_size = default_batch_size;
        session?.ok_or_else(|| anyhow::anyhow!("NATIVE_HF_CATALOG_SESSION_UNAVAILABLE"))
    })
}

pub fn run(
    root: &Path,
    request: NativeConfiguredBacktestRequestV1,
    factory: Option<ConfiguredRunFactory>,
) -> Result<NativeConfiguredBacktestResultV1> {
    run_with_catalogs(root, &[], request, factory)
}

/// Executes on the original root plus explicitly admitted additional catalog roots.
/// The factory still receives the primary root; returned data configs are checked
/// against every canonical admitted root without broadening to a common parent.
pub fn run_with_catalogs(
    root: &Path,
    additional_catalogs: &[PathBuf],
    request: NativeConfiguredBacktestRequestV1,
    factory: Option<ConfiguredRunFactory>,
) -> Result<NativeConfiguredBacktestResultV1> {
    run_with_factory(
        root,
        additional_catalogs,
        request,
        factory.map(ConfiguredFactory::Legacy),
    )
}

pub fn run_with_completion(
    root: &Path,
    additional_catalogs: &[PathBuf],
    request: NativeConfiguredBacktestRequestV1,
    factory: Option<CompletedConfiguredRunFactory>,
) -> Result<NativeConfiguredBacktestResultV1> {
    run_with_factory(
        root,
        additional_catalogs,
        request,
        factory.map(ConfiguredFactory::Completion),
    )
}

pub(crate) fn run_with_factory(
    root: &Path,
    additional_catalogs: &[PathBuf],
    request: NativeConfiguredBacktestRequestV1,
    factory: Option<ConfiguredFactory>,
) -> Result<NativeConfiguredBacktestResultV1> {
    let factory =
        factory.ok_or_else(|| anyhow::anyhow!("NATIVE_CONFIGURED_RUN_REQUIRES_USER_RUNNER"))?;
    let root = bound_catalog_root(root)?;
    let mut roots = vec![root.clone()];
    for catalog in additional_catalogs {
        let canonical = bound_catalog_root(catalog)?;
        if !roots.contains(&canonical) {
            roots.push(canonical);
        }
    }
    let CompletedConfiguredNativeRun {
        run: ConfiguredNativeRun { config, install },
        after_success,
    } = factory.build(&root, &request.name, &request.config)?;
    let has_completion = after_success.is_some();
    ensure!(
        !has_completion || !config.dispose_on_completion(),
        "NATIVE_CONFIGURED_COMPLETION_REQUIRES_RETAINED_ENGINE"
    );
    // Check the returned data config, not just the arguments passed to trusted owner Rust.
    // Local paths remain canonical descendants; public catalog locators must match exactly.
    #[cfg(feature = "hf-catalog")]
    let mut uses_hf = false;
    for data in config.data() {
        #[cfg(feature = "hf-catalog")]
        if checked_hf_catalog(&roots, data.catalog())? {
            uses_hf = true;
            continue;
        }
        checked_catalog_path(
            &roots,
            data.catalog().fs_protocol(),
            Path::new(data.catalog().path()),
        )?;
    }
    let id = config.id().to_string();
    let omit_canonical = config.dispose_on_completion();
    let builder = BacktestNode::builder(vec![config]);
    #[cfg(feature = "hf-catalog")]
    let builder = if uses_hf {
        builder.with_catalog_factories(extend_catalog_factories([(
            HF_BACKEND.into(),
            configured_hf_factory(roots),
        )])?)
    } else {
        builder
    };
    let mut node = builder.build()?;
    let execute = || {
        node.build()?;
        let engine = node
            .get_engine_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("NATIVE_CONFIGURED_ENGINE_UNAVAILABLE"))?;
        install(engine)?;
        // Official actors/controllers may register strategies during start, quote or timer
        // callbacks. Do not impose a pre-run strategy-count gate on a typed configuration.
        let results = node.run()?;
        if let Some(complete) = after_success {
            // raise_exception=false can swallow a run error and return Ok([]).
            ensure!(
                results.len() == 1 && results[0].run_config_id.as_deref() == Some(id.as_str()),
                "NATIVE_CONFIGURED_COMPLETION_REQUIRES_SUCCESSFUL_RUN"
            );
            ensure!(
                node.get_engine(&id).is_some(),
                "NATIVE_CONFIGURED_ENGINE_UNAVAILABLE"
            );
            complete(&mut node, &id, &results)?;
        }
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
    };
    let result = execute();
    node.dispose();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "hf-catalog")]
    #[test]
    fn typed_hf_config_uses_only_the_exact_admitted_immutable_catalog() {
        let uri = "hf://datasets/owner/repo@0123456789abcdef0123456789abcdef01234567";
        let primary = bound_catalog_root(Path::new(uri)).unwrap();
        assert_eq!(primary.to_str(), Some(uri));
        let roots = vec![primary];
        let config = |path: &str, protocol: Option<&str>, backend| {
            DataCatalogConfig::new(path.to_owned(), protocol.map(str::to_owned), backend)
        };
        let hf = || Some(CatalogBackendType::External(HF_BACKEND.into()));
        let correct = config(uri, Some("hf"), hf());
        assert!(checked_hf_catalog(&roots, &correct).unwrap());
        assert_eq!(correct.connect_config().uri, uri);
        assert!(checked_hf_catalog(&roots, &config(uri, None, hf())).is_err());
        assert!(checked_hf_catalog(&roots, &config(uri, Some("hf"), None)).is_err());
        assert!(
            checked_hf_catalog(&roots, &config(&format!("{uri}/other"), Some("hf"), hf())).is_err()
        );
        let other_root = config(&uri.replace("owner", "other"), Some("hf"), hf());
        assert!(checked_hf_catalog(&roots, &other_root).is_err());
        assert!(!checked_hf_catalog(&roots, &config("/tmp/local", None, None)).unwrap());

        let factory = configured_hf_factory(roots);
        assert!(factory(&other_root.connect_config()).is_err());
    }

    #[cfg(all(feature = "hf-catalog", feature = "high-precision"))]
    #[test]
    #[ignore = "explicit anonymous replay of the existing public Oct 13 native ALT slice"]
    fn configured_owner_replays_published_hf_catalog_without_orders() -> Result<()> {
        use nautilus_backtest::config::{BacktestDataConfig, BacktestVenueConfig};
        use nautilus_common::actor::DataActor;
        use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
        use nautilus_model::{
            data::{NautilusDataType, OrderBookDeltas, TradeTick},
            enums::{AccountType, BookType, OmsType, RecordFlag},
            identifiers::{InstrumentId, StrategyId},
        };
        use nautilus_trading::{
            nautilus_strategy,
            strategy::{StrategyConfig, StrategyCore},
        };
        use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

        const START: u64 = 1_791_864_000_000_000_000;
        const END: u64 = 1_791_867_600_000_000_000;
        #[derive(Debug, Default)]
        struct Counts {
            trades: AtomicU64,
            book_rows: AtomicU64,
        }
        #[derive(Debug)]
        struct Observer {
            core: StrategyCore,
            id: InstrumentId,
            counts: Arc<Counts>,
        }
        nautilus_strategy!(Observer);
        impl DataActor for Observer {
            fn on_start(&mut self) -> Result<()> {
                self.subscribe_trades(self.id, None, None);
                self.subscribe_book_deltas(self.id, BookType::L2_MBP, None, None, false, None);
                Ok(())
            }

            fn on_book_deltas(&mut self, batch: &OrderBookDeltas) -> Result<()> {
                assert_eq!(batch.instrument_id, self.id);
                assert!(batch.deltas.last().is_some_and(|last| last.flags & RecordFlag::F_LAST as u8 != 0));
                for row in &batch.deltas {
                    assert!((START..END).contains(&row.ts_init.as_u64()));
                    assert!(row.ts_event <= row.ts_init);
                }
                self.counts.book_rows.fetch_add(batch.deltas.len() as u64, Ordering::Relaxed);
                Ok(())
            }

            fn on_trade(&mut self, row: &TradeTick) -> Result<()> {
                assert_eq!(row.instrument_id, self.id);
                assert!((START..END).contains(&row.ts_init.as_u64()));
                assert!(row.ts_event <= row.ts_init);
                self.counts.trades.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }

        fn owner(root: &Path, _: &str, _: &serde_json::Value) -> Result<CompletedConfiguredNativeRun> {
            let id = InstrumentId::from("ALT-USD-PERP.HYPERLIQUID");
            let catalog = DataCatalogConfig::new(
                root.to_str().ok_or_else(|| anyhow::anyhow!("expected UTF-8 locator"))?.to_owned(),
                Some("hf".into()),
                Some(CatalogBackendType::External(HF_BACKEND.into())),
            ).with_read_only(true);
            let books = BacktestDataConfig::builder()
                .catalog(catalog.clone())
                .data_type(NautilusDataType::OrderBookDelta)
                .instrument_id(id)
                .optimize_file_loading(false)
                .batch_deltas(true)
                .build()?;
            let trades = BacktestDataConfig::builder()
                .catalog(catalog)
                .data_type(NautilusDataType::TradeTick)
                .instrument_id(id)
                .optimize_file_loading(false)
                .build()?;
            let venue = BacktestVenueConfig::builder()
                .name(id.venue.as_str())
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Margin)
                .book_type(BookType::L2_MBP)
                .starting_balances(vec!["1000 USDC".into()])
                .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
                .build()?;
            let config = BacktestRunConfig::builder()
                .venues(vec![venue])
                .data(vec![books, trades])
                .start(START.into())
                .end((END - 1).into())
                .chunk_size(4096)
                .raise_exception(true)
                .dispose_on_completion(false)
                .build()?;
            let counts = Arc::new(Counts::default());
            let observer_counts = Arc::clone(&counts);
            let run = ConfiguredNativeRun {
                config,
                install: Box::new(move |engine| engine.add_strategy(Observer {
                    core: StrategyCore::new(StrategyConfig {
                        strategy_id: Some(StrategyId::from("HF-OBSERVER-001")),
                        order_id_tag: Some("001".into()),
                        ..Default::default()
                    }),
                    id,
                    counts: observer_counts,
                })),
            };
            Ok(run.with_completion(move |node, run_id, _| {
                assert_eq!(counts.trades.load(Ordering::Relaxed), 323);
                assert_eq!(counts.book_rows.load(Ordering::Relaxed), 27_429);
                let canonical = node.get_engine(run_id).unwrap().get_canonical_result()?;
                assert_eq!(canonical.as_value()["run"]["outcome"], "completed");
                for table in ["orders", "fills", "positions"] {
                    assert!(canonical.as_value()[table].as_array().unwrap().is_empty(), "{table}");
                }
                Ok(())
            }))
        }

        let uri = std::env::var("QZ_TEST_HF_CATALOG")?;
        let result = run_with_completion(
            Path::new(&uri),
            &[],
            NativeConfiguredBacktestRequestV1 {
                schema_version: SchemaV1,
                name: "public-hf-owner".into(),
                config: serde_json::Value::Null,
            },
            Some(owner),
        )?;
        assert_eq!(result.native_results.as_array().unwrap().len(), 1);
        Ok(())
    }

    #[test]
    fn typed_factory_checks_actual_catalog_path_and_protocol() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let bound = vec![root.path().canonicalize().unwrap()];
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
    fn multiple_bound_roots_do_not_admit_siblings_or_symlink_escapes() {
        let temp = tempfile::tempdir().unwrap();
        let primary = temp.path().join("trades");
        let secondary = temp.path().join("books");
        let unbound = temp.path().join("unbound");
        for path in [&primary, &secondary, &unbound] {
            std::fs::create_dir(path).unwrap();
        }
        let roots = vec![
            primary.canonicalize().unwrap(),
            secondary.canonicalize().unwrap(),
        ];
        checked_catalog_path(&roots, "file", &primary).unwrap();
        checked_catalog_path(&roots, "file", &secondary).unwrap();
        assert!(checked_catalog_path(&roots[..1], "file", &secondary).is_err());
        assert!(checked_catalog_path(&roots, "file", &unbound).is_err());
        assert!(checked_catalog_path(&roots, "file", temp.path()).is_err());
        assert!(checked_catalog_path(&roots, "s3", &secondary).is_err());
        #[cfg(unix)]
        {
            let link = secondary.join("outside");
            std::os::unix::fs::symlink(&unbound, &link).unwrap();
            assert!(checked_catalog_path(&roots, "file", &link).is_err());
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
    fn completion_uses_real_native_results_and_disposes_on_result_errors() {
        use nautilus_backtest::config::BacktestVenueConfig;
        use nautilus_common::{
            actor::{DataActor, DataActorConfig, DataActorCore},
            nautilus_actor,
        };
        use nautilus_execution::models::fee::{FeeModelAny, MakerTakerFeeModel};
        use nautilus_model::{
            data::{Data, QuoteTick},
            enums::{AccountType, BookType, OmsType},
            identifiers::{ActorId, InstrumentId},
            types::{Price, Quantity},
        };
        use std::sync::Mutex;
        static EVENTS: Mutex<Vec<&str>> = Mutex::new(Vec::new());
        #[derive(Debug)]
        struct LifecycleActor {
            core: DataActorCore,
            stop: bool,
        }
        nautilus_actor!(LifecycleActor);
        impl DataActor for LifecycleActor {
            fn on_start(&mut self) -> Result<()> {
                EVENTS.lock().unwrap().push("start");
                if self.stop {
                    self.shutdown_system(Some("synthetic stop".into()));
                }
                Ok(())
            }
            fn on_dispose(&mut self) -> Result<()> {
                EVENTS.lock().unwrap().push("dispose");
                Ok(())
            }
        }
        fn factory(
            _: &Path,
            scenario: &str,
            _: &serde_json::Value,
        ) -> Result<CompletedConfiguredNativeRun> {
            let scenario = scenario.to_string();
            let venue = BacktestVenueConfig::builder()
                .name("SIM")
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Margin)
                .book_type(BookType::L1_MBP)
                .starting_balances(vec!["1000 USD".into()])
                .fee_model(FeeModelAny::MakerTaker(MakerTakerFeeModel::zero()))
                .build()?;
            // Missing start is legal; the synthetic future tick makes the real engine reject
            // start > end during run (not at builder/install time).
            let config = BacktestRunConfig::builder()
                .venues(vec![venue])
                .data(Vec::new())
                .end(1u64.into())
                .raise_exception(scenario != "swallowed")
                .dispose_on_completion(scenario == "early-dispose")
                .build()?;
            let install_scenario = scenario.clone();
            let run = ConfiguredNativeRun {
                config,
                install: Box::new(move |engine| {
                    EVENTS.lock().unwrap().push("install");
                    engine.add_actor(LifecycleActor {
                        core: DataActorCore::new(DataActorConfig {
                            actor_id: Some(ActorId::from("COMPLETION-001")),
                            ..Default::default()
                        }),
                        stop: install_scenario == "stopped",
                    })?;
                    if install_scenario == "install-error" {
                        anyhow::bail!("SYNTHETIC_INSTALL_ERROR");
                    }
                    if matches!(install_scenario.as_str(), "run-error" | "swallowed") {
                        engine.add_data(
                            vec![Data::Quote(QuoteTick::new(
                                InstrumentId::from("TEST.SIM"),
                                Price::from("1"),
                                Price::from("2"),
                                Quantity::from("1"),
                                Quantity::from("1"),
                                2u64.into(),
                                2u64.into(),
                            ))],
                            None,
                            false,
                            true,
                        )?;
                    }
                    Ok(())
                }),
            };
            Ok(run.with_completion(move |node, id, results| {
                EVENTS.lock().unwrap().push("completion");
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].run_config_id.as_deref(), Some(id));
                let engine = node.get_engine(id).unwrap();
                let canonical = engine.get_canonical_result()?;
                assert_eq!(
                    canonical.as_value()["run"]["outcome"],
                    if scenario == "stopped" {
                        "stopped"
                    } else {
                        "completed"
                    }
                );
                assert!(!EVENTS.lock().unwrap().contains(&"dispose"));
                if scenario == "callback-error" {
                    anyhow::bail!("SYNTHETIC_CALLBACK_ERROR");
                }
                if scenario == "stopped" {
                    anyhow::bail!("OWNER_REQUIRES_NATURAL_COMPLETION");
                }
                Ok(())
            }))
        }
        let root = tempfile::tempdir().unwrap();
        for (scenario, expected) in [
            ("success", None),
            ("callback-error", Some("SYNTHETIC_CALLBACK_ERROR")),
            ("install-error", Some("SYNTHETIC_INSTALL_ERROR")),
            ("run-error", Some("start was > end")),
            (
                "swallowed",
                Some("NATIVE_CONFIGURED_COMPLETION_REQUIRES_SUCCESSFUL_RUN"),
            ),
            (
                "early-dispose",
                Some("NATIVE_CONFIGURED_COMPLETION_REQUIRES_RETAINED_ENGINE"),
            ),
            ("stopped", Some("OWNER_REQUIRES_NATURAL_COMPLETION")),
        ] {
            EVENTS.lock().unwrap().clear();
            let result = run_with_completion(
                root.path(),
                &[],
                NativeConfiguredBacktestRequestV1 {
                    schema_version: SchemaV1,
                    name: scenario.into(),
                    config: serde_json::Value::Null,
                },
                Some(factory),
            );
            if let Some(expected) = expected {
                assert_eq!(result.unwrap_err().to_string(), expected, "{scenario}");
            } else {
                assert_eq!(result.unwrap().native_results.as_array().unwrap().len(), 1);
            }
            let events = EVENTS.lock().unwrap();
            if scenario == "early-dispose" {
                assert!(events.is_empty());
            } else {
                assert_eq!(events.last(), Some(&"dispose"), "{scenario}: {events:?}");
                assert_eq!(
                    events.iter().filter(|e| **e == "completion").count(),
                    usize::from(matches!(scenario, "success" | "callback-error" | "stopped")),
                    "{scenario}: {events:?}"
                );
            }
        }
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
