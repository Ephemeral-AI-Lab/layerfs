//! The binding's two functions and the task that runs one call.
use layerfs_api::calls::{self, json, Scope};
use layerfs_api::host::outcome::refused;
use layerfs_bridge::control::SdkValue;
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

impl Call {
    fn run(&mut self) -> layerfs_api::host::outcome::Outcome {
        let parse = |text: &str| json::parse(text).map_err(|why| refused("InvalidInput", why.0));
        let args = parse(&self.args)?;
        let scope = match &self.scope {
            Some(text) => Some(Scope::parse(&parse(text)?)?),
            None => None,
        };
        let events = self.events.clone();
        let mut send = move |event: SdkValue| {
            if let Some(events) = &events {
                events.call(json::print(&event), ThreadsafeFunctionCallMode::Blocking);
            }
        };
        calls::call_scoped(
            self.home.as_deref(),
            scope.as_ref(),
            &self.name,
            args,
            &mut send,
        )
    }
}

impl Task for Call {
    type Output = String;
    type JsValue = String;

    fn compute(&mut self) -> napi::Result<String> {
        let answer = match self.run() {
            Ok(value) => SdkValue::map([("value", value)]),
            Err(refusal) => SdkValue::map([(
                "refused",
                SdkValue::map([
                    ("code", SdkValue::Text(refusal.code)),
                    ("message", SdkValue::Text(refusal.message)),
                    ("detail", refusal.detail),
                ]),
            )]),
        };
        // the event function is released with the call
        self.events = None;
        Ok(json::print(&answer))
    }

    fn resolve(&mut self, _env: Env, answer: String) -> napi::Result<String> {
        Ok(answer)
    }
}
