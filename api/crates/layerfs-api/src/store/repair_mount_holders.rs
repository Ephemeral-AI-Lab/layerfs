//! `store.repair_mount_holders`: explicit first-class repair of one Branch's
//! mount admissions, on the machine whose worker holds a SQLite Store and in
//! this process for a PostgreSQL Store. `list` and `preview` write nothing:
//! one page of holders, each with its row `state` and the `liveness`
//! admission would give it now; `settle` refuses a holder still `live`
//! unless forced; `release` retains a settled row as the durable done audit
//! and records the operator label; `audit` pages the Branch's retained rows.
//! Every call is also in the home's `trace/calls.jsonl` call log.
use crate::calls::table::{
    a, call, opt, Call,
    Kind::{Bool, Int, Name, Project, Text},
};
use crate::calls::{Context, Paging};
use layerfs_bridge::control::SdkValue;
use layerfs_control::store::RepairRequest;
use layerfs_home::outcome::Outcome;

pub(crate) const CALL: Call = call(
    "store",
    "repair_mount_holders",
    &[a("project", Project), a("branch", Name), a("action", Name)],
    &[
        opt("op", Text),
        opt("legacy", Bool),
        opt("force", Bool),
        opt("operator", Text),
        opt("limit", Int),
        opt("cursor", Text).redact(),
    ],
    "list, preview, settle, release or audit mount holders (operator)",
    run,
)
.paged(Paging::Cursor);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = RepairRequest {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        action: text("action").unwrap_or_default(),
        op: text("op"),
        legacy: matches!(args.get("legacy"), Some(SdkValue::Bool(true))),
        force: matches!(args.get("force"), Some(SdkValue::Bool(true))),
        operator: text("operator"),
        limit: args.get_int("limit"),
        cursor: text("cursor"),
    };
    let (home, _) = context.parts();
    layerfs_control::store::repair(home, &request)
}
