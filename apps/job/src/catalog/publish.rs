//! One native publication/readback boundary for prepared immutable batches.
use std::path::PathBuf;

use anyhow::{ensure, Context, Result};
use nautilus_model::data::{Data, HasTsInit, NautilusDataType};
use nautilus_persistence::{
    backend::parquet::catalog::ParquetDataCatalog, catalog::types::CatalogQuery,
};

/// Publish a homogeneous native batch, then compare the original values and
/// precision as they stream back through official decoders. The caller owns
/// source evidence and whole-dataset publication, not another Parquet format.
pub fn publish_batch(
    catalog: &ParquetDataCatalog,
    items: &[Data],
    verify_only: bool,
) -> Result<()> {
    publish_and_read_back(catalog, items, verify_only, false).map(|_| ())
}

pub(crate) fn publish_fresh_book_batch(
    catalog: &ParquetDataCatalog,
    items: &[Data],
) -> Result<String> {
    publish_and_read_back(catalog, items, false, true)?
        .context("NATIVE_PUBLICATION_BATCH_MISMATCH")
}

fn publish_and_read_back(
    catalog: &ParquetDataCatalog,
    items: &[Data],
    verify_only: bool,
    fresh_book: bool,
) -> Result<Option<String>> {
    #[cfg(test)]
    let profile_started = std::time::Instant::now();
    let first = items.first().context("EMPTY_NATIVE_BATCH")?;
    let identifier = match first {
        Data::Bar(bar) => bar.bar_type.to_string(),
        data => data.instrument_id().to_string(),
    };
    let kind = NautilusDataType::from_data(first);
    ensure!(
        items.iter().all(|data| {
            NautilusDataType::from_data(data) == kind
                && match data {
                    Data::Bar(bar) => bar.bar_type.to_string() == identifier,
                    data => data.instrument_id().to_string() == identifier,
                }
        }),
        "NATIVE_PUBLICATION_BATCH_MISMATCH"
    );
    #[cfg(test)]
    let write_started = std::time::Instant::now();
    let owned_book_key = if fresh_book {
        ensure!(
            !verify_only && !catalog.is_remote_uri() && matches!(first, Data::BookDelta(_)),
            "NATIVE_PUBLICATION_BATCH_MISMATCH"
        );
        let deltas = items
            .iter()
            .map(|data| match data {
                Data::BookDelta(delta) => *delta,
                _ => unreachable!("batch kind was checked above"),
            })
            .collect::<Vec<_>>();
        let path = catalog.write_to_parquet(&deltas, None, None, None)?;
        // Use the same object-key encoding as Nautilus' writer.
        Some(
            catalog
                .to_object_path(path.to_str().context("CATALOG_PATH_ENCODING")?)?
                .to_string(),
        )
    } else {
        if !verify_only {
            catalog.write_data_enum(items, None, None, None)?;
        }
        None
    };
    #[cfg(test)]
    profile::add(0, write_started.elapsed().as_nanos());
    let query = CatalogQuery::new(kind)
        .with_identifiers(Some(vec![identifier]))
        .with_range(Some(first.ts_init()), Some(items.last().unwrap().ts_init()));
    let batch_rows = items.len().saturating_add(1).min(4096);
    let mut session = match &owned_book_key {
        Some(key) => super::file_batches::file_batch_session_with_paths(
            query,
            None,
            batch_rows,
            vec![PathBuf::from(catalog.reconstruct_full_uri(key))],
        ),
        None => super::file_batch_session(catalog, query, None, batch_rows)?,
    };
    let mut observed = 0;
    loop {
        #[cfg(test)]
        let read_started = std::time::Instant::now();
        let batch = session.next_batch()?;
        #[cfg(test)]
        profile::add(1, read_started.elapsed().as_nanos());
        let Some(batch) = batch else { break };
        #[cfg(test)]
        let compare_started = std::time::Instant::now();
        for index in 0..batch.len() {
            let expected = items
                .get(observed)
                .context("NATIVE_RECORD_READBACK_MISMATCH")?;
            let actual = batch
                .get(index)
                .context("NATIVE_RECORD_READBACK_MISMATCH")?;
            ensure!(
                records_equal(&actual.to_owned_data(), expected)?,
                "NATIVE_RECORD_READBACK_MISMATCH"
            );
            observed += 1;
        }
        #[cfg(test)]
        profile::add(2, compare_started.elapsed().as_nanos());
    }
    ensure!(observed == items.len(), "NATIVE_RECORD_READBACK_MISMATCH");
    #[cfg(test)]
    profile::add(3, profile_started.elapsed().as_nanos());
    Ok(owned_book_key)
}

