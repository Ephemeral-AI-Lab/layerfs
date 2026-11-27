//! `store.repair_mount_holders`: explicit first-class repair of one Branch's
//! mount admissions and the Init, merge and removal admissions at the Branch
//! or its project, on the machine whose worker holds a SQLite Store and in
//! this process for a PostgreSQL Store. `list` and `preview` write nothing:
//! one page of holders, each with its row `status` and the `liveness`
//! admission would give it now; `settle` refuses a holder still `live`
//! unless forced; `release` retains a settled row as the durable done audit
//! and records the operator label; `settle_legacy` settles only mount rows
//! without owner or place evidence; `audit` pages the Branch's retained rows by
//! `(updated_ms, op)`.
//! Every call is also in the home's `trace/calls.jsonl` call log.
use crate::dispatch::table::{
    a, call, opt, Call,
    Kind::{Bool, Int, Name, Project, Text},
};
use crate::dispatch::{Context, Form, Paging, Part};
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::admin::with_users::RepairRequest;

/// One row of a page: a holder (`list`, `preview`), with its typed view,
/// its row `status` (`live`, `uncertain`), the `liveness` admission would
/// give it now and whether it is `safe` to settle; or a retained `done` row
/// (`audit`), with its scope, phase, `evidence` and `updated_ms`. The row's
/// id is `operation`: `op` is only ever the state companion.
const ROW: Form = Form::of(
    &["operation", "kind"],
    &[
        "status",
        "liveness",
        "safe",
        "legacy",
        "project",
        "branch",
        "workspace",
        "sandbox",
        "machine",
        "phase",
        "evidence",
        "updated_ms",
    ],
    &[
        "operation",
        "kind",
        "status",
        "liveness",
        "safe",
        "legacy",
        "project",
        "branch",
        "workspace",
        "sandbox",
        "machine",
        "owner_runtime",
        "owner_boot",
        "machine_incarnation",
        "sandbox_generation",
        "scope_kind",
        "scope_id",
        "phase",
        "evidence",
        "updated_ms",
    ],
);

/// Its answer, by action: a page `{ action, items, next_cursor }`, one
/// operation `{ action, operation, status }` (`settle`, `release`) or
/// `{ action, settled }` (`settle_legacy`).
const FORM: Form = Form::of(
    &["action"],
    &["items", "next_cursor", "operation", "status", "settled"],
    &[
        "items",
        "next_cursor",
        "action",
        "operation",
        "status",
        "settled",
    ],
)
.parts(&[("items", Part::Rows(&ROW))]);

/// Its table entry.
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
)
.form(&FORM)
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
    layerfs_store::admin::with_users::repair(home, &request)
}
