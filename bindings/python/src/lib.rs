use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::wrap_pyfunction;
use tramuri_core::{init, scan, TramuriError as CoreError};

create_exception!(_native, TramuriError, PyException);

fn map_core_error(error: CoreError) -> PyErr {
    TramuriError::new_err(error.to_string())
}

fn map_serialization_error(error: serde_json::Error) -> PyErr {
    TramuriError::new_err(format!("failed to serialize Tramuri result: {error}"))
}

#[pyfunction]
fn _init_json(path: &str) -> PyResult<String> {
    let report = init(path).map_err(map_core_error)?;
    serde_json::to_string(&report).map_err(map_serialization_error)
}

#[pyfunction]
fn _scan_json(path: &str) -> PyResult<String> {
    let report = scan(path).map_err(map_core_error)?;
    serde_json::to_string(&report).map_err(map_serialization_error)
}

#[pymodule]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(_init_json, module)?)?;
    module.add_function(wrap_pyfunction!(_scan_json, module)?)?;
    module.add("TramuriError", module.py().get_type::<TramuriError>())?;
    Ok(())
}
