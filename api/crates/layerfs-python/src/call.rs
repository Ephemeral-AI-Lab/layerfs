//! `table()` and `call(name, args, home=None, scope=None, on_event=None)`.
use layerfs_api::calls;
use pyo3::prelude::*;
use std::path::PathBuf;

/// The call table as JSON text.
#[pyfunction]
pub fn table() -> String {
    calls::table_json()
}

/// Runs the call `name` once with JSON `args`, in `home` (else the
/// library's default) and under JSON `scope`. Each event is passed to
/// `on_event` as JSON text, with the interpreter held; an exception it
/// raises is reported as unraisable and the call goes on. Returns the
/// answer as JSON text: `{ value }` or `{ refused }`.
#[pyfunction]
#[pyo3(signature = (name, args, home=None, scope=None, on_event=None))]
pub fn call(
    py: Python<'_>,
    name: String,
    args: String,
    home: Option<String>,
    scope: Option<String>,
    on_event: Option<Py<PyAny>>,
) -> String {
    let home = home.map(PathBuf::from);
    py.detach(move || {
        let mut send = |event: String| {
            if let Some(function) = &on_event {
                Python::attach(|py| {
                    if let Err(error) = function.call1(py, (event,)) {
                        error.write_unraisable(py, None);
                    }
                });
            }
        };
        calls::call_json(home.as_deref(), scope.as_deref(), &name, &args, &mut send)
    })
}
