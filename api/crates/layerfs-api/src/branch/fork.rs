//! `branch.fork`: a new Branch from another Branch's head, or from a Commit.
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Name, Project};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form, Policy};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{fork, BranchFork};

/// `commit` is the Commit the new Branch starts at.
const FORM: Form = Form::of(
    &["commit"],
    &[],
    &["commit", "outcome", "branch", "base", "root"],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "fork",
    &[a("project", Project), a("name", Name), opt("from", Name)],
    &[opt("at", Name)],
    "a new Branch from another Branch's head, or from a Commit",
    run,
)
.scoped(Rule::BranchFork)
.policy(Policy::Project("project"))
.saves()
.form(&FORM)
.agent();

/// Runs `branch.fork`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchFork {
        project: text("project").unwrap_or_default(),
        name: text("name").unwrap_or_default(),
        from: text("from"),
        at: text("at"),
    };
    let (home, events) = context.parts();
    fork(home, events, &request)
}
