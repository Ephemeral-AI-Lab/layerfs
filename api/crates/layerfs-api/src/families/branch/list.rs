//! `branch.list`: a project's Branches, main included, in name order.
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::guard::scope::NAMESPACE;
use crate::dispatch::table::Kind::{Int, Project, Text};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form, Paging, Part};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::branch::{list, BranchList};

/// One Branch: `commit` is its head, or the Commit it was forked at while
/// it has none of its own; main is a row.
const ROW: Form = Form::of(
    &["branch", "commit"],
    &[],
    &["branch", "commit", "forked_from", "id", "base", "root"],
);
const FORM: Form = Form::of(&["items"], &["next_cursor"], &["items", "next_cursor"])
    .parts(&[("items", Part::Rows(&ROW))]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "list",
    &[a("project", Project)],
    &[
        opt("prefix", Text),
        opt("exclude_prefix", Text),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "a project's Branches, main included, in name order",
    run,
)
.scoped(Rule::BranchList)
.form(&FORM)
.paged(Paging::Cursor);

/// Runs `branch.list`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchList {
        project: text("project").unwrap_or_default(),
        namespace: text(NAMESPACE),
        prefix: text("prefix"),
        exclude_prefix: text("exclude_prefix"),
        limit: args.get_int("limit"),
        cursor: text("cursor"),
    };
    let (home, events) = context.parts();
    list(home, events, &request)
}
