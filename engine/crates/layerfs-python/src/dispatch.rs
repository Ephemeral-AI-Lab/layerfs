//! Dispatch and native argument decoding for PyO3 LayerFS extension.
use crate::{convert, direct};
use layerfs_bridge::control::SdkValue;
use pyo3::prelude::*;

pub fn call<'py>(
    py: Python<'py>,
    op: String,
    args: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let sdk_args = match convert::from_py(&args) {
        Ok(val) => val,
        Err(e) => {
            return convert::refusal_dict(py, "InvalidInput", &e.to_string(), &SdkValue::Null);
        }
    };
    match direct::call(&op, sdk_args) {
        Ok(val) => convert::value_dict(py, &val),
        Err(refusal) => convert::refusal_dict(py, &refusal.code, &refusal.message, &refusal.detail),
    }
}

pub fn backend_call<'py>(
    py: Python<'py>,
    op: String,
    args: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let sdk_args = match convert::from_py(&args) {
        Ok(val) => val,
        Err(e) => {
            return convert::refusal_dict(py, "InvalidInput", &e.to_string(), &SdkValue::Null);
        }
    };
    match direct::backend_call(&op, sdk_args) {
        Ok(val) => convert::value_dict(py, &val),
        Err(refusal) => convert::refusal_dict(py, &refusal.code, &refusal.message, &refusal.detail),
    }
}

pub fn store_open<'py>(py: Python<'py>, args: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let sdk_args = match convert::from_py(&args) {
        Ok(val) => val,
        Err(e) => {
            return convert::refusal_dict(py, "InvalidInput", &e.to_string(), &SdkValue::Null);
        }
    };
    match direct::store_open(&sdk_args) {
        Ok(val) => convert::value_dict(py, &val),
        Err(refusal) => convert::refusal_dict(py, &refusal.code, &refusal.message, &refusal.detail),
    }
}

pub fn store_close<'py>(py: Python<'py>, args: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    let sdk_args = match convert::from_py(&args) {
        Ok(val) => val,
        Err(e) => {
            return convert::refusal_dict(py, "InvalidInput", &e.to_string(), &SdkValue::Null);
        }
    };
    match direct::store_close(&sdk_args) {
        Ok(val) => convert::value_dict(py, &val),
        Err(refusal) => convert::refusal_dict(py, &refusal.code, &refusal.message, &refusal.detail),
    }
}

pub fn exec_output<'py>(py: Python<'py>, _args: Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    convert::refusal_dict(
        py,
        "Failed",
        "a worker exec's output arrives whole; there is no spool to read",
        &SdkValue::Null,
    )
}
