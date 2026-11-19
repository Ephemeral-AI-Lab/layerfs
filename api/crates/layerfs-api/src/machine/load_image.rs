//! `machine.load_image`: copy an image from the host's Docker to a machine
//! without creating a Sandbox. Progress phases: `inspect`, `lock` (once a
//! second while another copy of the image holds its lock), `export`, `send`
//! (bytes), `unpack`.
use crate::calls::table::Kind::{Image, Name};
use crate::calls::table::{a, call, Call, PROGRESS};
use crate::calls::{Context, Form, Part};
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
use layerfs_home::outcome::Outcome;

/// The most paths `dropped` shows.
#[cfg(feature = "machine")]
const SHOWN: usize = 20;

/// Its answer: the image's id on the machine, `reused` when it was there,
/// `dropped` the FIFO and device paths the copy left out.
const FORM: Form = Form::of(
    &["image"],
    &["reused", "dropped"],
    &[
        "image",
        "reused",
        "dropped",
        "outcome",
        "machine",
        "refs",
        "size",
        "dropped_count",
        "expanded_bytes",
    ],
)
.parts(&[
    ("refs", Part::Scalars("ref")),
    ("dropped", Part::Scalars("path")),
]);

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
    let loaded = layerfs_sandbox::image_load::machine_load(home, events, &machine, &image)?;
    let reused = loaded.get_bool("reused");
    // the copy's filter report, when it sent the image: the FIFOs and
    // devices it left out (init files are always left out and not listed)
    let report = |part: &str| loaded.get(part).cloned().unwrap_or_default();
    let (dropped, cut) = dropped(&report("dropped"));
    let expanded = report("sparse").get("expanded_bytes").cloned();
    Ok(SdkValue::map([
        ("image", field(&loaded, "id")),
        ("reused", SdkValue::Bool(reused)),
        ("dropped", dropped),
        (
            "outcome",
            SdkValue::text(if reused { "reused" } else { "loaded" }),
        ),
        ("machine", SdkValue::text(machine.as_str())),
        ("refs", field(&loaded, "refs")),
        ("size", field(&loaded, "size")),
        ("dropped_count", cut),
        ("expanded_bytes", expanded.unwrap_or_default()),
    ]))
}

/// The FIFO and device paths of a copy's `dropped {fifos, devices}`, the
/// first [`SHOWN`], with how many there were when that cut the list.
#[cfg(feature = "machine")]
fn dropped(report: &SdkValue) -> (SdkValue, SdkValue) {
    let paths = |key: &str| match report.get(key) {
        Some(SdkValue::List(paths)) => paths.clone(),
        _ => Vec::new(),
    };
    let all: Vec<SdkValue> = paths("fifos").into_iter().chain(paths("devices")).collect();
    let count = match all.len() > SHOWN {
        true => SdkValue::Int(i64::try_from(all.len()).unwrap_or(i64::MAX)),
        false => SdkValue::Null,
    };
    (SdkValue::List(all.into_iter().take(SHOWN).collect()), count)
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
