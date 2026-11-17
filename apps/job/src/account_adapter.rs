//! Select the official adapter factories and native configuration in one place.
//! Account lifecycle, cache, risk, strategy ownership and Sandbox remain in the
//! existing runtime. Only Hyperliquid is shipped; unknown adapters never fall back.
use nautilus_common::factories::{ClientConfig, DataClientFactory, ExecutionClientFactory};
use nautilus_hyperliquid::{
    common::enums::{HyperliquidEnvironment, HyperliquidProductType},
    HyperliquidDataClientConfig, HyperliquidDataClientFactory, HyperliquidExecutionClientConfig,
    HyperliquidExecutionClientFactory,
};
use nautilus_model::identifiers::{AccountId, ClientId};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountAdapter {
    #[default]
    Hyperliquid,
}
impl AccountAdapter {
    pub fn name(self) -> &'static str {
        match self {
            Self::Hyperliquid => "HYPERLIQUID",
        }
    }
    pub fn client_id(self) -> ClientId {
        ClientId::from(self.name())
    }
    pub(crate) fn data_client(
        self,
        environment: HyperliquidEnvironment,
    ) -> (Box<dyn DataClientFactory>, Box<dyn ClientConfig>) {
        match self {
            Self::Hyperliquid => (
                Box::new(HyperliquidDataClientFactory::new()),
                Box::new(HyperliquidDataClientConfig {
                    environment,
                    ..Default::default()
                }),
            ),
        }
    }
    pub(crate) fn execution_client(
        self,
        environment: HyperliquidEnvironment,
        account_id: AccountId,
        account_address: Option<String>,
        product_types: Vec<HyperliquidProductType>,
    ) -> (Box<dyn ExecutionClientFactory>, Box<dyn ClientConfig>) {
        match self {
            Self::Hyperliquid => (
                Box::new(HyperliquidExecutionClientFactory::new()),
                Box::new(HyperliquidExecutionClientConfig {
                    product_types,
                    account_id,
                    account_address,
                    environment,
                    normalize_prices: false,
                    include_builder_attribution: false,
                    ..Default::default()
                }),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_hyperliquid_is_selectable() {
        assert_eq!(AccountAdapter::default().name(), "HYPERLIQUID");
        assert_eq!(
            serde_json::from_str::<AccountAdapter>("\"HYPERLIQUID\"").unwrap(),
            AccountAdapter::Hyperliquid
        );
        assert!(serde_json::from_str::<AccountAdapter>("\"OTHER\"").is_err());
    }
    #[test]
    fn official_factory_configuration_preserves_existing_runtime_choices_offline() {
        for environment in [
            HyperliquidEnvironment::Mainnet,
            HyperliquidEnvironment::Testnet,
        ] {
            let adapter = AccountAdapter::Hyperliquid;
            let (factory, config) = adapter.data_client(environment);
            assert_eq!(factory.name(), "HYPERLIQUID");
            let native = config
                .as_any()
                .downcast_ref::<HyperliquidDataClientConfig>()
                .unwrap();
            assert_eq!(native.environment, environment);
            let account = AccountId::from("HYPERLIQUID-SYNTHETIC");
            let products = vec![HyperliquidProductType::Perp];
            let (factory, config) = adapter.execution_client(
                environment,
                account,
                Some("synthetic-not-used".into()),
                products.clone(),
            );
            assert_eq!(factory.name(), "HYPERLIQUID");
            let native = config
                .as_any()
                .downcast_ref::<HyperliquidExecutionClientConfig>()
                .unwrap();
            assert_eq!(native.account_id, account);
            assert_eq!(
                native.account_address.as_deref(),
                Some("synthetic-not-used")
            );
            assert_eq!(native.environment, environment);
            assert_eq!(native.product_types, products);
            assert!(!native.normalize_prices && !native.include_builder_attribution);
            assert!(native.private_key.is_none() && native.vault_address.is_none());
        }
    }
}
