//! `branch.merge`: merge one Branch into another, or into a new Branch.
use crate::dispatch::guard::scope::Rule;
use crate::dispatch::table::Kind::{Json, Name, Project};
use crate::dispatch::table::{a, call, opt, Call, MESSAGE};
use crate::dispatch::{Context, Form, Policy};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::data::branch::{merge, BranchMerge};

/// `parent` and `second_parent` are the two heads merged; `overwritten`
/// lists the paths a newer-write-wins rule decided.
const FORM: Form = Form::of(
    &["commit"],
    &[
        "unchanged",
        "fast_forward",
        "overwritten",
        "overwritten_count",
    ],
    &[
        "commit",
        "unchanged",
        "fast_forward",
        "overwritten",
        "overwritten_count",
        "outcome",
        "branch",
        "parent",
        "second_parent",
        "merge_base",
        "root",
        "units",
        "units_count",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "branch",
    "merge",
    &[a("project", Project), a("branch", Name), a("from", Name)],
    &[
        opt("into", Name),
        opt("units", Json).redact(),
        opt("expected", Name),
        MESSAGE,
    ],
    "merge one Branch into another, or into a new Branch",
    run,
)
.scoped(Rule::BranchMerge)
.policy(Policy::Project("project"))
.saves()
.form(&FORM);

/// Runs `branch.merge`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = BranchMerge {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        from: text("from").unwrap_or_default(),
        into: text("into"),
        units: args.get("units").cloned(),
        expected: text("expected"),
        message: text("message"),
    };
    let (home, events) = context.parts();
    merge(home, events, &request)
}
