//! The library's one call with JSON text on both sides, for the front ends
//! that cross a language boundary (the Node and Python bindings): arguments,
//! scope, events and the answer are JSON, bytes as `{ "base64": … }`. The
//! answer is always text: `{ value }` or `{ refused }`, the refusal as
//! every front end answers it (`{ error, message, fix }` and the fields the
//! next call needs; `detail` when the call asked for the long form).
//! Nothing is retried here.
use super::dispatch::{asks_detail, refuse_early, Client};
use super::{call_scoped, json, Family, Scope};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

/// One call's outcome, and whether it asked for the long form.
type Ran = (Outcome, bool);
use std::path::Path;

fn run(
    families: &'static [Family],
    home: Option<&Path>,
    scope: Option<&str>,
    name: &str,
    args: &str,
    events: &mut dyn FnMut(String),
    client: Client,
) -> Ran {
    let parse = |text: &str| json::parse(text).map_err(|why| refused("InvalidInput", why.0));
    let read = || -> Result<_, SdkRefusal> {
        let args = parse(args)?;
        let scope = match scope {
            Some(text) => Some(Scope::parse(&parse(text)?)?),
            None => None,
        };
        Ok((args, scope))
    };
    let (args, scope) = match read() {
        Ok(read) => read,
        Err(refusal) => return (refuse_early(families, home, name, client, refusal), false),
    };
    let detail = asks_detail(&args);
    let mut send = |event: SdkValue| events(json::print(&event));
    let outcome = call_scoped(
        families,
        home,
        scope.as_ref(),
        name,
        args,
        &mut send,
        client,
    );
    (outcome, detail)
}

/// Runs the call `name` of `families` once with JSON `args`, in `home` (else the default)
/// and under JSON `scope` (`{ folder, branch? }`); each event goes to
/// `events` as JSON text, the call logged as made by `client`. Returns the
/// answer as JSON text.
pub fn call_json(
    families: &'static [Family],
    home: Option<&Path>,
    scope: Option<&str>,
    name: &str,
    args: &str,
    events: &mut dyn FnMut(String),
    client: Client,
) -> String {
    let answer = match run(families, home, scope, name, args, events, client) {
        (Ok(value), _) => SdkValue::map([("value", value)]),
        (Err(refusal), detail) => {
            SdkValue::map([("refused", super::refusal::value(&refusal, detail))])
        }
    };
    json::print(&answer)
}
