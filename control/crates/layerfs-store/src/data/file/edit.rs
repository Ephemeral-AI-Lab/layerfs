//! `file.edit`: one text, or every occurrence of it, replaced in a file.
use super::read::{run, take};
use super::requests::FileEdit;
use super::write::{answer, base, on_main, word};
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::{with_fix, Outcome};
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;

/// `file.edit`: one text, or with `replace_all` every occurrence of it,
/// replaced in a file, as one Commit. More than one occurrence without
/// `replace_all` is refused with a `fix` that adds it.
pub fn edit(home: &Path, events: &mut Events<'_>, request: &FileEdit) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("path", &request.path)
        .text("oldText", &request.old_string)
        .text("newText", &request.new_string)
        .boolean("replaceAll", Some(request.replace_all));
    let value = run(home, events, &request.project, "fs.edit", args).map_err(|refusal| {
        match refusal.code == "AmbiguousMatch" {
            true => with_fix(
                refusal,
                format!(
                    "layerfs file edit {} {} {} --old-string {} --new-string {} --replace-all",
                    word(&request.project),
                    word(&request.branch),
                    word(&request.path),
                    word(&request.old_string),
                    word(&request.new_string),
                ),
            ),
            false => on_main(refusal, &request.project),
        }
    })?;
    // without --replace-all exactly one occurrence was replaced
    let replaced = match take(&value, "replaced") {
        SdkValue::Null => SdkValue::Int(1),
        counted => counted,
    };
    Ok(answer(&value, Some(replaced)))
}
