//! One user-built process entry; inspection never invokes a user factory or node.
#[cfg(feature = "account-runtime")]
use crate::account_runtime::AccountStrategyRegistration;
use crate::native_configured::{
    CompletedConfiguredRunFactory, ConfiguredFactory, ConfiguredRunFactory,
};
use crate::native_strategy::registry::StrategyRegistration;

pub fn capabilities(
    registration: bool,
    configured: bool,
    account_registration: bool,
) -> serde_json::Value {
    serde_json::json!({
        "schemaVersion":1,"identity":crate::build_identity(),
        "protocol":"quazonai-native-process-v1",
        "research":{"singleStrategyRegistration":registration,"configuredRunFactory":configured,"multiCatalogBindings":configured},
        "preparation":{"marketImport":cfg!(feature="market-history"),"fillObservationImport":cfg!(feature="market-history")},
        "account":{"compiled":cfg!(feature="account-runtime"),
            "adapters":if cfg!(feature="account-runtime") { vec!["HYPERLIQUID"] } else { vec![] },
            "registration":account_registration,
            "command":"account-runtime","requiresExplicitStart":true},
        "running":false
    })
}

/// Caller-declared compatibility, NOT verification or isolation of trusted Rust.
/// Covers construction, on_load/on_start and every subsequent callback: only
/// register Strategy values on this node; never create a second node or replace
/// thread-global senders/message-bus endpoints for the entire thread lifetime.
#[cfg(feature = "account-runtime")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountCompatibility {
    Unspecified,
    TrustedStrategyOnlySingleNode,
}
#[cfg(feature = "account-runtime")]
impl AccountCompatibility {
    pub fn declaration(self) -> Option<&'static str> {
        match self {
            Self::Unspecified => None,
            Self::TrustedStrategyOnlySingleNode => Some("TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1"),
        }
    }
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
    main_with_hooks_and_account_compatibility(
        registration,
        configured,
        account,
        AccountCompatibility::Unspecified,
    )
}

/// Explicit generated-entry choice. Existing entry points retain unspecified behavior.
#[cfg(feature = "account-runtime")]
pub fn main_with_hooks_and_account_compatibility(
    registration: Option<StrategyRegistration>,
    configured: Option<ConfiguredRunFactory>,
    account: Option<AccountStrategyRegistration>,
    compatibility: AccountCompatibility,
) {
    main_with_factory(
        registration,
        configured.map(ConfiguredFactory::Legacy),
        account,
        compatibility,
    )
}

#[cfg(feature = "account-runtime")]
pub fn main_with_completion_hooks(
    registration: Option<StrategyRegistration>,
    configured: Option<CompletedConfiguredRunFactory>,
    account: Option<AccountStrategyRegistration>,
) {
    main_with_completion_hooks_and_account_compatibility(
        registration,
        configured,
        account,
        AccountCompatibility::Unspecified,
    )
}
#[cfg(feature = "account-runtime")]
pub fn main_with_completion_hooks_and_account_compatibility(
    registration: Option<StrategyRegistration>,
    configured: Option<CompletedConfiguredRunFactory>,
    account: Option<AccountStrategyRegistration>,
    compatibility: AccountCompatibility,
) {
    main_with_factory(
        registration,
        configured.map(ConfiguredFactory::Completion),
        account,
        compatibility,
    )
}
#[cfg(feature = "account-runtime")]
fn main_with_factory(
    registration: Option<StrategyRegistration>,
    configured: Option<ConfiguredFactory>,
    account: Option<AccountStrategyRegistration>,
    compatibility: AccountCompatibility,
) {
    match std::env::args().nth(1).as_deref() {
        Some("capabilities") => {
            let mut value = capabilities(
                registration.is_some(),
                configured.is_some(),
                account.is_some(),
            );
            value["account"]["compatibilityDeclaration"] =
                serde_json::json!(compatibility.declaration());
            value["account"]["compatibilityVerified"] = serde_json::json!(false);
            println!("{value}");
        }
        Some("account-runtime") => {
            let result = on_account_thread(move |owner| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(crate::account_runtime::run_owned_registration(
                    account,
                    &owner,
                    compatibility,
                ))
            });
            if let Err(error) = result {
                eprintln!("account native runtime stopped: {error}");
                std::process::exit(1);
            }
        }
        _ => crate::cli::main_with_configured_factory(registration, configured),
    }
}

#[cfg(not(feature = "account-runtime"))]
pub fn main_with_hooks(
    registration: Option<StrategyRegistration>,
    configured: Option<ConfiguredRunFactory>,
    _account: Option<std::convert::Infallible>,
) {
    main_with_factory(registration, configured.map(ConfiguredFactory::Legacy))
}
#[cfg(not(feature = "account-runtime"))]
pub fn main_with_completion_hooks(
    registration: Option<StrategyRegistration>,
    configured: Option<CompletedConfiguredRunFactory>,
    _account: Option<std::convert::Infallible>,
) {
    main_with_factory(registration, configured.map(ConfiguredFactory::Completion))
}
#[cfg(not(feature = "account-runtime"))]
fn main_with_factory(
    registration: Option<StrategyRegistration>,
    configured: Option<ConfiguredFactory>,
) {
    match std::env::args().nth(1).as_deref() {
        Some("capabilities") => println!(
            "{}",
            capabilities(registration.is_some(), configured.is_some(), false)
        ),
        _ => crate::cli::main_with_configured_factory(registration, configured),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn inspection_reports_compiled_hooks_not_live_readiness() {
        let value = super::capabilities(false, true, false);
        let expected = if cfg!(feature = "account-runtime") {
            serde_json::json!(["HYPERLIQUID"])
        } else {
            serde_json::json!([])
        };
        assert_eq!(value["account"]["adapters"], expected);
        assert_eq!(value["running"], false);
        assert_eq!(
            value["preparation"]["marketImport"],
            cfg!(feature = "market-history")
        );
        assert_eq!(
            value["preparation"]["fillObservationImport"],
            cfg!(feature = "market-history")
        );
        assert_eq!(value["research"]["configuredRunFactory"], true);
        assert_eq!(value["research"]["multiCatalogBindings"], true);
        assert_eq!(
            super::capabilities(true, false, false)["research"]["multiCatalogBindings"],
            false
        );
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
                SyncTradingCommandSender, set_exec_cmd_sender, try_get_trading_cmd_sender,
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
