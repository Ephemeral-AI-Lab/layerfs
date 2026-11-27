//! `backup.restore`: restore a Store file from a bucket as a sqlite Store
//! on this computer.
use crate::dispatch::table::Kind::Name;
use crate::dispatch::table::{a, call, Call, PROGRESS};
use crate::dispatch::{Context, Form, Part};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::backup::bucket::connect::BackupRestore;

/// Its answer: the projects found in it, or `ok` for an empty Store.
const FORM: Form = Form::of(
    &[],
    &["ok", "projects", "projects_count"],
    &[
        "ok",
        "projects",
        "projects_count",
        "outcome",
        "store",
        "bucket",
        "file",
        "size",
        "sha256",
        "heads_sha256",
    ],
)
.parts(&[("projects", Part::Scalars("project"))]);

/// Its table entry: source first, the bucket, its file, then the Store.
pub(crate) const CALL: Call = call(
    "backup",
    "restore",
    &[a("bucket", Name), a("file", Name), a("store", Name)],
    &[PROGRESS],
    "restore a Store file from a bucket as a sqlite Store on this computer",
    run,
)
.form(&FORM);

/// Runs `backup.restore`: `{ phase, done, total }` events, phases `check`,
/// `download`, `verify` and `install`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BackupRestore {
        store: text("store").unwrap_or_default(),
        bucket: text("bucket").unwrap_or_default(),
        file: text("file"),
    };
    let (home, events) = context.parts();
    layerfs_store::backup::jobs::restore::restore(
        home,
        events,
        &request,
        &layerfs_sandbox::users::STORE,
    )
}
