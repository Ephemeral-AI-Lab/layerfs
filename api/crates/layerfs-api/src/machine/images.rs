//! `machine.images`: the Sandbox images a machine holds, one item per image
//! with its state (spec v19: `busy` while a copy holds it, `broken`
//! `partial` when a copy stopped partway); with `--with docker`, the host's
//! Docker images it could copy.
use crate::calls::table::Kind::Name;
use crate::calls::table::{a, call, Call};
#[cfg(feature = "machine")]
use crate::calls::Context;
use crate::calls::{Form, Part, Section};
#[cfg(feature = "machine")]
use layerfs_bridge::control::SdkValue;
#[cfg(feature = "machine")]
use layerfs_home::names::MachineName;
#[cfg(feature = "machine")]
use layerfs_home::outcome::Outcome;

/// One image the machine holds, with its state.
const ITEM: Form = Form::of(
    &["image"],
    &["refs", "size", "default", "state", "op", "reason", "fix"],
    &[
        "image",
        "refs",
        "size",
        "default",
        "state",
        "op",
        "reason",
        "fix",
        "message",
        "staged_bytes",
        "architecture",
        "source",
        "sandboxes",
        "sandboxes_count",
        "last_used_at",
    ],
)
.parts(&[
    ("refs", Part::Scalars("ref")),
    ("sandboxes", Part::Scalars("sandbox")),
]);
const FORM: Form = Form::of(
    &["items"],
    &[],
    &[
        "items",
        "machine",
        "mode",
        "architecture",
        "disk_free_bytes",
    ],
)
.parts(&[("items", Part::Rows(&ITEM))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "machine",
    "images",
    &[a("machine", Name)],
    &[],
    "the Sandbox images a machine holds",
    run,
)
.form(&FORM)
.with(&[Section::own("docker", &["images", "reason", "message"])]);

/// Runs `machine.images`: the host's Docker is read only with `--with
/// docker`.
#[cfg(feature = "machine")]
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = MachineName::parse(args.get_text("machine").unwrap_or(""))?;
    let docker = context.with("docker");
    let answer = layerfs_sandbox::images::list(&context.home, &machine, docker)?;
    Ok(shown(&answer, docker))
}

/// The answer of `layerfs_sandbox::images::list` as the row names it: one
/// item per image, its state and companion from the listing.
#[cfg(feature = "machine")]
fn shown(answer: &SdkValue, docker: bool) -> SdkValue {
    use super::stats::{cut, field, rows};
    let default = answer.get_text("default");
    let items = rows(answer.get("images"), |image| {
        let id = image.get_text("id");
        let (sandboxes, sandboxes_count) = cut(field(image, "sandboxes"));
        SdkValue::map([
            ("image", field(image, "id")),
            ("refs", field(image, "refs")),
            ("size", field(image, "size")),
            ("default", SdkValue::Bool(id.is_some() && id == default)),
            ("state", field(image, "state")),
            ("op", field(image, "op")),
            ("reason", field(image, "reason")),
            ("fix", field(image, "fix")),
            ("message", field(image, "message")),
            ("staged_bytes", field(image, "staged_bytes")),
            ("architecture", field(image, "architecture")),
            ("source", field(image, "source")),
            ("sandboxes_count", sandboxes_count),
            ("sandboxes", sandboxes),
            ("last_used_at", field(image, "last_used")),
        ])
    });
    let mut fields = vec![
        ("items", items),
        ("machine", field(answer, "machine")),
        ("mode", field(answer, "mode")),
        ("architecture", field(answer, "architecture")),
        ("disk_free_bytes", field(answer, "free")),
    ];
    if docker {
        let state = field(answer, "docker");
        let images = rows(answer.get("available"), |image| {
            SdkValue::map([
                ("image", field(image, "id")),
                ("refs", field(image, "refs")),
                ("size", field(image, "size")),
                ("os", field(image, "os")),
                ("architecture", field(image, "architecture")),
                ("usable", field(image, "usable")),
                ("needed_bytes", field(image, "need")),
                ("fits", field(image, "fits")),
                ("unusable", field(image, "reason")),
            ])
        });
        let section = SdkValue::map([
            ("images", images),
            ("reason", field(&state, "reason")),
            ("message", field(&state, "message")),
        ]);
        fields.push(("docker", section));
    }
    SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
}

// without the `machine` feature (the guest worker's build) it is
// refused before any effect
crate::calls::host_only::host_only!("images");
