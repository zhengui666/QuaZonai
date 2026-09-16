//! Apache Arrow Rust IPC, with explicit schema/provenance and native round-trip.
use anyhow::{ensure, Result};
use arrow_array::{ArrayRef, Float64Array, RecordBatch, StringArray};
use arrow_ipc::{reader::FileReader, writer::FileWriter};
use arrow_schema::{DataType, Field, Schema};
use std::{collections::HashMap, fs::OpenOptions, path::Path, sync::Arc};

pub(crate) fn write_arrow(directory: &Path, weights: &[f64]) -> Result<()> {
    ensure!(
        weights.len() == 2 && weights.iter().all(|w| w.is_finite()),
        "INVALID_NATIVE_WEIGHTS"
    );
    let schema = Arc::new(Schema::new_with_metadata(
        vec![
            Field::new("fixture_asset", DataType::Utf8, false),
            Field::new("weight", DataType::Float64, false),
            Field::new("origin", DataType::Utf8, false),
        ],
        HashMap::from([
            ("schema".into(), "quazonai.native_fixture.v1".into()),
            ("origin".into(), "FIXTURE".into()),
            ("deliverable".into(), "false".into()),
        ]),
    ));
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec!["FIXTURE_A", "FIXTURE_B"])),
        Arc::new(Float64Array::from(weights.to_vec())),
        Arc::new(StringArray::from(vec!["FIXTURE", "FIXTURE"])),
    ];
    let batch = RecordBatch::try_new(schema.clone(), columns)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("native-weights.arrow"))?;
    let mut writer = FileWriter::try_new(file, &schema)?;
    writer.write(&batch)?;
    writer.finish()?;
    writer.into_inner()?.sync_all()?;
    let mut reader = FileReader::try_new(
        std::fs::File::open(directory.join("native-weights.arrow"))?,
        None,
    )?;
    ensure!(reader.schema() == schema, "ARROW_SCHEMA_MISMATCH");
    ensure!(
        reader.next().transpose()?.as_ref() == Some(&batch),
        "ARROW_ROUNDTRIP_MISMATCH"
    );
    ensure!(reader.next().is_none(), "UNEXPECTED_ARROW_BATCH");
    Ok(())
}
