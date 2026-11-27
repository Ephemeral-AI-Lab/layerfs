//! `branch.publish`: a Branch's head or Commit as main's next head.
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Bool, Name, Project};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Policy};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::calls::branch::{publish, BranchPublish};

/// `commit` is main's new head; `previous` is main's head before, when
/// that was not the Branch's base.
const FORM: Form = Form::of(
    &["commit"],
    &["unchanged", "previous"],
    &[
        "commit",
        "unchanged",
        "previous",
        "outcome",
        "branch",
        "source_commit",
        "base",
        "root",
        "size",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "publish",
    &[a("project", Project), a("branch", Name)],
    &[opt("at", Name), opt("expected", Name), opt("force", Bool)],
    "a Branch's head or Commit as main's next head, refusing a stale base",
    run,
)
.scoped(Rule::BranchPublish)
.policy(Policy::Project("project"))
.saves()
.form(&FORM);

/// Runs `branch.publish`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchPublish {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        at: text("at"),
        expected: text("expected"),
        force: args.get("force").map(|_| args.get_bool("force")),
    };
    let (home, events) = context.parts();
    publish(home, events, &request)
}
