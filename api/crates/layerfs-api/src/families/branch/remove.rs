//! `branch.remove`: remove one Branch; its name stays taken until
//! compaction.
use crate::dispatch::table::Kind::{Name, Project};
use crate::dispatch::table::{a, call, Call};
use crate::dispatch::{Context, Form};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::data::branch::{remove, BranchRemove};

/// A soft removal, through the Branch's Store marker.
const FORM: Form = Form::of(&["ok"], &[], &["ok", "outcome", "branch", "removed_at"]);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "remove",
    &[a("project", Project), a("branch", Name)],
    &[],
    "remove one Branch; its name stays taken until compaction",
    run,
)
.form(&FORM);

/// Runs `branch.remove`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchRemove {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
    };
    let (home, events) = context.parts();
    remove(home, events, &request)
}
