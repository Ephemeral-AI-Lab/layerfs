//! The one answer of the six doctors (call surface 2.0, `calls.json`
//! `doctor`): `{"ok":true}` alone when every check was made and nothing
//! was found, else `findings` and `not_checked`; the long form adds
//! `status`, `counts`, `observed_at` and `tree`, and its `ok` is false
//! whenever anything was found or could not be checked. A finding and a
//! `not_checked` entry name their thing by its own word. A doctor exits 0
//! once its report is made; only a thing it cannot find is a refusal.
use crate::dispatch::{Context, Form, Part, THING};
use layerfs_home::outcome::Outcome;
use layerfs_inspect::{Kind, Options, Thing};

/// The words a finding or a `not_checked` entry names its thing by.
const THINGS: &[&str] = &[
    "home",
    "machine",
    "store",
    "project",
    "sandbox",
    "workspace",
    "bucket",
];

/// One finding.
const FINDING: Form = Form::of(
    &[THING, "check", "severity", "message"],
    &["fix", "from"],
    &[
        THING, "check", "severity", "message", "fix", "from", "evidence",
    ],
)
.things(THINGS);

/// One check that could not be made: `blocked_by` or `reason`, never both.
const NOT_CHECKED: Form = Form::of(
    &[THING],
    &["check", "blocked_by", "reason"],
    &[THING, "check", "blocked_by", "reason"],
)
.things(THINGS);

/// The answer every doctor declares. `tree` is the long form's alone and
/// is answered as the report makes it: the doctor's own node with one list
/// per kind of thing on it.
pub(crate) const FORM: Form = Form::of(
    &[],
    &["ok", "findings", "not_checked"],
    &[
        "ok",
        "findings",
        "not_checked",
        "status",
        "counts",
        "observed_at",
        "tree",
    ],
)
.parts(&[
    ("findings", Part::Rows(&FINDING)),
    ("not_checked", Part::Rows(&NOT_CHECKED)),
]);

/// Runs the doctor of the `kind` thing named `name` with `options`: its
/// long form, which the dispatcher shapes. Each finished thing is one
/// progress event on the call's stream, except `backup doctor`'s.
pub(crate) fn run(context: &mut Context<'_>, kind: Kind, name: &str, options: Options) -> Outcome {
    let (home, events) = context.parts();
    let options = Options {
        build: crate::families::home::build(),
        ..options
    };
    // `backup doctor` declares no progress (its row's `progress` is false)
    let thing = Thing::new(kind, name);
    let report = if kind == Kind::Bucket {
        let mut quiet = |_| {};
        let mut silent = layerfs_home::events::Events::new(&mut quiet);
        layerfs_inspect::run(home, thing, &options, &mut silent)?
    } else {
        layerfs_inspect::run(home, thing, &options, events)?
    };
    Ok(report.to_sdk())
}
