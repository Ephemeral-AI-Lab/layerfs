//! `branch.get`: one Branch and its head Commit.
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, Call};
use crate::calls::{Context, Form};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::branch::{get, BranchGet};

/// The head to pass as `--expected`; `forked_from` is the Commit the
/// Branch was forked at.
const FORM: Form = Form::of(
    &["commit"],
    &[],
    &["commit", "forked_from", "branch", "id", "base", "root"],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "get",
    &[a("project", Project), a("branch", Name)],
    &[],
    "one Branch and its head Commit",
    run,
)
.scoped(Rule::BranchGet)
.form(&FORM);

/// Runs `branch.get`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchGet {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
    };
    let (home, events) = context.parts();
    get(home, events, &request)
}
