//! The Python front end of the `layerfs` package (#533 step 16): the
//! module `layerfs._native` with [`table`], the call table the package
//! builds its ten families from; [`call`], exactly the library's one call,
//! made once with the interpreter released; [`call::close`], the top-level
//! `layerfs.close()`; and [`call::doctor`], the
//! top-level `layerfs.doctor()`. Arguments, results and events cross as
//! JSON text (`layerfs_api::call_json`).
#![forbid(unsafe_code)]

mod call;

use pyo3::prelude::*;

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(call::table, m)?)?;
    m.add_function(wrap_pyfunction!(call::call, m)?)?;
    m.add_function(wrap_pyfunction!(call::close, m)?)?;
    m.add_function(wrap_pyfunction!(call::doctor, m)?)?;
    Ok(())
}
