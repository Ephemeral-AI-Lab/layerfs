//! `table()`, `call(name, args, root=None, scope=None, on_event=None)` and
//! `close()`; the doctors are calls like any other.
use pyo3::prelude::*;
use std::path::PathBuf;

/// The call table as JSON text.
#[pyfunction]
pub fn table() -> String {
    layerfs_api::table_json()
}

/// Runs the call `name` once with JSON `args`, in the home found in the
/// LayerFS root `root` (else the library's default) and under JSON `scope`. Each event is passed to
/// `on_event` as JSON text, with the interpreter held; an exception it
/// raises is reported as unraisable and the call goes on. Returns the
/// answer as JSON text: `{ value }` or `{ refused }`.
#[pyfunction]
#[pyo3(signature = (name, args, root=None, scope=None, on_event=None))]
pub fn call(
    py: Python<'_>,
    name: String,
    args: String,
    root: Option<String>,
    scope: Option<String>,
    on_event: Option<Py<PyAny>>,
) -> String {
    let root = root.map(PathBuf::from);
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
        layerfs_api::call_json(
            root.as_deref(),
            scope.as_deref(),
            &name,
            &args,
            &mut send,
            layerfs_api::Client::Python,
        )
    })
}

/// Closes every Store this process holds open with no call running, which
/// releases its leases (outside the call table), and ends its idle Sandbox
/// daemon sessions inside a short bound (layerfs-project#181), with the
/// interpreter released. Returns how many Stores it closed.
#[pyfunction]
pub fn close(py: Python<'_>) -> usize {
    py.detach(layerfs_api::close)
}
