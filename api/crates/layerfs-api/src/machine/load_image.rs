//! `machine.load_image`: copy an image from the host's Docker to a machine
//! without creating a Sandbox.
use crate::calls::table::Kind::{Image, Name};
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::{Context, Form, Part};
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// Its answer: the image's id on the machine, `reused` when it was there.
const FORM: Form = Form::of(
    &["image"],
    &["reused"],
    &["image", "reused", "outcome", "machine", "refs", "size"],
)
.parts(&[("refs", Part::Scalars("ref"))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "load_image",
    &[a("machine", Name), a("image", Image)],
    &[PROGRESS],
    "copy an image from the host's Docker to a machine without creating a Sandbox",
    run,
)
.form(&FORM);

/// Runs `machine.load_image`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    use super::stats::field;
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let image = args.get_text("image").unwrap_or("").to_owned();
    let (home, events) = context.parts();
    let loaded = layerfs_control::machine::load_image(home, events, &machine, &image)?;
    let reused = loaded.get_bool("reused");
    Ok(SdkValue::map([
        ("image", field(&loaded, "id")),
        ("reused", SdkValue::Bool(reused)),
        (
            "outcome",
            SdkValue::text(if reused { "reused" } else { "loaded" }),
        ),
        ("machine", SdkValue::text(machine.as_str())),
        ("refs", field(&loaded, "refs")),
        ("size", field(&loaded, "size")),
    ]))
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.load_image runs on the host",
    ))
}
