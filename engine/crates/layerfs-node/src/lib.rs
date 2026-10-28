//! Node entry points for the public SDK and its backend table (#428).
mod convert;
mod direct;
#[cfg(unix)]
mod discovery;
#[cfg(windows)]
#[path = "discovery_windows.rs"]
mod discovery;
mod dispatch;
#[cfg(unix)]
mod exec;
mod job;
#[cfg(unix)]
mod output;
#[cfg(unix)]
mod pool;
mod project;
mod registry;
#[cfg(unix)]
mod sandbox;
mod trace;
#[cfg(unix)]
mod workspace;

use job::Job;
use napi::{bindgen_prelude::AsyncTask, JsUnknown};
use napi_derive::napi;

/// Runs one public SDK operation once on the native worker pool.
#[napi]
pub fn call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    dispatch::call(op, args)
}

/// Fills `page` with one page of a spooled exec output.
#[napi]
pub fn exec_output(
    args: JsUnknown,
    page: napi::bindgen_prelude::Buffer,
) -> napi::Result<AsyncTask<Job>> {
    dispatch::exec_output(args, page)
}

/// One more client of the Store at `args.storePath` in this process.
#[napi]
pub fn store_open(args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    dispatch::store(args, direct::acquire)
}

/// One client less of the Store at `args.storePath`; the last one closes it.
#[napi]
pub fn store_close(args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    dispatch::store(args, direct::release)
}

/// Runs one backend or runtime operation on its separate method table.
#[napi]
pub fn backend_call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    dispatch::backend_call(op, args)
}