// Native BookOrder equality intentionally does not compare all serialized fields.
// Keep exact raw values AND precision, plus every remaining delta/order field.
// Other native variants retain the original JSON comparison unchanged.
fn records_equal(actual: &Data, expected: &Data) -> Result<bool> {
    #[cfg(test)]
    if profile::legacy() {
        return Ok(serde_json::to_value(actual)? == serde_json::to_value(expected)?);
    }
    if let (Data::BookDelta(a), Data::BookDelta(b)) = (actual, expected) {
        return Ok(a.instrument_id == b.instrument_id
            && a.action == b.action
            && a.order.side == b.order.side
            && a.order.price.raw() == b.order.price.raw()
            && a.order.price.precision == b.order.price.precision
            && a.order.size.raw() == b.order.size.raw()
            && a.order.size.precision == b.order.size.precision
            && a.order.order_id == b.order.order_id
            && a.flags == b.flags
            && a.sequence == b.sequence
            && a.ts_event == b.ts_event
            && a.ts_init == b.ts_init);
    }
    Ok(serde_json::to_value(actual)? == serde_json::to_value(expected)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{data::{BookOrder, OrderBookDelta}, enums::{BookAction, OrderSide}};

    #[test]
    fn exact_book_readback_rejects_each_field_mutation_like_json() {
        let original = OrderBookDelta::new("BTCUSDT.BINANCE".parse().unwrap(), BookAction::Add,
            BookOrder::new(OrderSide::Buy, "100.12".parse().unwrap(), "2.00".parse().unwrap(), 7),
            128, 42, 100.into(), 123.into());
        let mutations: &[fn(&mut OrderBookDelta)] = &[
            |d| d.instrument_id = "ETHUSDT.BINANCE".parse().unwrap(),
            |d| d.action = BookAction::Update,
            |d| d.order.side = Some(OrderSide::Sell),
            |d| d.order.side = None,
            |d| d.order.price = "100.13".parse().unwrap(),
            |d| d.order.price.precision = 3,
            |d| d.order.size = "2.01".parse().unwrap(),
            |d| d.order.size.precision = 3,
            |d| d.order.order_id += 1,
            |d| d.flags ^= 1,
            |d| d.sequence += 1,
            |d| d.ts_event = 101.into(),
            |d| d.ts_init = 124.into(),
        ];
        let expected = Data::BookDelta(original);
        assert!(records_equal(&expected, &expected).unwrap());
        for mutate in mutations {
            let mut changed = original;
            mutate(&mut changed);
            let actual = Data::BookDelta(changed);
            let legacy = serde_json::to_value(&actual).unwrap() == serde_json::to_value(&expected).unwrap();
            assert!(!legacy);
            assert_eq!(records_equal(&actual, &expected).unwrap(), legacy);
        }
        for action in [BookAction::Clear, BookAction::Delete, BookAction::Add, BookAction::Update] {
            let mut delta = original;
            delta.action = action;
            if action == BookAction::Clear { delta.order = BookOrder::default(); }
            let value = Data::BookDelta(delta);
            assert_eq!(records_equal(&value, &value).unwrap(),
                serde_json::to_value(&value).unwrap() == serde_json::to_value(&value).unwrap());
        }
    }
}

// Test-only timing, never linked into the product. Samples are diagnostic, not
// acceptance thresholds; whole-import wall time remains the primary measurement.
#[cfg(test)]
pub(crate) mod profile {
    use std::cell::Cell;
    thread_local! { static TOTALS: Cell<[u128; 4]> = const { Cell::new([0; 4]) }; }
    thread_local! { static LEGACY: Cell<bool> = const { Cell::new(false) }; }
    pub(crate) fn set_legacy(value: bool) { LEGACY.with(|legacy| legacy.set(value)); }
    pub(super) fn legacy() -> bool { LEGACY.with(Cell::get) }
    pub(super) fn add(index: usize, value: u128) {
        TOTALS.with(|totals| { let mut values = totals.get(); values[index] += value; totals.set(values); });
    }
    pub(crate) fn take() -> [u128; 4] { TOTALS.with(|totals| totals.replace([0; 4])) }
}
