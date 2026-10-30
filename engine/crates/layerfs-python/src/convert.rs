//! Python to and from SDK value conversions for LayerFS Python SDK.
use layerfs_bridge::control::SdkValue;
use pyo3::prelude::*;
use pyo3::types::{
    PyBool, PyByteArray, PyBytes, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple,
};

const MAX_DEPTH: usize = 16;
const MAX_SAFE_INT: i64 = 9_007_199_254_740_991;

pub fn from_py(obj: &Bound<'_, PyAny>) -> PyResult<SdkValue> {
    to_sdk_value(obj, 0)
}

fn to_sdk_value(obj: &Bound<'_, PyAny>, depth: usize) -> PyResult<SdkValue> {
    if depth > MAX_DEPTH {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "value nests too deeply",
        ));
    }
    if obj.is_none() {
        Ok(SdkValue::Null)
    } else if let Ok(b) = obj.cast::<PyBool>() {
        Ok(SdkValue::Bool(b.is_true()))
    } else if let Ok(i) = obj.cast::<PyInt>() {
        let n: i64 = i.extract()?;
        if n.abs() > MAX_SAFE_INT {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "numbers must be safe integers",
            ));
        }
        Ok(SdkValue::Int(n))
    } else if let Ok(f) = obj.cast::<PyFloat>() {
        let val: f64 = f.extract()?;
        if val.fract() != 0.0 || val.abs() > MAX_SAFE_INT as f64 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "numbers must be safe integers",
            ));
        }
        Ok(SdkValue::Int(val as i64))
    } else if let Ok(s) = obj.cast::<PyString>() {
        Ok(SdkValue::Text(s.to_str()?.to_string()))
    } else if let Ok(b) = obj.cast::<PyBytes>() {
        Ok(SdkValue::Bytes(b.as_bytes().to_vec()))
    } else if let Ok(b) = obj.cast::<PyByteArray>() {
        Ok(SdkValue::Bytes(b.to_vec()))
    } else if let Ok(list) = obj.cast::<PyList>() {
        let mut items = Vec::with_capacity(list.len());
        for item in list.iter() {
            items.push(to_sdk_value(&item, depth + 1)?);
        }
        Ok(SdkValue::List(items))
    } else if let Ok(tuple) = obj.cast::<PyTuple>() {
        let mut items = Vec::with_capacity(tuple.len());
        for item in tuple.iter() {
            items.push(to_sdk_value(&item, depth + 1)?);
        }
        Ok(SdkValue::List(items))
    } else if let Ok(dict) = obj.cast::<PyDict>() {
        let mut fields = Vec::with_capacity(dict.len());
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            if !v.is_none() {
                fields.push((key, to_sdk_value(&v, depth + 1)?));
            }
        }
        Ok(SdkValue::Map(fields))
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "unsupported Python type {}",
            obj.get_type().name()?
        )))
    }
}

pub fn to_py<'py>(py: Python<'py>, val: &SdkValue) -> PyResult<Bound<'py, PyAny>> {
    match val {
        SdkValue::Null => Ok(py.None().into_bound(py)),
        SdkValue::Bool(b) => Ok(PyBool::new(py, *b).to_owned().into_any()),
        SdkValue::Int(i) => Ok(i.into_pyobject(py)?.into_any()),
        SdkValue::Text(s) => Ok(PyString::new(py, s).into_any()),
        SdkValue::Bytes(b) => Ok(PyBytes::new(py, b).into_any()),
        SdkValue::List(items) => {
            let list = PyList::empty(py);
            for item in items {
                list.append(to_py(py, item)?)?;
            }
            Ok(list.into_any())
        }
        SdkValue::Map(fields) => {
            let dict = PyDict::new(py);
            for (k, v) in fields {
                dict.set_item(k, to_py(py, v)?)?;
            }
            Ok(dict.into_any())
        }
    }
}

pub fn refusal_dict<'py>(
    py: Python<'py>,
    code: &str,
    message: &str,
    detail: &SdkValue,
) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    let ref_dict = PyDict::new(py);
    ref_dict.set_item("code", code)?;
    ref_dict.set_item("message", message)?;
    ref_dict.set_item("detail", to_py(py, detail)?)?;
    dict.set_item("refused", ref_dict)?;
    Ok(dict.into_any())
}

pub fn value_dict<'py>(py: Python<'py>, val: &SdkValue) -> PyResult<Bound<'py, PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("value", to_py(py, val)?)?;
    Ok(dict.into_any())
}
