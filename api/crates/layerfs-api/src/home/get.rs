//! `home.get`: the home's state and build, changing nothing; with
//! `--with`, the platforms this computer supports or the tree of everything
//! in the home.
use crate::calls::table::{call, Call};
use crate::calls::{Context, Form, Part, Section};
use layerfs_bridge::control::SdkValue;
use layerfs_control::home::{Build, With};
use layerfs_home::outcome::Outcome;

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

/// The nodes of `--with tree`: a PostgreSQL Store's address, the home's
/// own (`machine` null) or one machine's.
const CONNECTION: Form = Form::of(&["url"], &["machine"], &["machine", "url"]);
const STORE: Form = Form::of(
    &["store", "engine"],
    &[
        "projects",
        "connections",
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
        "connections",
        "state",
        "op",
        "blocked_by",
        "reason",
        "fix",
    ],
)
.parts(&[
    ("projects", Part::Scalars("project")),
    ("connections", Part::Rows(&CONNECTION)),
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
/// bundle and the native daemon with, and `doctor`'s `host.native_daemon`
/// the native daemon.
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
    layerfs_control::home::get(&context.home, build(), with)
}
