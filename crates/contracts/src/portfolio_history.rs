//! Exact target-history wire format, shared by the native writer and artifact adoption.
//! No optimizer, simulated account, qualification or filesystem policy lives here.
use crate::{
    science::{NativePortfolioStudyRequestV1, NativePortfolioStudyResultV1},
    DecimalValue,
};
use arrow_array::{ArrayRef, Decimal128Array, RecordBatch, StringArray, TimestampNanosecondArray};
use arrow_ipc::{reader::FileReader, writer::FileWriter};
use arrow_schema::{ArrowError, DataType, Field, Schema, TimeUnit};
use std::{
    collections::HashMap,
    io::{Cursor, Write},
    sync::Arc,
};

pub const NAME: &str = "qz.portfolio_history";
pub const MEDIA_TYPE: &str = "application/vnd.apache.arrow.file";
pub const MAX_ROWS: usize = 256 * 256;

fn invalid() -> ArrowError {
    ArrowError::InvalidArgumentError("PORTFOLIO_HISTORY_CONTRACT_INVALID".into())
}

pub fn schema() -> Arc<Schema> {
    Arc::new(Schema::new_with_metadata(
        vec![
            Field::new(
                "cutoff_ns",
                DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "asof_ns",
                DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "valid_until_ns",
                DataType::Timestamp(TimeUnit::Nanosecond, Some("UTC".into())),
                false,
            ),
            Field::new("instrument_id", DataType::Utf8, false),
            Field::new("currency", DataType::Utf8, false),
            Field::new("solver_status", DataType::Utf8, false),
            Field::new("weight", DataType::Decimal128(38, 18), true),
            Field::new("cash_weight", DataType::Decimal128(38, 18), true),
        ],
        HashMap::from([
            ("name".into(), NAME.into()),
            ("version".into(), "1".into()),
            ("semantics".into(), "SIMULATED_TARGETS".into()),
            ("weight_unit".into(), "fraction".into()),
        ]),
    ))
}

fn decimal(value: &DecimalValue) -> Result<i128, ArrowError> {
    // DecimalValue already guarantees NUMERIC(38,18); rescaling is exact, never f64.
    value
        .as_decimal()
        .with_scale(18)
        .as_bigint_and_exponent()
        .0
        .to_string()
        .parse()
        .map_err(|_| invalid())
}

