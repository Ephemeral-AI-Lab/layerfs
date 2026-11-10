//! `sandbox.create`: start a named Sandbox of a project on a machine (default: its sqlite Store's, or native).
use crate::calls::table::Kind::{Json, Name, Project, Text};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::with_field;
use layerfs_home::outcome::{Outcome, View};
use layerfs_sandbox::family::{create, CreateRequest};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "sandbox",
    "create",
    &[a("project", Project), a("name", Name), opt("machine", Name)],
    &[opt("image", Json), opt("image_archive", Text), PROGRESS],
    "start a named Sandbox of a project on a machine (default: its sqlite Store's, or native)",
    run,
);

/// Runs `sandbox.create`: the Sandbox's record, with why its forward did
/// not open (`forward_failed`) when it did not.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let (image, image_archive) =
        CreateRequest::image_choice(args.get("image"), args.get_text("image_archive"))?;
    let request = CreateRequest {
        project: text("project").unwrap_or_default(),
        name: text("name").unwrap_or_default(),
        machine: text("machine"),
        image,
        image_archive,
    };
    let (home, events) = context.parts();
    let created = create(home, events, &request)?;
    let shown = created.record.to_sdk(View::Full);
    Ok(match created.forward_failed {
        Some(why) => with_field(shown, "forward_failed", SdkValue::Text(why)),
        None => shown,
    })
}
