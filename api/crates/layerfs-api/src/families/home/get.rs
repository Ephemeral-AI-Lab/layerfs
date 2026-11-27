//! `home.get`: the home's state and build, changing nothing; with
//! `--with`, the platforms this computer supports or the tree of everything
//! in the home.
use crate::dispatch::table::{call, Call};
use crate::dispatch::{Context, Form, Part, Section};
use layerfs_home::reply::outcome::Outcome;
use layerfs_inspect::home::get::{Build, With};
use layerfs_protocol::sdk::SdkValue;

/// Its answer: `state` always, with `reason` and `fix` when it is broken.
const FORM: Form = Form::of(
    &["home", "state", "version"],
    &["generation", "reason", "fix"],
    &[
        "home",
        "state",
        "version",
        "generation",
        "reason",
        "fix",
        "message",
        "layout",
        "mode",
        "bundle",
        "daemon",
        "cache",
    ],
);

/// One row of `--with platforms`.
const PLATFORM: Form = Form::of(
    &["platform", "ready"],
    &["missing"],
    &[
        "platform",
        "how",
        "backend",
        "route",
        "sqlite",
        "postgresql",
        "ready",
        "missing",
        "version",
        "options",
        "description",
        "socket",
        "candidates",
    ],
);
const PLATFORMS: Form = Form::of(&["host", "items"], &[], &["host", "items"])
    .parts(&[("items", Part::Rows(&PLATFORM))]);

const STORE: Form = Form::of(
    &["store", "engine"],
    &[
        "projects",
        "machines",
        "state",
        "op",
        "blocked_by",
        "reason",
        "fix",
    ],
    &[
        "store",
        "engine",
        "projects",
        "machines",
        "state",
        "op",
        "blocked_by",
        "reason",
        "fix",
    ],
)
.parts(&[
    ("projects", Part::Scalars("project")),
    ("machines", Part::Scalars("machine")),
]);
const WORKSPACE: Form = Form::of(&["workspace", "branch"], &[], &["workspace", "branch"]);
const SANDBOX: Form = Form::of(
    &["sandbox", "project"],
    &[
        "image",
        "workspaces",
        "workspaces_not_listed",
        "state",
        "op",
        "blocked_by",
        "reason",
        "fix",
    ],
    &[
        "sandbox",
        "project",
        "image",
        "workspaces",
        "workspaces_not_listed",
        "state",
        "op",
        "blocked_by",
        "reason",
        "fix",
    ],
)
.parts(&[("workspaces", Part::Rows(&WORKSPACE))]);
const MACHINE: Form = Form::of(
    &["machine", "platform"],
    &[
        "cpus",
        "memory_bytes",
        "state",
        "op",
        "reason",
        "fix",
        "stores",
        "sandboxes",
    ],
    &[
        "machine",
        "platform",
        "cpus",
        "memory_bytes",
        "state",
        "op",
        "reason",
        "fix",
        "stores",
        "sandboxes",
    ],
)
.parts(&[
    ("stores", Part::Rows(&STORE)),
    ("sandboxes", Part::Rows(&SANDBOX)),
]);
const BUCKET: Form = Form::of(&["bucket", "provider"], &[], &["bucket", "provider"]);
const TREE: Form = Form::of(
    &["machines", "stores", "buckets"],
    &[],
    &["machines", "stores", "buckets"],
)
.parts(&[
    ("machines", Part::Rows(&MACHINE)),
    ("stores", Part::Rows(&STORE)),
    ("buckets", Part::Rows(&BUCKET)),
]);

/// Its table entry: never opens the home and writes no log line.
pub(crate) const CALL: Call = call(
    "home",
    "get",
    &[],
    &[],
    "the home's state and build; with --with, the platforms this computer supports or the tree of everything in the home",
    run,
)
.unopened()
.quiet()
.form(&FORM)
.with(&[
    Section::own("platforms", &["host", "items"]).shaped(&PLATFORMS),
    Section::own("tree", &["machines", "stores", "buckets"]).shaped(&TREE),
]);

/// The commit this library was built from (`LAYERFS_BUILD_COMMIT`), empty
/// when the build named none.
const COMMIT: &str = env!("LAYERFS_API_BUILD_COMMIT");

/// The build this library was compiled from, as `home.get` compares the
/// bundle and the host machine's daemon with, and `doctor`'s
/// `home.host_daemon` the host machine's daemon.
pub(crate) fn build() -> Build<'static> {
    Build {
        commit: Some(COMMIT).filter(|commit| !commit.is_empty()),
        dirty: env!("LAYERFS_API_BUILD_DIRTY") == "true",
    }
}

/// Runs `home.get`: the platforms are probed and the records read only
/// when their section was asked for.
fn run(context: &mut Context<'_>, _args: &SdkValue) -> Outcome {
    let with = With {
        platforms: context.with("platforms"),
        tree: context.with("tree"),
    };
    layerfs_inspect::home::get::get(&context.home, build(), with)
}
