//! Frozen venue policy is independent of request fee hints and native instruments.
//! The native model owns commission arithmetic and the engine books its Money.
use crate::catalog::NativeMarketData;
use anyhow::{ensure, Context, Result};
use contracts::{
    catalogs::{FrozenMakerTakerFeesV1, MAKER_TAKER_FEES_FILE},
    portfolio::NativeModelRefV1,
    science::{NativeBarSelectionV1, NativeSimulationSettingsV1},
};
use nautilus_common::clock::Clock;
use nautilus_execution::models::fee::{FeeModel, FeeModelHandle, MakerTakerFeeModel};
use nautilus_model::{
    fees::MakerTakerFeeRates,
    identifiers::InstrumentId,
    instruments::{Instrument, InstrumentAny},
    orders::{Order, OrderAny},
    types::{Money, Price, Quantity},
};
use rust_decimal::Decimal;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
    rc::Rc,
};

const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn load(
    root: &Path,
    selection: &NativeBarSelectionV1,
) -> Result<Option<FrozenMakerTakerFeesV1>> {
    match load_file(&root.join(MAKER_TAKER_FEES_FILE), selection) {
        Ok(source) => Ok(Some(source)),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// Read an explicit independent operator input without changing its original file.
pub(crate) fn load_file(
    path: &Path,
    selection: &NativeBarSelectionV1,
) -> Result<FrozenMakerTakerFeesV1> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) => return Err(error.into()),
    };
    ensure!(file.metadata()?.is_file(), "MAKER_TAKER_FEE_SOURCE_INVALID");
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_SOURCE_BYTES,
        "MAKER_TAKER_FEE_SOURCE_TOO_LARGE"
    );
    let source = serde_json::from_slice(&bytes).context("MAKER_TAKER_FEE_SOURCE_INVALID")?;
    domain::catalogs::maker_taker_fee_selection(&source, selection)?;
    Ok(source)
}

pub(crate) fn validate_source_market(
    source: &FrozenMakerTakerFeesV1,
    market: &NativeMarketData,
) -> Result<()> {
    // Verify native identifiers and exact Decimal representability before
    // publishing a policy the official model could not install later.
    native_model(source)?;
    let rates: BTreeSet<_> = source
        .rates
        .iter()
        .map(|rate| rate.instrument_id.as_str())
        .collect();
    for series in &market.series {
        let instrument = &series.instrument;
        ensure!(
            matches!(
                instrument,
                InstrumentAny::CurrencyPair(_) | InstrumentAny::Equity(_)
            ) && instrument.venue().as_str() == source.venue
                && instrument.quote_currency().code.as_str() == source.currency
                && instrument.quote_currency().precision == source.currency_precision
                && rates.contains(instrument.id().to_string().as_str()),
            "MAKER_TAKER_FEE_SOURCE_MARKET_MISMATCH"
        );
    }
    Ok(())
}

pub(crate) fn write(root: &Path, source: &FrozenMakerTakerFeesV1) -> Result<()> {
    domain::catalogs::maker_taker_fee_source(source)?;
    let bytes = serde_json::to_vec(source)?;
    ensure!(
        bytes.len() as u64 <= MAX_SOURCE_BYTES,
        "MAKER_TAKER_FEE_SOURCE_TOO_LARGE"
    );
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(MAKER_TAKER_FEES_FILE))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(crate) fn validate_market(
    market: &NativeMarketData,
    settings: &NativeSimulationSettingsV1,
) -> Result<()> {
    ensure!(
        matches!(
            settings.fee_model,
            NativeModelRefV1::NautilusMakerTaker { .. }
        ),
        "MAKER_TAKER_FEE_MODEL_REQUIRED"
    );
    let source = market
        .execution_fees
        .as_ref()
        .context("MAKER_TAKER_FEE_SOURCE_REQUIRED")?;
    let selection = market
        .fee_selection
        .as_ref()
        .context("MAKER_TAKER_FEE_SELECTION_REQUIRED")?;
    domain::catalogs::maker_taker_fee_selection(source, selection)?;
    ensure!(
        source.currency == settings.base_currency
            && !market.series.is_empty()
            && settings.fee_rates.len() == market.series.len(),
        "SIMULATION_FEE_SOURCE_MISMATCH"
    );
    let originals: BTreeMap<_, _> = source
        .rates
        .iter()
        .map(|rate| (rate.instrument_id.as_str(), rate))
        .collect();
    let declarations: BTreeMap<_, _> = settings
        .fee_rates
        .iter()
        .map(|rate| (rate.instrument_id.as_str(), rate))
        .collect();
    ensure!(
        declarations.len() == settings.fee_rates.len(),
        "SIMULATION_DUPLICATE_FEE"
    );
    let mut selected = BTreeSet::new();
    for series in &market.series {
        let instrument = &series.instrument;
        let id = instrument.id().to_string();
        ensure!(
            selected.insert(id.clone())
                && instrument.venue().as_str() == source.venue
                && instrument.quote_currency().code.as_str() == source.currency
                && instrument.quote_currency().precision == source.currency_precision,
            "SIMULATION_FEE_SOURCE_SCOPE_MISMATCH"
        );
        let original = originals
            .get(id.as_str())
            .context("SIMULATION_FEE_SOURCE_MISSING")?;
        let declared = declarations
            .get(id.as_str())
            .context("SIMULATION_FEE_MISSING")?;
        ensure!(
            original.maker == declared.maker && original.taker == declared.taker,
            "SIMULATION_FEE_SOURCE_MISMATCH"
        );
    }
    Ok(())
}

