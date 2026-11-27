//! `store.repair_mount_holders`: explicit first-class repair of one Branch's
//! mount admissions and the Init, merge and removal admissions at the Branch
//! or its project, on the machine whose worker holds a SQLite Store and in
//! this process for a PostgreSQL Store. `list` and `preview` write nothing:
//! one page of holders, each with its row `state` and the `liveness`
//! admission would give it now; `settle` refuses a holder still `live`
//! unless forced; `release` retains a settled row as the durable done audit
//! and records the operator label; `settle_legacy` settles only mount rows
//! without owner or place evidence; `audit` pages the Branch's retained rows by
//! `(updated_ms, op)`.
//! Every call is also in the home's `trace/calls.jsonl` call log.
use crate::calls::table::{
    a, call, opt, Call,
    Kind::{Bool, Int, Name, Project, Text},
};
use crate::calls::Context;
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::admin::with_users::RepairRequest;

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
    "list, preview, settle, release, settle_legacy or audit a Branch's holders (operator)",
    run,
);
// It pages by `limit`/`cursor`, but its answers are not on the 2.0 answer
// form yet, so it declares no `Paging` (an entry without a form declares
// none; calls_table checks this).

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
    layerfs_store::admin::with_users::repair(home, &request)
}
