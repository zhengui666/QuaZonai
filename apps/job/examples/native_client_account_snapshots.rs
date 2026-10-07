//! Controlled no-order official Sandbox fixture for the native-to-HTTP pipeline.
//! No data client, strategy, venue network, credentials or real account is used.
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};

use anyhow::{bail, Result};
use contracts::Id;
use job::native_node_observer::NativeNodeObserver;
use nautilus_common::{
    enums::Environment,
    logging::logger::LoggerConfig,
    msgbus::{self, TypedHandler},
};
use nautilus_live::node::{LiveNode, NodeRunMode};
use nautilus_model::{
    enums::AccountType,
    events::PortfolioSnapshot,
    identifiers::{AccountId, ClientId, TraderId, Venue},
    types::{Currency, Money},
};
use nautilus_portfolio::config::PortfolioConfig;
use nautilus_sandbox::{SandboxExecutionClientConfig, SandboxExecutionClientFactory};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        bail!("expected controlled project ID and a new output path");
    }
    let project: Id = args[0].clone().try_into().map_err(anyhow::Error::msg)?;
    let output = PathBuf::from(&args[1]);
    let mut node = LiveNode::builder(TraderId::from("QZCLIENT-001"), Environment::Sandbox)?
        .with_logging(LoggerConfig {
            bypass_logging: true,
            ..Default::default()
        })
        .with_load_state(false)
        .with_save_state(false)
        .with_reconciliation(false)
        .with_timeout_connection(2)
        .with_timeout_portfolio(2)
        .with_delay_post_stop_secs(0)
        .with_delay_shutdown_secs(0)
        .with_portfolio_config(PortfolioConfig {
            snapshot_interval_ms: Some(10),
            ..Default::default()
        })
        .add_simulated_exec_client(
            Some("QZ-CLIENT-FIXTURE".into()),
            Box::new(SandboxExecutionClientFactory::new()),
            Box::new(SandboxExecutionClientConfig {
                account_id: AccountId::from("QZCLIENT-001"),
                venue: Venue::from("QZCLIENT"),
                starting_balances: vec![Money::from("10000 USDT")],
                base_currency: Some(Currency::USDT()),
                account_type: AccountType::Margin,
                ..Default::default()
            }),
        )?
        .build()?;
    let count = Rc::new(RefCell::new(0usize));
    let observed = count.clone();
    let handler = TypedHandler::from(move |_: &PortfolioSnapshot| *observed.borrow_mut() += 1);
    msgbus::subscribe_portfolio_snapshot(
        "events.portfolio.QZCLIENT-001".into(),
        handler.clone(),
        None,
    );
    let observer = NativeNodeObserver::attach_client_bound(
        &node,
        project,
        ClientId::new("QZ-CLIENT-FIXTURE"),
        &output,
        64,
    )?;
    observer.heartbeat()?;
    let handle = node.handle();
    let (run, drive) = tokio::join!(node.run_with_mode(NodeRunMode::Hosted), async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut ticker = tokio::time::interval(Duration::from_millis(10));
        loop {
            ticker.tick().await;
            observer.heartbeat()?;
            if (handle.is_running() && *count.borrow() > 0)
                || tokio::time::Instant::now() >= deadline
            {
                handle.stop();
                break;
            }
        }
        Ok::<(), anyhow::Error>(())
    });
    run?;
    drive?;
    observer.finish()?;
    msgbus::unsubscribe_portfolio_snapshot("events.portfolio.QZCLIENT-001".into(), &handler);
    if *count.borrow() == 0 {
        bail!("no official Sandbox snapshot observed");
    }
    if !node
        .kernel()
        .cache()
        .borrow()
        .orders(None, None, None, None, None)
        .is_empty()
    {
        bail!("no-order fixture unexpectedly contains orders");
    }
    Ok(())
}
