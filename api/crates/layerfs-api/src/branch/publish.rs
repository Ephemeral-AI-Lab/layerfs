//! `branch.publish`: a Branch's head or Commit as main's next Layer.
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Bool, Name, Project};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::Context;
use crate::calls::Policy;
use crate::calls::Saves;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::Outcome;
use layerfs_store::calls::branch::{publish, BranchPublish};

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "publish",
    &[
        a("project", Project),
        a("branch", Name),
        opt("commit", Name),
    ],
    &[
        opt("expected_base", Name),
        opt("expected_head", Name),
        opt("force", Bool),
    ],
    "a Branch's head or Commit as main's next Layer, refusing a stale base",
    run,
)
.scoped(Rule::BranchPublish)
.policy(Policy::Project("project"))
.saves(Saves::Publish);

/// Runs `branch.publish`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchPublish {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        commit: text("commit"),
        expected_base: text("expected_base"),
        expected_head: text("expected_head"),
        force: args.get("force").map(|_| args.get_bool("force")),
    };
    let (home, events) = context.parts();
    publish(home, events, &request)
}
