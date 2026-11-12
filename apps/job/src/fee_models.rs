//! Native fill-fee observation and independently sourced maker/taker policies.
use anyhow::Result;
use nautilus_execution::models::fee::{FeeModel, FeeModelHandle};
use nautilus_model::{
    identifiers::{ClientOrderId, InstrumentId},
    instruments::{Instrument, InstrumentAny},
    orders::{Order, OrderAny},
    types::{Money, Price, Quantity},
};
use std::{cell::RefCell, fmt, rc::Rc};
pub(crate) mod catalog_fees;
/// Original failed native fill inputs; this does not represent an OrderRejected.
#[derive(Clone, Debug)]
pub struct FillFeeFailure {
    pub reason: String,
    pub client_order_id: ClientOrderId,
    pub instrument_id: InstrumentId,
    pub quantity: Quantity,
    pub price: Price,
}

impl fmt::Display for FillFeeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.reason)
    }
}
impl std::error::Error for FillFeeFailure {}

/// One observer per run/session. Only venue handle clones share its first error.
#[derive(Clone, Debug, Default)]
pub(crate) struct FillFeeObserver(Rc<RefCell<Option<FillFeeFailure>>>);

impl FillFeeObserver {
    pub(crate) fn clear(&self) {
        self.0.borrow_mut().take();
    }

    pub(crate) fn wrap(&self, delegate: FeeModelHandle) -> FeeModelHandle {
        FeeModelHandle::new(ObservedFillFee {
            delegate,
            observer: self.clone(),
        })
    }

    pub(crate) fn failure(&self) -> Option<FillFeeFailure> {
        self.0.borrow().clone()
    }

    pub(crate) fn check(&self) -> Result<()> {
        match self.failure() {
            Some(failure) => Err(anyhow::Error::new(failure)),
            None => Ok(()),
        }
    }
}

#[derive(Debug)]
struct ObservedFillFee {
    delegate: FeeModelHandle,
    observer: FillFeeObserver,
}

impl ObservedFillFee {
    fn observe(
        &self,
        result: Result<Money>,
        order: &OrderAny,
        quantity: Quantity,
        price: Price,
        instrument: &InstrumentAny,
    ) -> Result<Money> {
        if let Err(error) = &result {
            let mut first = self.observer.0.borrow_mut();
            if first.is_none() {
                *first = Some(FillFeeFailure {
                    reason: format!("{error:#}"),
                    client_order_id: order.client_order_id(),
                    instrument_id: instrument.id(),
                    quantity,
                    price,
                });
            }
        }
        result
    }
}

impl FeeModel for ObservedFillFee {
    fn get_commission(
        &self,
        order: &OrderAny,
        quantity: Quantity,
        price: Price,
        instrument: &InstrumentAny,
    ) -> Result<Money> {
        let result = self
            .delegate
            .get_commission(order, quantity, price, instrument);
        self.observe(result, order, quantity, price, instrument)
    }

    fn get_commission_with_context(
        &self,
        order: &OrderAny,
        quantity: Quantity,
        price: Price,
        instrument: &InstrumentAny,
        underlying_px: Option<Price>,
    ) -> Result<Money> {
        let result = self.delegate.get_commission_with_context(
            order,
            quantity,
            price,
            instrument,
            underlying_px,
        );
        self.observe(result, order, quantity, price, instrument)
    }
}

#[cfg(test)]
mod reset_tests {
    use super::*;

    #[test]
    fn clearing_a_session_observer_clears_the_venue_handle_copy() {
        let observer = FillFeeObserver::default();
        let venue_copy = observer.clone();
        *venue_copy.0.borrow_mut() = Some(FillFeeFailure {
            reason: "prior-fill-error".into(),
            client_order_id: ClientOrderId::from("prior-order"),
            instrument_id: InstrumentId::from("BTC/USDT.TEST"),
            quantity: Quantity::from("1"),
            price: Price::from("1"),
        });
        assert!(observer.check().is_err());
        observer.clear();
        assert!(venue_copy.check().is_ok());
    }
}
