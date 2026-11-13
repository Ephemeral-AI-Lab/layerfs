//! The binding's functions and the task that runs one call.
use napi::bindgen_prelude::AsyncTask;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{Env, JsFunction, Status, Task};
use napi_derive::napi;
use std::path::{Path, PathBuf};

/// The call table as JSON text.
#[napi]
pub fn table() -> String {
    layerfs_api::table_json()
}

/// Runs the call `name` once with JSON `args`, in `home` (else the
/// library's default) and under `scope` (JSON `{ project, folder, branch? }`),
/// each event sent to `on_event` as JSON text. Resolves to JSON text
/// `{ events, answer }`: `events` counts the events queued to `on_event`,
/// which arrive on their own handle and may come after the resolve, so the
/// package waits for that many before it settles. `on_event` must not
/// throw: the package catches every error of its handlers.
#[napi]
pub fn call(
    name: String,
    args: String,
    home: Option<String>,
    scope: Option<String>,
    on_event: Option<JsFunction>,
) -> napi::Result<AsyncTask<Call>> {
    let events = match on_event {
        Some(function) => Some(
            function.create_threadsafe_function(0, |context: ThreadSafeCallContext<String>| {
                Ok(vec![context.value])
            })?,
        ),
        None => None,
    };
    Ok(AsyncTask::new(Call {
        name,
        args,
        home: home.map(PathBuf::from),
        scope,
        events,
    }))
}

/// Closes every Store this process holds open with no call running, which
/// releases its leases (outside the call table): what a process calls
/// before it exits. A later call opens its Store again. Answers how many
/// Stores it closed.
#[napi]
pub fn close() -> u32 {
    u32::try_from(layerfs_api::close_idle(std::time::Duration::ZERO)).unwrap_or(u32::MAX)
}

/// `layerfs doctor` (report-only, outside the call table) on `home` (else
/// the library's default). Resolves to JSON text `{ value }` or
/// `{ refused }`; it may dial running workers, so it runs off the
/// JavaScript thread.
#[napi]
pub fn doctor(home: Option<String>, options: Option<String>) -> AsyncTask<Doctor> {
    AsyncTask::new(Doctor { home, options })
}

/// One doctor report, run off the JavaScript thread.
pub struct Doctor {
    home: Option<String>,
    options: Option<String>,
}

impl Task for Doctor {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<String> {
        let home = layerfs_api::home(self.home.as_deref().map(Path::new));
        Ok(layerfs_api::doctor_options_json(
            home,
            self.options.as_deref().unwrap_or("{}"),
        ))
    }

    fn resolve(&mut self, _env: Env, answer: String) -> napi::Result<String> {
        Ok(answer)
    }
}

/// One call, run off the JavaScript thread.
pub struct Call {
    name: String,
    args: String,
    home: Option<PathBuf>,
    scope: Option<String>,
    events: Option<ThreadsafeFunction<String, ErrorStrategy::Fatal>>,
}

impl Task for Call {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<String> {
        // the event function is released with the call; events already
        // queued are still delivered
        let events = self.events.take();
        let mut queued = 0u64;
        let mut send = |event: String| {
            if let Some(events) = &events {
                if events.call(event, ThreadsafeFunctionCallMode::Blocking) == Status::Ok {
                    queued += 1;
                }
            }
        };
        let answer = layerfs_api::call_json(
            self.home.as_deref(),
            self.scope.as_deref(),
            &self.name,
            &self.args,
            &mut send,
            layerfs_api::Client::Node,
        );
        drop(events);
        Ok(format!("{{\"events\":{queued},\"answer\":{answer}}}"))
    }

    fn resolve(&mut self, _env: Env, answer: String) -> napi::Result<String> {
        Ok(answer)
    }
}
