//! Local native files, fixed Arrow batches, and the official typed decoders.
use anyhow::{ensure, Result};
use arrow_native::{Array, BooleanArray, RecordBatch, TimestampNanosecondArray};
use nautilus_model::data::{
    Bar, DataBatch, FundingRateUpdate, IndexPriceUpdate, InstrumentClose, MarkPriceUpdate,
    NautilusDataType, OrderBookDelta, QuoteTick, TradeTick,
};
use nautilus_persistence::{
    backend::parquet::catalog::ParquetDataCatalog,
    catalog::types::{CatalogAsOf, CatalogQuery},
};
use nautilus_serialization::arrow::{
    is_nautilus_legacy_schema, timestamp_data_type, DecodeFromRecordBatch,
};
use parquet::arrow::{
    arrow_reader::{
        ArrowPredicateFn, ParquetRecordBatchReader, ParquetRecordBatchReaderBuilder, RowFilter,
    },
    ProjectionMask,
};
use std::{collections::HashMap, fs::File, ops::Range, path::PathBuf};

/// Fixed-size native batches in catalog file order, retaining each file's encoding.
/// Equal timestamps do not extend a batch. Callers enforce their aggregate row
/// budget before retaining rows and, where needed, stably sort the bounded result.
/// Parquet pages and metadata still need the process's external memory limit.
pub struct FileBatchSession {
    paths: std::vec::IntoIter<PathBuf>,
    reader: Option<ParquetRecordBatchReader>,
    metadata: HashMap<String, String>,
    query: CatalogQuery,
    event_range: Option<Range<u64>>,
    batch_rows: usize,
}

/// Uses public catalog file selection and public Parquet/serialization APIs.
/// Arbitrary SQL, historical catalog snapshots and remote stores are not part of
/// this immutable local runtime path. Event and availability bounds stay distinct.
pub fn file_batch_session(
    catalog: &ParquetDataCatalog,
    query: CatalogQuery,
    event_range: Option<Range<u64>>,
    batch_rows: usize,
) -> Result<FileBatchSession> {
    ensure!(
        !catalog.is_remote_uri()
            && query.as_of == CatalogAsOf::Latest
            && query.where_clause.is_none()
            && query.params.is_none()
            && query.instrument_type.is_none(),
        "CATALOG_TYPED_QUERY_UNSUPPORTED"
    );
    ensure!(
        matches!(
            query.data_type,
            NautilusDataType::Bar
                | NautilusDataType::TradeTick
                | NautilusDataType::QuoteTick
                | NautilusDataType::OrderBookDelta
                | NautilusDataType::FundingRateUpdate
                | NautilusDataType::MarkPriceUpdate
                | NautilusDataType::IndexPriceUpdate
                | NautilusDataType::InstrumentClose
        ),
        "CATALOG_NATIVE_TYPE_UNSUPPORTED"
    );
    let paths = catalog
        .query_files(
            &query.data_type.clone().into(),
            query.identifiers.clone(),
            query.start,
            query.end,
        )?
        .into_iter()
        .map(|path| PathBuf::from(catalog.reconstruct_full_uri(&path)))
        .collect::<Vec<_>>()
        .into_iter();
    Ok(FileBatchSession {
        paths,
        reader: None,
        metadata: HashMap::new(),
        query,
        event_range,
        batch_rows: batch_rows.clamp(1, 4096),
    })
}

impl FileBatchSession {
    pub fn next_batch(&mut self) -> Result<Option<DataBatch>> {
        loop {
            if let Some(batch) = self.reader.as_mut().and_then(Iterator::next) {
                let batch = batch?;
                if batch.num_rows() != 0 {
                    return self.decode(batch).map(Some);
                }
                continue;
            }
            self.reader = None;
            let Some(path) = self.paths.next() else {
                return Ok(None);
            };
            let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?;
            let schema = builder.schema();
            // Runtime files must already use the pinned native schema. Migration
            // is an explicit operator action, never a decoder compatibility path.
            ensure!(
                !is_nautilus_legacy_schema(schema)
                    && schema.field_with_name("ts_event")?.data_type() == &timestamp_data_type()
                    && schema.field_with_name("ts_init")?.data_type() == &timestamp_data_type(),
                "CATALOG_NATIVE_SCHEMA_MIGRATION_REQUIRED"
            );
            self.metadata = schema.metadata().clone();
            let start = self.query.start.map(|value| value.as_u64());
            let end = self.query.end.map(|value| value.as_u64());
            let events = self.event_range.clone();
            let projection = ProjectionMask::roots(
                builder.parquet_schema(),
                [schema.index_of("ts_event")?, schema.index_of("ts_init")?],
            );
            let builder = builder.with_batch_size(self.batch_rows);
            let builder = if start.is_some() || end.is_some() || events.is_some() {
                let predicate = ArrowPredicateFn::new(projection, move |batch| {
                    // The builder schema above establishes these concrete types.
                    let event = batch
                        .column_by_name("ts_event")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<TimestampNanosecondArray>()
                        .unwrap();
                    let available = batch
                        .column_by_name("ts_init")
                        .unwrap()
                        .as_any()
                        .downcast_ref::<TimestampNanosecondArray>()
                        .unwrap();
                    let selected = (0..batch.num_rows()).map(|row| {
                        let event_matches = events.as_ref().is_none_or(|bounds| {
                            !event.is_null(row)
                                && u64::try_from(event.value(row))
                                    .ok()
                                    .is_some_and(|value| bounds.contains(&value))
                        });
                        let available_matches = (start.is_none() && end.is_none())
                            || (!available.is_null(row)
                                && start.is_none_or(|start| {
                                    i128::from(available.value(row)) >= i128::from(start)
                                })
                                && end.is_none_or(|end| {
                                    i128::from(available.value(row)) <= i128::from(end)
                                }));
                        Some(event_matches && available_matches)
                    });
                    Ok(BooleanArray::from_iter(selected))
                });
                builder.with_row_filter(RowFilter::new(vec![Box::new(predicate)]))
            } else {
                builder
            };
            self.reader = Some(builder.build()?);
        }
    }

    fn decode(&self, batch: RecordBatch) -> Result<DataBatch> {
        macro_rules! decode {
            ($type:ty, $variant:ident) => {
                Ok(DataBatch::$variant(
                    <$type>::decode_batch(&self.metadata, batch)?.into(),
                ))
            };
        }
        match self.query.data_type {
            NautilusDataType::Bar => decode!(Bar, Bar),
            NautilusDataType::TradeTick => decode!(TradeTick, Trade),
            NautilusDataType::QuoteTick => decode!(QuoteTick, Quote),
            NautilusDataType::OrderBookDelta => decode!(OrderBookDelta, BookDelta),
            NautilusDataType::FundingRateUpdate => decode!(FundingRateUpdate, FundingRate),
            NautilusDataType::MarkPriceUpdate => decode!(MarkPriceUpdate, MarkPrice),
            NautilusDataType::IndexPriceUpdate => decode!(IndexPriceUpdate, IndexPrice),
            NautilusDataType::InstrumentClose => decode!(InstrumentClose, InstrumentClose),
            _ => anyhow::bail!("CATALOG_NATIVE_TYPE_UNSUPPORTED"),
        }
    }
}