/// Rates come only from the validated frozen source; no implicit zero default.
/// Nautilus 0.67's official commission path uses Money::from_decimal, rounding
/// each fill to currency precision with half-even ties. Its exact Money storage
/// APIs do not change that fee-model policy; no alternate rounding is installed.
pub(crate) fn native_model(source: &FrozenMakerTakerFeesV1) -> Result<MakerTakerFeeModel> {
    domain::catalogs::maker_taker_fee_source(source)?;
    let exact = |value: &contracts::DecimalValue| {
        Decimal::from_str_exact(&value.as_decimal().to_plain_string())
            .context("MAKER_TAKER_FEE_RATE_OUT_OF_NATIVE_RANGE")
    };
    let first = source
        .rates
        .first()
        .context("MAKER_TAKER_FEE_SOURCE_REQUIRED")?;
    let mut model = MakerTakerFeeModel::new(exact(&first.maker)?, exact(&first.taker)?);
    for rate in &source.rates {
        model.set_override(
            InstrumentId::from_as_ref(&rate.instrument_id)?,
            MakerTakerFeeRates::new(exact(&rate.maker)?, exact(&rate.taker)?),
        );
    }
    Ok(model)
}

#[derive(Debug)]
struct FrozenFeeModel {
    model: MakerTakerFeeModel,
    source: FrozenMakerTakerFeesV1,
    selected: BTreeSet<InstrumentId>,
    clock: Rc<RefCell<dyn Clock>>,
}

impl FeeModel for FrozenFeeModel {
    fn get_commission(
        &self,
        order: &OrderAny,
        quantity: Quantity,
        price: Price,
        instrument: &InstrumentAny,
    ) -> Result<Money> {
        let now = self.clock.borrow().timestamp_ns().as_u64();
        ensure!(
            now >= self.source.valid_from_ns.get()
                && now < self.source.valid_until_ns.get()
                && now >= self.source.available_at_ns.get(),
            "MAKER_TAKER_FEE_FILL_OUTSIDE_SOURCE_WINDOW"
        );
        ensure!(
            order.instrument_id() == instrument.id()
                && self.selected.contains(&instrument.id())
                && instrument.venue().as_str() == self.source.venue
                && instrument.quote_currency().code.as_str() == self.source.currency
                && instrument.quote_currency().precision == self.source.currency_precision,
            "MAKER_TAKER_FEE_FILL_SOURCE_MISMATCH"
        );
        self.model
            .get_commission(order, quantity, price, instrument)
    }
}

pub(crate) fn fee_model(
    market: &NativeMarketData,
    settings: &NativeSimulationSettingsV1,
    clock: Rc<RefCell<dyn Clock>>,
) -> Result<FeeModelHandle> {
    validate_market(market, settings)?;
    let source = market
        .execution_fees
        .as_ref()
        .context("MAKER_TAKER_FEE_SOURCE_REQUIRED")?;
    Ok(FeeModelHandle::new(FrozenFeeModel {
        model: native_model(source)?,
        source: source.clone(),
        selected: market
            .series
            .iter()
            .map(|series| series.instrument.id())
            .collect(),
        clock,
    }))
}

#[cfg(test)]
mod tests;
