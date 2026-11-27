//! `file.rm`: a path removed, as one Commit.
use super::read::run;
use super::requests::FileRm;
use super::write::{answer, base, on_main, word};
use layerfs_home::reply::events::Events;
use layerfs_home::reply::outcome::{with_fix, Outcome};
use std::path::Path;

/// `file.rm`: a path removed, as one Commit; a folder that is not empty
/// only with `recursive`, else it is refused with a `fix` that adds it.
pub fn rm(home: &Path, events: &mut Events<'_>, request: &FileRm) -> Outcome {
    let mut args = base(&request.branch, &request.guard);
    args.text("path", &request.path)
        .boolean("recursive", Some(request.recursive));
    let value =
        run(home, events, &request.project, "fs.rm", args).map_err(|refusal| {
            match refusal.code == "NotEmpty" {
                true => with_fix(
                    refusal,
                    format!(
                        "layerfs file rm {} {} {} --recursive",
                        word(&request.project),
                        word(&request.branch),
                        word(&request.path),
                    ),
                ),
                false => on_main(refusal, &request.project),
            }
        })?;
    Ok(answer(&value, None))
}
