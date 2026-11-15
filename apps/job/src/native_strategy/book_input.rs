//! Evidence checks for a cold-start, window-filtered delta stream, not a book engine.
//! F_LAST proves only an observed batch tail. It cannot recover a missing prefix.
use anyhow::{ensure, Result};
use nautilus_model::{
    data::OrderBookDelta,
    enums::{BookAction, RecordFlag},
};

#[derive(Default)]
pub(super) struct BookInputCheck {
    seen: bool,
    batch_has_clear: bool,
}

impl BookInputCheck {
    pub(super) fn observe(&mut self, delta: &OrderBookDelta) -> Result<()> {
        let clear = delta.action == BookAction::Clear;
        // Each run starts with an empty official engine. Do not silently interpret an
        // arbitrary increment (or snapshot suffix ending in F_LAST) as a complete book.
        ensure!(
            self.seen || clear,
            "NATIVE_BOOK_INITIAL_SNAPSHOT_UNPROVEN: selected deltas must start with CLEAR inside the requested window"
        );
        self.seen = true;
        self.batch_has_clear |= clear;
        ensure!(
            delta.flags & RecordFlag::F_SNAPSHOT as u8 == 0 || self.batch_has_clear,
            "NATIVE_BOOK_SNAPSHOT_PREFIX_UNPROVEN: snapshot-marked delta has no observed CLEAR in its batch"
        );
        if delta.flags & RecordFlag::F_LAST as u8 != 0 {
            self.batch_has_clear = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        data::BookOrder,
        enums::OrderSide,
        identifiers::InstrumentId,
        types::{Price, Quantity},
    };

    fn clear(last: bool) -> OrderBookDelta {
        let mut value = OrderBookDelta::clear(
            InstrumentId::from("ETHUSDT-PERP.BINANCE"),
            0,
            1.into(),
            2.into(),
        );
        if last {
            value.flags |= RecordFlag::F_LAST as u8;
        }
        value
    }
    fn row(action: BookAction, flags: u8) -> OrderBookDelta {
        OrderBookDelta::new(
            clear(false).instrument_id,
            action,
            BookOrder::new(
                OrderSide::Buy,
                Price::from("100.00"),
                Quantity::from("1.000"),
                0,
            ),
            flags,
            0,
            1.into(),
            2.into(),
        )
    }
    #[test]
    fn prefix_cut_with_last_does_not_prove_initialization() {
        for flags in [
            RecordFlag::F_LAST as u8,
            RecordFlag::F_LAST as u8 | RecordFlag::F_SNAPSHOT as u8,
        ] {
            assert!(BookInputCheck::default()
                .observe(&row(BookAction::Add, flags))
                .unwrap_err()
                .to_string()
                .contains("INITIAL_SNAPSHOT_UNPROVEN"));
        }
    }
    #[test]
    fn official_empty_snapshot_and_later_increments_are_valid() {
        let mut check = BookInputCheck::default();
        check.observe(&clear(true)).unwrap();
        for action in [BookAction::Add, BookAction::Update, BookAction::Delete] {
            check
                .observe(&row(action, RecordFlag::F_LAST as u8))
                .unwrap();
        }
        check.observe(&clear(true)).unwrap();
    }
    #[test]
    fn snapshot_can_span_pages_and_contain_one_sided_depth() {
        let mut check = BookInputCheck::default();
        check.observe(&clear(false)).unwrap();
        for _ in 0..8193 {
            check
                .observe(&row(BookAction::Add, RecordFlag::F_SNAPSHOT as u8))
                .unwrap();
        }
        check
            .observe(&row(
                BookAction::Add,
                RecordFlag::F_SNAPSHOT as u8 | RecordFlag::F_LAST as u8,
            ))
            .unwrap();
        check
            .observe(&row(BookAction::Update, RecordFlag::F_LAST as u8))
            .unwrap();
    }
    #[test]
    fn later_snapshot_suffix_is_not_an_increment() {
        let mut check = BookInputCheck::default();
        check.observe(&clear(true)).unwrap();
        assert!(check
            .observe(&row(
                BookAction::Add,
                RecordFlag::F_SNAPSHOT as u8 | RecordFlag::F_LAST as u8
            ))
            .unwrap_err()
            .to_string()
            .contains("SNAPSHOT_PREFIX_UNPROVEN"));
    }
    #[test]
    fn unflagged_clear_is_not_rejected() {
        let mut check = BookInputCheck::default();
        let mut reset = clear(true);
        reset.flags = RecordFlag::F_LAST as u8;
        check.observe(&reset).unwrap();
        check
            .observe(&row(BookAction::Add, RecordFlag::F_LAST as u8))
            .unwrap();
    }
}
