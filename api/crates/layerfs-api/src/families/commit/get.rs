//! `commit.get`: one Commit, by its id or a prefix of 8 or more characters.
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::{Name, Project};
use crate::dispatch::table::{a, call, opt, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::data::commit::{get, CommitGet};

/// `branch` is the Branch the Commit was made on; a publication of main
/// answers its `source_branch` and `source_commit`.
const FORM: Form = Form::of(
    &[],
    &["branch", "parent", "message"],
    &[
        "branch",
        "parent",
        "message",
        "commit",
        "second_parent",
        "base",
        "root",
        "via",
        "created_at",
        "merged_from",
        "displaced",
        "overwritten",
        "overwritten_count",
        "units",
        "source_branch",
        "source_commit",
        "size",
        "new_canonical_bytes",
        "new_stored_bytes",
        "new_metadata_bytes",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "commit",
    "get",
    &[a("project", Project), a("commit", Name)],
    &[opt("branch", Name)],
    "one Commit, by its id or a prefix of 8 or more characters",
    run,
)
.scoped(Rule::History)
.form(&FORM);

/// Runs `commit.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = CommitGet {
        project: text("project").unwrap_or_default(),
        commit: text("commit").unwrap_or_default(),
        branch: text("branch"),
    };
    let (home, events) = context.parts();
    get(home, events, &request)
}
