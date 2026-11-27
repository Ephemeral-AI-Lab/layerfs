//! `machine.load_image`: copy an image from the host's Docker to a machine
//! without creating a Sandbox. Progress phases: `inspect`, `lock` (once a
//! second while another copy of the image holds its lock), `export`, `send`
//! (bytes), `unpack`.
use crate::dispatch::table::Kind::{Image, Name};
use crate::dispatch::table::{a, call, Call, PROGRESS};
#[cfg(feature = "machine")]
use crate::dispatch::Context;
use crate::dispatch::{Form, Part};
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;
#[cfg(feature = "machine")]
use layerfs_protocol::sdk::SdkValue;

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
    use super::status::stats::field;
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
/// first [`layerfs_home::helpers::SHOWN`], with how many there were when
/// that cut the list.
#[cfg(feature = "machine")]
fn dropped(report: &SdkValue) -> (SdkValue, SdkValue) {
    let paths = |key: &str| match report.get(key) {
        Some(SdkValue::List(paths)) => paths.clone(),
        _ => Vec::new(),
    };
    let all: Vec<SdkValue> = paths("fifos").into_iter().chain(paths("devices")).collect();
    layerfs_home::helpers::shown(all, 0, false)
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::dispatch::guard::host_only::host_only!("load_image");
