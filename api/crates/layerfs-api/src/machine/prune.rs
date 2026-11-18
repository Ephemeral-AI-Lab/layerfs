//! `machine.prune`: remove a Sandbox image from a machine, or every unused one.
use crate::calls::table::Kind::{Bool, Image, Name};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Part};
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// The most ids `removed` shows.
#[cfg(feature = "machine")]
const SHOWN: usize = 20;

/// Its answer: the bytes freed; the long form adds what was removed.
const FORM: Form = Form::of(
    &["freed_bytes"],
    &[],
    &[
        "freed_bytes",
        "outcome",
        "machine",
        "removed",
        "removed_count",
    ],
)
.parts(&[("removed", Part::Scalars("image"))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "prune",
    &[a("machine", Name), opt("image", Image)],
    &[opt("unused", Bool)],
    "remove a Sandbox image from a machine, or every unused one",
    run,
)
.form(&FORM);

/// Runs `machine.prune`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    use super::stats::count;
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let unused = args.get_bool("unused");
    let image = args.get_text("image");
    let pruned = layerfs_control::machine::prune(&context.home, &machine, image, unused)?;
    let removed = pruned.get("removed").cloned().unwrap_or_default();
    let shown = match &removed {
        SdkValue::List(ids) => SdkValue::List(ids.iter().take(SHOWN).cloned().collect()),
        _ => SdkValue::List(Vec::new()),
    };
    Ok(SdkValue::map([
        (
            "freed_bytes",
            SdkValue::Int(pruned.get_int("freed").unwrap_or(0)),
        ),
        ("outcome", SdkValue::text("pruned")),
        ("machine", SdkValue::text(machine.as_str())),
        ("removed", shown),
        ("removed_count", count(&removed)),
    ]))
}

/// Without the `machine` feature (the guest worker's build) it is
/// refused before any effect.
#[cfg(not(feature = "machine"))]
fn run(_context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    Err(layerfs_home::outcome::refused(
        "Unsupported",
        "machine.prune runs on the host",
    ))
}
