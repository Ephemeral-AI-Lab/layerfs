//! Node entry points for the public SDK and its backend table (#428).
mod convert;
mod dispatch;
mod job;

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
    dispatch::store(args, dispatch::ACQUIRE)
}

/// One client less of the Store at `args.storePath`; the last one closes it.
#[napi]
pub fn store_close(args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    dispatch::store(args, dispatch::RELEASE)
}

/// Runs one backend or runtime operation on its separate method table.
#[napi]
pub fn backend_call(op: String, args: JsUnknown) -> napi::Result<AsyncTask<Job>> {
    dispatch::backend_call(op, args)
}
