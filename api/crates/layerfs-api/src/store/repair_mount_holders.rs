//! `store.repair_mount_holders`: explicit first-class repair of one Branch's
//! mount admissions, on the machine whose worker holds a SQLite Store and in
//! this process for a PostgreSQL Store. List and preview write nothing: they
//! answer the typed holder view, preview with what release would do for each
//! holder; settle/release name one op; `audit` reads the bounded retained done
//! rows of that Branch. Release retains the row as the durable operator audit
//! and records the operator label; every call is also in the home's
//! `trace/calls.jsonl` call log.
use crate::calls::table::{
    a, call, opt, Call,
    Kind::{Bool, Int, Name, Text},
};
use crate::calls::Context;
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{refused, Outcome};

pub(crate) const CALL: Call = call(
    "store",
    "repair_mount_holders",
    &[
        a("name", Name),
        a("project", Name),
        a("branch", Name),
        a("action", Name),
    ],
    &[
        opt("op", Text),
        opt("legacy", Bool),
        opt("operator", Text),
        opt("page", Int),
        opt("limit", Int),
    ],
    "list, preview, settle, release or audit mount holders (operator)",
    run,
);

fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let name = StoreName::parse(args.get_text("name").unwrap_or(""))?;
    let project = args.get_text("project").unwrap_or("").to_owned();
    let branch = args.get_text("branch").unwrap_or("").to_owned();
    let action = args.get_text("action").unwrap_or("preview").to_owned();
    let op = match args.get_text("op") {
        None | Some("") => None,
        Some(text) => Some(
            layerfs_bridge::sdk_values::unhex(text)
                .and_then(|bytes| bytes.try_into().ok())
                .ok_or_else(|| refused("InvalidInput", "op must be 16 bytes of hex"))?,
        ),
    };
    let legacy = matches!(args.get("legacy"), Some(SdkValue::Bool(true)));
    let operator = args.get_text("operator").unwrap_or("");
    let page = args
        .get_int("page")
        .and_then(|page| u32::try_from(page).ok())
        .unwrap_or(0);
    let limit = args
        .get_int("limit")
        .and_then(|limit| u32::try_from(limit).ok())
        .unwrap_or(100);
    let (home, _) = context.parts();
    layerfs_control::store::repair(
        home, &name, &project, &branch, &action, op, legacy, operator, page, limit,
    )
}
