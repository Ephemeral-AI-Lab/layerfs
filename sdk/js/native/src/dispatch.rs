//! Native argument conversion and the thin bindings of the session layer.
use crate::convert;
use crate::job::Job;
use layerfs_bridge::control::SdkValue;
use layerfs_session::Outcome;
use napi::{bindgen_prelude::AsyncTask, JsUnknown};

/// Runs one SDK operation; the Promise resolves `{ value }` or `{ refused }`.
pub(crate) fn call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    let mut args = convert::from_js(args)?;
    let scope = layerfs_session::take_scope(&mut args);
    Ok(AsyncTask::new(
        Job::new(move || layerfs_session::dispatch(&op, args)).traced(scope),
    ))
}

/// Fills the caller's page with spooled exec output.
pub(crate) fn exec_output(
    args: JsUnknown,
    page: napi::bindgen_prelude::Buffer,
) -> napi::Result<AsyncTask<Job>> {
    let args = convert::from_js(args)?;
    Ok(AsyncTask::new(Job::new(move || {
        let mut page = page;
        layerfs_session::exec_output(&args, &mut page[..])
    })))
}

/// Opens or releases a Store of this process.
pub(crate) fn store(
    args: JsUnknown,
    run: fn(&SdkValue) -> Outcome,
) -> napi::Result<AsyncTask<Job>> {
    let args = convert::from_js(args)?;
    Ok(AsyncTask::new(Job::new(move || run(&args))))
}

/// Runs one internal operator call through its separate method table.
pub(crate) fn operator_call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    let mut args = convert::from_js(args)?;
    let scope = layerfs_session::take_scope(&mut args);
    Ok(AsyncTask::new(
        Job::new(move || layerfs_session::operator_call(&op, args)).traced(scope),
    ))
}
