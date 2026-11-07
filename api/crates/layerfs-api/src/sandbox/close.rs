//! `sandbox.close`: stop a Sandbox.
use crate::calls::table::Kind::{Bool, Name};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::with_field;
use layerfs_home::outcome::Outcome;
use layerfs_sandbox::family::{close, CloseRequest, Closed};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "close",
    &[a("name", Name)],
    &[opt("force", Bool)],
    "stop a Sandbox",
    run,
);

/// Runs `sandbox.close`: `{ closed, ... }`, with how it ended.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = CloseRequest {
        name: args.get_text("name").unwrap_or("").to_owned(),
        force: args.get_bool("force"),
    };
    let (home, events) = context.parts();
    let name = SdkValue::text(&request.name);
    let mut fields = vec![("closed", name.clone())];
    match close(home, events, &request)? {
        Closed::Stopped(answer) => return Ok(with_field(answer, "closed", name)),
        Closed::WasGone => fields.push(("was_gone", SdkValue::Bool(true))),
        Closed::Forgotten { container, left } => {
            let remove = format!("docker rm -f {container}");
            fields.push(("forgotten", SdkValue::Bool(true)));
            fields.push(("container", SdkValue::Text(container)));
            fields.push(("remove", SdkValue::Text(remove)));
            fields.extend(left.map(|left| ("left", SdkValue::Text(left))));
        }
        Closed::Unstarted { was, left } => {
            fields.push(("was", SdkValue::text(was)));
            fields.extend(left.into_iter().map(|left| ("left", SdkValue::Text(left))));
        }
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}
