//! PyO3 native binding for the LayerFS Python SDK.
#![forbid(unsafe_code)]

mod convert;
mod direct;
mod discovery;
mod dispatch;
mod project;

use pyo3::prelude::*;

#[pyfunction]
fn call<'py>(py: Python<'py>, op: String, args: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    dispatch::call(py, op, args)
}

#[pyfunction]
fn backend_call<'py>(
    py: Python<'py>,
    op: String,
    args: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    dispatch::backend_call(py, op, args)
}

#[pyfunction]
fn store_open<'py>(py: Python<'py>, args: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    dispatch::store_open(py, args)
}

#[pyfunction]
fn store_close<'py>(py: Python<'py>, args: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    dispatch::store_close(py, args)
}

#[pyfunction]
fn exec_output<'py>(py: Python<'py>, args: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    dispatch::exec_output(py, args)
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(call, m)?)?;
    m.add_function(wrap_pyfunction!(backend_call, m)?)?;
    m.add_function(wrap_pyfunction!(store_open, m)?)?;
    m.add_function(wrap_pyfunction!(store_close, m)?)?;
    m.add_function(wrap_pyfunction!(exec_output, m)?)?;
    Ok(())
}
