//! The library's one call with JSON text on both sides, for the front ends
//! that cross a language boundary (the Node and Python bindings): arguments,
//! scope, events and the answer are JSON, bytes as `{ "base64": … }`. The
//! answer is always text: `{ value }` or `{ refused: { code, message,
//! detail } }`. Nothing is retried here.
use super::{call_scoped, json, Family, Scope};
use crate::common::outcome::{refused, Outcome};
use layerfs_bridge::control::SdkValue;
use std::path::Path;

fn run(
    families: &'static [Family],
    home: Option<&Path>,
    scope: Option<&str>,
    name: &str,
    args: &str,
    events: &mut dyn FnMut(String),
) -> Outcome {
    let parse = |text: &str| json::parse(text).map_err(|why| refused("InvalidInput", why.0));
    let args = parse(args)?;
    let scope = match scope {
        Some(text) => Some(Scope::parse(&parse(text)?)?),
        None => None,
    };
    let mut send = |event: SdkValue| events(json::print(&event));
    call_scoped(families, home, scope.as_ref(), name, args, &mut send)
}

/// Runs the call `name` of `families` once with JSON `args`, in `home` (else the default)
/// and under JSON `scope` (`{ folder, branch? }`); each event goes to
/// `events` as JSON text. Returns the answer as JSON text.
pub fn call_json(
    families: &'static [Family],
    home: Option<&Path>,
    scope: Option<&str>,
    name: &str,
    args: &str,
    events: &mut dyn FnMut(String),
) -> String {
    let answer = match run(families, home, scope, name, args, events) {
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
    json::print(&answer)
}