pub fn batch(
    request: &NativePortfolioStudyRequestV1,
    result: &NativePortfolioStudyResultV1,
) -> Result<RecordBatch, ArrowError> {
    if !(1..=256).contains(&request.assets.len()) || !(1..=256).contains(&result.frames.len()) {
        return Err(invalid());
    }
    let mut cutoffs = Vec::new();
    let mut asofs = Vec::new();
    let mut untils = Vec::new();
    let mut instruments = Vec::new();
    let mut currencies = Vec::new();
    let mut statuses = Vec::new();
    let mut weights = Vec::new();
    let mut cash = Vec::new();
    let ttl = u64::from(request.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000;
    for frame in &result.frames {
        let cutoff = i64::try_from(frame.cutoff_ns.get()).map_err(|_| invalid())?;
        let asof =
            i64::try_from(frame.input.forecasts.decision_asof_ns.get()).map_err(|_| invalid())?;
        let until = frame
            .cutoff_ns
            .get()
            .checked_add(ttl)
            .and_then(|n| i64::try_from(n).ok())
            .ok_or_else(invalid)?;
        let status = serde_json::to_value(frame.allocation.solver_status).map_err(|_| invalid())?;
        let status = status.as_str().ok_or_else(invalid)?;
        let target_cash = frame
            .allocation
            .cash_weight
            .as_ref()
            .map(decimal)
            .transpose()?;
        if frame.allocation.targets.is_some() != target_cash.is_some()
            || frame
                .allocation
                .targets
                .as_ref()
                .is_some_and(|t| t.len() != request.assets.len())
        {
            return Err(invalid());
        }
        for (index, asset) in request.assets.iter().enumerate() {
            let weight = frame
                .allocation
                .targets
                .as_ref()
                .map(|targets| {
                    let target = &targets[index];
                    if target.instrument_id != asset.instrument_id
                        || target.currency != asset.currency
                    {
                        return Err(invalid());
                    }
                    decimal(&target.weight)
                })
                .transpose()?;
            cutoffs.push(cutoff);
            asofs.push(asof);
            untils.push(until);
            instruments.push(asset.instrument_id.as_str());
            currencies.push(asset.currency.as_str());
            statuses.push(status.to_owned());
            weights.push(weight);
            cash.push(target_cash);
        }
    }
    let columns: Vec<ArrayRef> = vec![
        Arc::new(TimestampNanosecondArray::from(cutoffs).with_timezone("UTC")),
        Arc::new(TimestampNanosecondArray::from(asofs).with_timezone("UTC")),
        Arc::new(TimestampNanosecondArray::from(untils).with_timezone("UTC")),
        Arc::new(StringArray::from(instruments)),
        Arc::new(StringArray::from(currencies)),
        Arc::new(StringArray::from(statuses)),
        Arc::new(Decimal128Array::from(weights).with_precision_and_scale(38, 18)?),
        Arc::new(Decimal128Array::from(cash).with_precision_and_scale(38, 18)?),
    ];
    RecordBatch::try_new(schema(), columns)
}

pub fn write(writer: impl Write, batch: &RecordBatch) -> Result<(), ArrowError> {
    if batch.schema() != schema() || !(1..=MAX_ROWS).contains(&batch.num_rows()) {
        return Err(invalid());
    }
    let mut writer = FileWriter::try_new(writer, &batch.schema())?;
    writer.write(batch)?;
    writer.finish()
}

pub fn read(bytes: &[u8]) -> Result<RecordBatch, ArrowError> {
    if bytes.is_empty() || bytes.len() as u64 > crate::runtime_jobs::MAX_JOB_OUTPUT_BYTES {
        return Err(invalid());
    }
    let mut reader = FileReader::try_new(Cursor::new(bytes), None)?;
    if reader.schema() != schema() || reader.num_batches() != 1 {
        return Err(invalid());
    }
    let batch = reader.next().transpose()?.ok_or_else(invalid)?;
    if !(1..=MAX_ROWS).contains(&batch.num_rows()) || reader.next().is_some() {
        return Err(invalid());
    }
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow_array::Array;

    #[test]
    fn native_ipc_preserves_decimal_nanoseconds_nulls_and_exact_schema() {
        let maximum = decimal(&"99999999999999999999.999999999999999999".parse().unwrap()).unwrap();
        assert_eq!(
            maximum,
            "99999999999999999999999999999999999999"
                .parse::<i128>()
                .unwrap()
        );
        assert_eq!(
            decimal(&"-0.000000000000000001".parse().unwrap()).unwrap(),
            -1
        );
        let time: ArrayRef =
            Arc::new(TimestampNanosecondArray::from(vec![1, 2, i64::MAX]).with_timezone("UTC"));
        let weight: ArrayRef = Arc::new(
            Decimal128Array::from(vec![Some(maximum), Some(0), None])
                .with_precision_and_scale(38, 18)
                .unwrap(),
        );
        let original = RecordBatch::try_new(
            schema(),
            vec![
                time.clone(),
                time.clone(),
                time,
                Arc::new(StringArray::from(vec!["A", "B", "C"])),
                Arc::new(StringArray::from(vec!["USD"; 3])),
                Arc::new(StringArray::from(vec!["OPTIMAL", "OPTIMAL", "INFEASIBLE"])),
                weight.clone(),
                weight,
            ],
        )
        .unwrap();
        let mut bytes = Vec::new();
        write(&mut bytes, &original).unwrap();
        let restored = read(&bytes).unwrap();
        assert_eq!(restored, original);
        let weights = restored
            .column(6)
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .unwrap();
        assert_eq!(weights.value(0), maximum);
        assert_eq!(weights.value(1), 0);
        assert!(weights.is_null(2));
        assert!(read(&bytes[..bytes.len() - 1]).is_err());
        assert!(write(Vec::new(), &original.slice(0, 0)).is_err());
        let foreign = RecordBatch::try_new(
            Arc::new(schema().as_ref().clone().with_metadata(HashMap::new())),
            original.columns().to_vec(),
        )
        .unwrap();
        let mut wrong = Vec::new();
        let mut writer = FileWriter::try_new(&mut wrong, &foreign.schema()).unwrap();
        writer.write(&foreign).unwrap();
        writer.finish().unwrap();
        drop(writer);
        assert!(read(&wrong).is_err());
        let mut extra = Vec::new();
        let mut writer = FileWriter::try_new(&mut extra, &schema()).unwrap();
        writer.write(&original).unwrap();
        writer.write(&original).unwrap();
        writer.finish().unwrap();
        drop(writer);
        assert!(read(&extra).is_err());
    }
}
