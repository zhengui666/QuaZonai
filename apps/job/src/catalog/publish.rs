//! One native publication/readback boundary for prepared immutable batches.
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
    if !verify_only {
        catalog.write_data_enum(items, None, None, None)?;
    }
    #[cfg(test)]
    profile::add(0, write_started.elapsed().as_nanos());
    let query = CatalogQuery::new(kind)
        .with_identifiers(Some(vec![identifier]))
        .with_range(Some(first.ts_init()), Some(items.last().unwrap().ts_init()));
    let mut session = super::file_batch_session(
        catalog,
        query,
        None,
        items.len().saturating_add(1).min(4096),
    )?;
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
                serde_json::to_value(actual.to_owned_data())? == serde_json::to_value(expected)?,
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
    Ok(())
}

// Test-only timing, never linked into the product. Samples are diagnostic, not
// acceptance thresholds; whole-import wall time remains the primary measurement.
#[cfg(test)]
pub(crate) mod profile {
    use std::cell::Cell;
    thread_local! { static TOTALS: Cell<[u128; 4]> = const { Cell::new([0; 4]) }; }
    pub(super) fn add(index: usize, value: u128) {
        TOTALS.with(|totals| { let mut values = totals.get(); values[index] += value; totals.set(values); });
    }
    pub(crate) fn take() -> [u128; 4] { TOTALS.with(|totals| totals.replace([0; 4])) }
}
