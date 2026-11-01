//! The Python front end of the `layerfs` package (#533 step 16): the
//! module `layerfs._native` with [`table`], the call table the package
//! builds its nine families from, and [`call`], exactly the library's one
//! call, made once with the interpreter released. Arguments, results and
//! events cross as JSON text (`calls::call_json`).
#![forbid(unsafe_code)]

mod call;

use pyo3::prelude::*;

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(call::table, m)?)?;
    m.add_function(wrap_pyfunction!(call::call, m)?)?;
    Ok(())
}
