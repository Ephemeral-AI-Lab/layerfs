//! `file.apply`: apply a list of changes; one Commit.
use crate::calls::table::Kind::{Json, Name, Project};
use crate::calls::table::{a, call, Call, MESSAGE};
use crate::calls::Context;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::calls::file::{apply, FileApply};
use layerfs_store::calls::Change;

/// Its table entry.
pub(crate) const CALL: Call = call(
    "file",
    "apply",
    &[a("project", Project), a("branch", Name), a("changes", Json)],
    &[MESSAGE],
    "apply a list of changes; one Commit",
    run,
);

/// Runs `file.apply`.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let text = |name: &str| args.get_text(name).map(str::to_owned);
    let request = FileApply {
        project: text("project").unwrap_or_default(),
        branch: text("branch").unwrap_or_default(),
        changes: changes(args.get("changes"))?,
        message: text("message"),
    };
    let (home, events) = context.parts();
    apply(home, events, &request)
}

/// `file.apply`'s changes, each a record of its snake_case fields.
fn changes(value: Option<&SdkValue>) -> Result<Vec<Change>, SdkRefusal> {
    let Some(SdkValue::List(list)) = value else {
        return Err(refused(
            "InvalidInput",
            "`changes` must be a list of changes",
        ));
    };
    let mut out = Vec::with_capacity(list.len());
    for change in list {
        let SdkValue::Map(fields) = change else {
            return Err(refused("InvalidInput", "each change is a record"));
        };
        for (name, _) in fields {
            // the Store's own names are not a change's: every path field
            // must be one the folder scope checks
            if matches!(
                name.as_str(),
                "from" | "to" | "oldText" | "newText" | "bytes"
            ) {
                return Err(refused(
                    "InvalidInput",
                    format!(
                        "`{name}` is not a change field; a change uses op, path, from_path, \
                         to_path, content, old, new and mode"
                    ),
                ));
            }
        }
        out.push(Change {
            fields: fields.clone(),
        });
    }
    Ok(out)
}
