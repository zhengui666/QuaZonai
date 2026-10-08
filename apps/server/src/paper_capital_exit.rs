//! Deployment opt-in for the already authenticated original Paper source.
//! No credentials, HTTP registration endpoint, or caller-selected budget key.
use contracts::{
    Id, SchemaV1, account_observation::AccountObservationReceiptV2, forward::ForwardEnvironmentV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use store::{Store, StoreError, authority::Actor, capital_exit::CapitalExitOwnerRegistration};

pub const REGISTERED_SOURCE_HEADER: &str = "x-qz-capital-exit-source";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperCapitalExitOwnerConfiguration {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub downstream_id: Id,
    pub native_trader_id: String,
    pub native_account_id: String,
    pub native_client_id: String,
    pub native_version: String,
    pub venue: String,
    pub collateral_currency: String,
    pub instrument_id: String,
    pub controlled_strategy_ids: Vec<String>,
}

#[derive(Clone, Default)]
pub struct PaperCapitalExitOwners {
    owners: Vec<PaperCapitalExitOwnerConfiguration>,
}
impl PaperCapitalExitOwners {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        let owners = serde_json::from_str(value)
            .map_err(|_| "invalid Paper capital-exit owner configuration")?;
        Self::new(owners)
    }
    pub fn new(owners: Vec<PaperCapitalExitOwnerConfiguration>) -> Result<Self, &'static str> {
        let mut identities = BTreeSet::new();
        for owner in &owners {
            for value in [
                &owner.native_trader_id,
                &owner.native_account_id,
                &owner.native_client_id,
                &owner.native_version,
                &owner.venue,
                &owner.instrument_id,
            ] {
                domain::control::text(value, 1, 200, false)
                    .map_err(|_| "invalid Paper capital-exit owner identity")?;
            }
            if !contracts::research_currency::supported(&owner.collateral_currency)
                || owner.controlled_strategy_ids.len() != 1
            {
                return Err(
                    "Paper capital exit requires one configured currency/instrument/strategy",
                );
            }
            domain::control::text(&owner.controlled_strategy_ids[0], 1, 200, false)
                .map_err(|_| "invalid Paper capital-exit strategy")?;
            if !identities.insert((
                owner.project_id,
                owner.downstream_id,
                &owner.native_trader_id,
                &owner.native_account_id,
                &owner.native_client_id,
            )) {
                return Err("ambiguous Paper capital-exit owner configuration");
            }
        }
        Ok(Self { owners })
    }
    fn matching(
        &self,
        receipt: &AccountObservationReceiptV2,
    ) -> Option<&PaperCapitalExitOwnerConfiguration> {
        let binding = &receipt.resource.observation.binding;
        if binding.environment != ForwardEnvironmentV1::Paper {
            return None;
        }
        self.owners.iter().find(|owner| {
            owner.project_id == binding.project_id
                && owner.downstream_id == receipt.resource.downstream_id
                && owner.native_trader_id == binding.native_trader_id
                && owner.native_account_id == binding.native_account_id
                && owner.native_client_id == receipt.native_client_id
                && owner.native_version == binding.native_version
        })
    }
    /// Called after original intake commits. A registration failure does not
    /// fabricate an ACK: retrying the unchanged observation replays its source,
    /// retries this idempotent registration, and never creates a fresh allowance.
    pub async fn register_observed_source(
        &self,
        store: &Store,
        actor: &Actor,
        receipt: &AccountObservationReceiptV2,
    ) -> Result<Option<Id>, StoreError> {
        let Some(owner) = self.matching(receipt) else {
            return Ok(None);
        };
        let source = receipt.resource.source_id;
        let registration = CapitalExitOwnerRegistration {
            managed_account_key: format!("paper-native:{source}"),
            owner_binding_ref: format!("nautilus-paper:{source}"),
            downstream_id: owner.downstream_id,
            environment: ForwardEnvironmentV1::Paper,
            venue: owner.venue.clone(),
            native_account_id: owner.native_account_id.clone(),
            native_trader_id: owner.native_trader_id.clone(),
            native_client_id: owner.native_client_id.clone(),
            collateral_currency: owner.collateral_currency.clone(),
            instrument_id: owner.instrument_id.clone(),
            controlled_strategy_ids: owner.controlled_strategy_ids.clone(),
        };
        store
            .register_paper_capital_exit_owner(
                actor,
                source,
                &receipt.resource.observation.binding,
                &registration,
            )
            .await?;
        Ok(Some(source))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_configuration_crosses_count_and_byte_caps_without_widening_identity() {
        let project_id = Id::new();
        let downstream_id = Id::new();
        let owners: Vec<_> = (0..257)
            .map(|index| PaperCapitalExitOwnerConfiguration {
                schema_version: SchemaV1,
                project_id,
                downstream_id,
                native_trader_id: "TRADER-001".into(),
                native_account_id: format!("POLYMARKET-{index:03}"),
                native_client_id: "POLYMARKET".into(),
                native_version: "0.63.0".into(),
                venue: "POLYMARKET".into(),
                collateral_currency: "USDC".into(),
                instrument_id: "YES.POLYMARKET".into(),
                controlled_strategy_ids: vec!["QZ-PAPER-001".into()],
            })
            .collect();
        let raw = serde_json::to_string(&owners).unwrap();
        assert!(raw.len() > 65536);
        let parsed = PaperCapitalExitOwners::parse(&raw).unwrap();
        assert_eq!(serde_json::to_value(&parsed.owners).unwrap(), serde_json::to_value(&owners).unwrap());
        let mut duplicate = owners.clone();
        duplicate.push(owners[0].clone());
        assert!(PaperCapitalExitOwners::new(duplicate).is_err());
        let mut wrong_strategy = owners.clone();
        wrong_strategy.last_mut().unwrap().controlled_strategy_ids.push("OTHER-001".into());
        assert!(PaperCapitalExitOwners::new(wrong_strategy).is_err());
        let mut wrong_currency = owners;
        wrong_currency.last_mut().unwrap().collateral_currency = "UNKNOWN".into();
        assert!(PaperCapitalExitOwners::new(wrong_currency).is_err());
    }
    #[test]
    fn paper_capital_exit_configuration_is_explicit_closed_and_disabled_by_default() {
        assert!(
            PaperCapitalExitOwners::parse("[]")
                .unwrap()
                .owners
                .is_empty()
        );
        assert!(
            PaperCapitalExitOwners::parse(
                "[{\"schema_version\":1,\"managed_account_key\":\"caller-key\"}]"
            )
            .is_err()
        );
        let owner = PaperCapitalExitOwnerConfiguration {
            schema_version: SchemaV1,
            project_id: Id::new(),
            downstream_id: Id::new(),
            native_trader_id: "TRADER-001".into(),
            native_account_id: "POLYMARKET-001".into(),
            native_client_id: "POLYMARKET".into(),
            native_version: "0.63.0".into(),
            venue: "POLYMARKET".into(),
            collateral_currency: "USDC".into(),
            instrument_id: "YES.POLYMARKET".into(),
            controlled_strategy_ids: vec!["QZ-PAPER-001".into()],
        };
        PaperCapitalExitOwners::new(vec![owner.clone()]).unwrap();
        assert!(PaperCapitalExitOwners::new(vec![owner.clone(), owner.clone()]).is_err());
        let mut bad = serde_json::to_value(&owner).unwrap();
        bad["managed_account_key"] = serde_json::json!("caller-key");
        assert!(
            PaperCapitalExitOwners::parse(&serde_json::to_string(&vec![bad]).unwrap()).is_err()
        );
    }
}
