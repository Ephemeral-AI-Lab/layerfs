//! `home.log`: the calls made against this home, newest first.
use crate::calls::table::Kind::{Int, Name, Project, Text};
use crate::calls::table::{call, opt, Call};
use crate::calls::{Context, Form, Paging, Part};
use layerfs_home::logs::Filter;
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;

/// One logged call.
const ROW: Form = Form::of(
    &["call", "outcome", "logged_at"],
    &["project", "error"],
    &[
        "call",
        "outcome",
        "logged_at",
        "project",
        "error",
        "duration_ms",
        "client",
        "argv",
        "commit",
        "tag",
    ],
);
const FORM: Form = Form::of(
    &["items"],
    &["next_cursor"],
    &["items", "next_cursor", "skipped"],
)
.parts(&[("items", Part::Rows(&ROW))]);

/// Its table entry: a read a UI polls, so it writes no log line.
pub(crate) const CALL: Call = call(
    "home",
    "log",
    &[],
    &[
        opt("call", Text),
        opt("project", Project),
        opt("client", Name),
        opt("tag", Text),
        opt("outcome", Text),
        opt("since", Int),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "the calls made against this home, newest first",
    run,
)
.quiet()
.form(&FORM)
.paged(Paging::Cursor);

/// Runs `home.log`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let filter = Filter {
        call: text("call"),
        project: text("project"),
        client: text("client"),
        tag: text("tag"),
        outcome: text("outcome"),
        since: args.get_int("since"),
    };
    layerfs_home::logs::page(
        &context.home,
        &filter,
        args.get_text("cursor"),
        args.get_int("limit"),
    )
}
