//! One user-built process entry; inspection never invokes a user factory or node.
#[cfg(feature = "account-runtime")]
use crate::account_runtime::AccountStrategyRegistration;
use crate::native_configured::ConfiguredRunFactory;
use crate::native_strategy::registry::StrategyRegistration;

pub fn capabilities(
    registration: bool,
    configured: bool,
    account_registration: bool,
) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion":1,"identity":crate::build_identity(),
        "protocol":"quazonai-native-process-v1",
        "research":{"singleStrategyRegistration":registration,"configuredRunFactory":configured},
        "account":{"compiled":cfg!(feature="account-runtime"),"registration":account_registration,
            "command":"account-runtime","requiresExplicitStart":true},
        "running":false
    })
}

/// Generated QZ-side main calls this with statically linked official user factories.
/// Constructed only inside this entry's fresh dedicated OS thread and runtime.
/// Not a caller-supplied flag, and never handed to user registration callbacks.
#[cfg(feature = "account-runtime")]
pub(crate) struct OwnedAccountRuntime(());

#[cfg(feature = "account-runtime")]
fn on_account_thread(
    run: impl FnOnce(OwnedAccountRuntime) -> anyhow::Result<()> + Send + 'static,
) -> anyhow::Result<()> {
    // A new tokio runtime alone would still inherit the calling OS thread's
    // Nautilus TLS. No node/registry/cache references cross this boundary.
    std::thread::Builder::new()
        .name("qz-account-native-node".into())
        .spawn(move || run(OwnedAccountRuntime(())))?
        .join()
        .map_err(|_| anyhow::anyhow!("account_native_owner_thread_failed"))?
}

/// Account commands are serviced on their original current-thread native actor loop.
#[cfg(feature = "account-runtime")]
pub fn main_with_hooks(
    registration: Option<StrategyRegistration>,
    configured: Option<ConfiguredRunFactory>,
    account: Option<AccountStrategyRegistration>,
) {
    match std::env::args().nth(1).as_deref() {
        Some("capabilities") => println!(
            "{}",
            capabilities(
                registration.is_some(),
                configured.is_some(),
                account.is_some()
            )
        ),
        Some("account-runtime") => {
            let result = on_account_thread(move |owner| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(crate::account_runtime::run_owned_registration(
                    account, &owner,
                ))
            });
            if let Err(error) = result {
                eprintln!("account native runtime stopped: {error}");
                std::process::exit(1);
            }
        }
        _ => crate::cli::main_with_native_hooks(registration, configured),
    }
}

#[cfg(not(feature = "account-runtime"))]
pub fn main_with_hooks(
    registration: Option<StrategyRegistration>,
    configured: Option<ConfiguredRunFactory>,
    _account: Option<std::convert::Infallible>,
) {
    match std::env::args().nth(1).as_deref() {
        Some("capabilities") => println!(
            "{}",
            capabilities(registration.is_some(), configured.is_some(), false)
        ),
        _ => crate::cli::main_with_native_hooks(registration, configured),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn inspection_reports_compiled_hooks_not_live_readiness() {
        let value = super::capabilities(false, true, false);
        assert_eq!(value["running"], false);
        assert_eq!(value["research"]["configuredRunFactory"], true);
        assert_eq!(value["account"]["registration"], false);
        assert_eq!(value["account"]["requiresExplicitStart"], true);
    }
}

#[cfg(all(test, feature = "account-runtime"))]
mod owner_thread_tests {
    #[test]
    fn fresh_owner_thread_does_not_inherit_callers_native_sender() {
        std::thread::spawn(|| {
            use nautilus_common::runner::{
                set_exec_cmd_sender, try_get_trading_cmd_sender, SyncTradingCommandSender,
            };
            set_exec_cmd_sender(std::sync::Arc::new(SyncTradingCommandSender));
            assert!(try_get_trading_cmd_sender().is_some());
            let caller = std::thread::current().id();
            super::on_account_thread(move |_owner| {
                assert_ne!(std::thread::current().id(), caller);
                assert!(try_get_trading_cmd_sender().is_none());
                Ok(())
            })
            .unwrap();
            assert!(try_get_trading_cmd_sender().is_some());
        })
        .join()
        .unwrap();
    }
}
