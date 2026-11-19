//! `file.apply`: apply a list of changes; one Commit, all or none.
use super::write::{COMMITTED, EXPECTED};
use super::{guard, text};
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Json, Name, Project};
use crate::calls::table::{a, call, Call, MESSAGE};
use crate::calls::{Context, Policy};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::calls::file::{apply, FileApply};
use layerfs_store::calls::Change;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "apply",
    &[a("project", Project), a("branch", Name), a("changes", Json)],
    &[EXPECTED, MESSAGE],
    "apply a list of changes; one Commit, all or none",
    run,
)
.scoped(Rule::FileChanges)
.policy(Policy::Project("project"))
.saves()
.form(&COMMITTED);

/// Runs `file.apply`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = FileApply {
        project: text(args, "project").unwrap_or_default(),
        branch: text(args, "branch").unwrap_or_default(),
        changes: changes(args.get("changes"))?,
        guard: guard(args),
    };
    let (home, events) = context.parts();
    apply(home, events, &request)
}

/// `file.apply`'s changes, each a record of the single calls' fields
/// (`op`, `path`, `from`, `to`, `content`, `old_string`, `new_string`,
/// `replace_all`, `mode`, `recursive`); the Store checks each name.
fn changes(value: Option<&SdkValue>) -> Result<Vec<Change>, SdkRefusal> {
    let Some(SdkValue::List(list)) = value else {
        return Err(refused(
            "InvalidInput",
            "`changes` must be a list of changes",
        ));
    };
    let change = |change: &SdkValue| match change {
        SdkValue::Map(fields) => Ok(Change {
            fields: fields.clone(),
        }),
        _ => Err(refused("InvalidInput", "each change is a record")),
    };
    list.iter().map(change).collect()
}
