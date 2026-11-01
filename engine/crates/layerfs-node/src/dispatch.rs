//! Native argument conversion and the two disjoint method tables (#428),
//! which run in layerfs-api's host module (#533 step 2).
use crate::convert;
use crate::job::Job;
use layerfs_api::host::{direct, dispatch, outcome::Outcome, output, trace};
use layerfs_bridge::control::SdkValue;
use napi::{bindgen_prelude::AsyncTask, JsUnknown};

/// Runs one SDK operation; the Promise resolves `{ value }` or `{ refused }`.
pub(crate) fn call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    let mut args = convert::from_js(args)?;
    let scope = trace::take_scope(&mut args);
    Ok(AsyncTask::new(
        Job::new(move || dispatch::call(&op, args)).traced(scope),
    ))
}

/// Fills the caller's page with spooled exec output; see [`output::page`].
pub(crate) fn exec_output(
    args: JsUnknown,
    mut page: napi::bindgen_prelude::Buffer,
) -> napi::Result<AsyncTask<Job>> {
    let args = convert::from_js(args)?;
    Ok(AsyncTask::new(Job::new(move || {
        output::page(&args, &mut page)
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

/// The Store open and release entries.
pub(crate) const ACQUIRE: fn(&SdkValue) -> Outcome = direct::acquire;
/// See [`ACQUIRE`].
pub(crate) const RELEASE: fn(&SdkValue) -> Outcome = direct::release;

/// Runs one backend or runtime call through its separate method table.
pub(crate) fn backend_call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    let mut args = convert::from_js(args)?;
    let scope = trace::take_scope(&mut args);
    Ok(AsyncTask::new(
        Job::new(move || dispatch::backend(op, args)).traced(scope),
    ))
}
