//! Preserve native currency definitions: InstrumentAny serializes currency codes only.
use anyhow::{Result, ensure};
use nautilus_model::{
    enums::CurrencyType,
    instruments::{Instrument, InstrumentAny},
    types::Currency,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Write, path::Path};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definition {
    code: String,
    precision: u8,
    iso4217: u16,
    name: String,
    currency_type: CurrencyType,
}

impl From<Currency> for Definition {
    fn from(currency: Currency) -> Self {
        Self {
            code: currency.code.to_string(),
            precision: currency.precision,
            iso4217: currency.iso4217,
            name: currency.name.to_string(),
            currency_type: currency.currency_type,
        }
    }
}

pub fn definitions(instruments: &[InstrumentAny]) -> Vec<Definition> {
    instruments
        .iter()
        .flat_map(|i| {
            [
                i.base_currency(),
                Some(i.quote_currency()),
                Some(i.settlement_currency()),
            ]
        })
        .flatten()
        .map(|c| (c.code.to_string(), Definition::from(c)))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect()
}

fn register(definitions: Vec<Definition>) -> Result<()> {
    for definition in definitions {
        let currency = Currency::new_checked(
            &definition.code,
            definition.precision,
            definition.iso4217,
            &definition.name,
            definition.currency_type,
        )?;
        if let Some(existing) = Currency::try_from_str(&definition.code) {
            ensure!(
                Definition::from(existing) == definition,
                "NATIVE_CURRENCY_DEFINITION_CONFLICT"
            );
        } else {
            Currency::register(currency, false)?;
        }
    }
    Ok(())
}

/// Legacy native arrays still work for currencies already known to Nautilus.
pub fn read_instruments(path: &Path) -> Result<Vec<InstrumentAny>> {
    instruments_from_value(serde_json::from_reader(fs::File::open(path)?)?)
}

pub fn instruments_from_value(mut document: serde_json::Value) -> Result<Vec<InstrumentAny>> {
    if document.is_object() {
        register(serde_json::from_value(document["currencies"].take())?)?;
        document = document["instruments"].take();
    }
    Ok(serde_json::from_value(document)?)
}

pub fn load(root: &Path) -> Result<()> {
    let path = root.join("currencies.json");
    if path.exists() {
        register(serde_json::from_reader(fs::File::open(path)?)?)?;
    }
    Ok(())
}

pub fn write(root: &Path, instruments: &[InstrumentAny]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("currencies.json"))?;
    serde_json::to_writer_pretty(&mut file, &definitions(instruments))?;
    writeln!(file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_currency_properties_survive_registration_without_guessing() {
        let original = Currency::new_checked(
            "QZ-PORTABLE-CURRENCY",
            7,
            0,
            "Original custom denomination",
            CurrencyType::Crypto,
        )
        .unwrap();
        let definition = Definition::from(original);
        register(vec![definition.clone()]).unwrap();
        assert_eq!(
            Definition::from(Currency::try_from_str(&definition.code).unwrap()),
            definition
        );
        let mut conflicting = definition;
        conflicting.precision = 6;
        assert!(register(vec![conflicting]).is_err());
    }
}
