//! `sandbox.scratch` (#549): one bounded, machine-keyed pass over the
//! settled scratch this home's records own. The close path uses the same
//! policy (`sandbox::scratch::sweep`), so the automatic and the operator
//! pass cannot drift; this module only reads the call and answers it.
use super::scratch::{self, Mode, Owned, Report};
use crate::common::context::Context;
use crate::common::outcome::{refused, Outcome};
use layerfs_bridge::control::SdkValue;

/// `sandbox.scratch(machine, { dry_run })`: the roots a real pass would
/// delete (`dry_run`), or the roots it deleted. `machine` is the machine
/// whose records this pass may clean: records of another machine are left
/// for that machine's pass, so a pass never touches another machine's
/// scratch. Every entry names its Sandbox, so an operator sees the record
/// that authorized each root and not only a path.
pub(super) fn scratch(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let machine = args.get_text("machine").unwrap_or("");
    if machine.is_empty() {
        return Err(refused(
            "InvalidInput",
            "sandbox.scratch: a machine name is required",
        ));
    }
    let dry_run = matches!(args.get("dry_run"), Some(SdkValue::Bool(true)));
    let mode = match dry_run {
        true => Mode::DryRun,
        false => Mode::Apply,
    };
    let report = scratch::sweep(&context.home, machine, mode)?;
    Ok(answer(machine, dry_run, &report))
}

/// The pass as the call answers it: a list of roots per outcome, and the
/// reason a kept one was kept.
fn answer(machine: &str, dry_run: bool, report: &Report) -> SdkValue {
    SdkValue::map([
        ("machine", SdkValue::text(machine)),
        ("dry_run", SdkValue::Bool(dry_run)),
        (
            "planned",
            SdkValue::List(report.planned.iter().map(entry).collect()),
        ),
        (
            "deleted",
            SdkValue::List(report.deleted.iter().map(entry).collect()),
        ),
        (
            "kept",
            SdkValue::List(
                report
                    .kept
                    .iter()
                    .map(|(owned, why)| {
                        SdkValue::map([
                            ("sandbox", SdkValue::text(owned.sandbox())),
                            ("root", SdkValue::text(owned.root().to_string_lossy())),
                            ("why", SdkValue::text(why.as_str())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ])
}

/// One planned or deleted root: `{ sandbox, root }`.
fn entry(owned: &Owned) -> SdkValue {
    SdkValue::map([
        ("sandbox", SdkValue::text(owned.sandbox())),
        ("root", SdkValue::text(owned.root().to_string_lossy())),
    ])
}
