//! The binding's two functions and the task that runs one call.
use layerfs_api::calls;
use napi::bindgen_prelude::AsyncTask;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{Env, JsFunction, Task};
use napi_derive::napi;
use std::path::PathBuf;

/// The call table as JSON text.
#[napi]
pub fn table() -> String {
    calls::table_json()
}

/// Runs the call `name` once with JSON `args`, in `home` (else the
/// library's default) and under `scope` (JSON `{ folder, branch? }`), each
/// event sent to `on_event` as JSON text.
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
        let events = self.events.clone();
        let mut send = move |event: String| {
            if let Some(events) = &events {
                events.call(event, ThreadsafeFunctionCallMode::Blocking);
            }
        };
        let answer = calls::call_json(
            self.home.as_deref(),
            self.scope.as_deref(),
            &self.name,
            &self.args,
            &mut send,
        );
        // the event function is released with the call
        self.events = None;
        Ok(answer)
    }

    fn resolve(&mut self, _env: Env, answer: String) -> napi::Result<String> {
        Ok(answer)
    }
}
