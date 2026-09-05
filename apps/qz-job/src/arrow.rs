//! Native Arrow IPC write and read-back compatibility verification.
use std::path::Path;

use pyo3::exceptions::PyAssertionError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub(crate) fn write_arrow(py: Python<'_>, directory: &Path, weights: &[f64]) -> PyResult<()> {
    eprintln!("native-probe: Arrow IPC round trip");
    let arrow = py.import("pyarrow")?;
    let columns = PyDict::new(py);
    columns.set_item("fixture_asset", vec!["FIXTURE_A", "FIXTURE_B"])?;
    columns.set_item("weight", weights.to_vec())?;
    columns.set_item("origin", vec!["FIXTURE", "FIXTURE"])?;
    let table = arrow
        .getattr("Table")?
        .call_method1("from_pydict", (columns,))?;
    let path = directory.join("native-weights.arrow");
    let path = path
        .to_str()
        .ok_or_else(|| PyAssertionError::new_err("INVALID_OUTPUT_PATH"))?;
    let sink = arrow.getattr("OSFile")?.call1((path, "wb"))?;
    let ipc = py.import("pyarrow.ipc")?;
    let result = (|| {
        let writer = ipc
            .getattr("new_file")?
            .call1((&sink, table.getattr("schema")?))?;
        let write_result = writer.call_method1("write_table", (&table,));
        let close_result = writer.call_method0("close");
        write_result?;
        close_result?;
        PyResult::Ok(())
    })();
    let closed = sink.call_method0("close");
    result?;
    closed?;
    let reader = ipc.getattr("open_file")?.call1((path,))?;
    let roundtrip = reader.call_method0("read_all")?;
    if !table
        .call_method1("equals", (&roundtrip,))?
        .extract::<bool>()?
    {
        return Err(PyAssertionError::new_err("ARROW_ROUNDTRIP_MISMATCH"));
    }
    Ok(())
}
