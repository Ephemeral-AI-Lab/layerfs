//! `machine.prune`: remove a Sandbox image from a machine, or every unused
//! one; on the host machine, `--retained` removes the host worker's kept
//! write outcomes once their Branches were checked.
use crate::calls::table::Kind::{Bool, Image, Name};
use crate::calls::table::{a, call, opt, Call};
#[cfg(feature = "machine")]
use crate::calls::Context;
use crate::calls::{Form, Part};
#[cfg(feature = "machine")]
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;

/// One image `--unused` could not remove, and why.
const SKIPPED: Form = Form::of(&["image", "message"], &[], &["image", "message"]);

/// Its answer: the bytes freed, and what could not be removed; the long
/// form adds what was removed.
const FORM: Form = Form::of(
    &["freed_bytes"],
    &["skipped"],
    &[
        "freed_bytes",
        "skipped",
        "outcome",
        "machine",
        "removed",
        "removed_count",
        "skipped_count",
    ],
)
.parts(&[
    ("removed", Part::Scalars("image")),
    ("skipped", Part::Rows(&SKIPPED)),
]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "prune",
    &[a("machine", Name), opt("image", Image)],
    &[opt("unused", Bool), opt("retained", Bool)],
    "remove a Sandbox image from a machine, or every unused one",
    run,
)
.form(&FORM);

/// Runs `machine.prune`: a named image a live copy holds is refused `Busy`
/// with the copy's `op`, one a Sandbox uses `InUse` (`{sandbox_count}`).
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let unused = args.get_bool("unused");
    let image = args.get_text("image");
    if args.get_bool("retained") {
        return retained(context, &machine, image.is_some() || unused);
    }
    let pruned = layerfs_sandbox::images::prune(&context.home, &machine, image, unused)?;
    let removed = pruned.get("removed").cloned().unwrap_or_default();
    let skipped: Vec<SdkValue> = match pruned.get("skipped") {
        Some(SdkValue::List(rows)) => rows
            .iter()
            .map(|row| {
                SdkValue::map([
                    ("image", row.get("id").cloned().unwrap_or_default()),
                    ("message", row.get("reason").cloned().unwrap_or_default()),
                ])
            })
            .collect(),
        _ => Vec::new(),
    };
    let (removed, removed_count) = super::stats::cut(removed);
    let (skipped, skipped_count) = layerfs_home::helpers::shown(skipped, 0, false);
    Ok(SdkValue::map([
        (
            "freed_bytes",
            SdkValue::Int(pruned.get_int("freed").unwrap_or(0)),
        ),
        ("skipped", skipped),
        ("outcome", SdkValue::text("pruned")),
        ("machine", SdkValue::text(machine.as_str())),
        ("removed", removed),
        ("removed_count", removed_count),
        ("skipped_count", skipped_count),
    ]))
}

/// `--retained`: the host machine's kept write outcomes (§8.5), alone.
#[cfg(feature = "machine")]
fn retained(context: &mut Context<'_>, machine: &MachineName, images: bool) -> Outcome {
    use layerfs_home::outcome::refused;
    if machine.as_str() != layerfs_home::names::HOST_MACHINE {
        return Err(refused(
            "InvalidInput",
            format!(
                "--retained names the host worker's kept write outcomes: only {} has them",
                layerfs_home::names::HOST_MACHINE
            ),
        ));
    }
    if images {
        return Err(refused(
            "InvalidInput",
            "--retained is given alone, without an image or --unused",
        ));
    }
    let removed = layerfs_store::host_write::prune_retained(&context.home)?;
    let removed = SdkValue::List(removed.into_iter().map(SdkValue::Text).collect());
    let (removed, removed_count) = super::stats::cut(removed);
    Ok(SdkValue::map([
        ("freed_bytes", SdkValue::Int(0)),
        ("skipped", SdkValue::List(Vec::new())),
        ("outcome", SdkValue::text("pruned")),
        ("machine", SdkValue::text(machine.as_str())),
        ("removed", removed),
        ("removed_count", removed_count),
        ("skipped_count", SdkValue::Null),
    ]))
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::calls::host_only::host_only!("prune");
