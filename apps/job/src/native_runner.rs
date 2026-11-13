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
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(anyhow::Error::from)
                .and_then(|runtime| {
                    runtime.block_on(crate::account_runtime::run_with_registration(account))
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
