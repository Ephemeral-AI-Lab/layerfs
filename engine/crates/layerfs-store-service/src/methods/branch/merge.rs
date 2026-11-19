//! `branch.merge`.
use crate::methods::args::{invalid, project, text, Answer};
use crate::{history::branch, StoreService};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::merge::MergeUnit;

/// `merge(branch, with, { name?, expected?, message?, units? })`: `with`
/// merged into `branch`, the result a new Branch `name` when given, else
/// `branch` moved to it; a fast-forward or one merge Commit; a conflict
/// changes nothing.
pub fn merge(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let units = units(args)?;
    let request = branch::Merge {
        left: text(args, "branch")?,
        right: text(args, "with")?,
        into: args.get_text("name"),
        expected: args.get_text("expected"),
        message: args.get_text("message"),
        units: &units,
    };
    branch::merge(&service.store, &stack, &request)
}

/// The `units` argument (#352 F-057): each a bare pattern, named by itself,
/// or `{ name, patterns }`. Names and patterns are admitted by the merge.
fn units(args: &SdkValue) -> Result<Vec<MergeUnit>, SdkRefusal> {
    let list = match args.get("units") {
        None | Some(SdkValue::Null) => return Ok(Vec::new()),
        Some(SdkValue::List(list)) => list,
        Some(_) => return Err(invalid("units must be a list")),
    };
    list.iter()
        .map(|unit| match unit {
            SdkValue::Text(pattern) => Ok(MergeUnit {
                name: pattern.clone(),
                patterns: vec![pattern.clone()],
            }),
            SdkValue::Map(_) => {
                let patterns = match unit.get("patterns") {
                    Some(SdkValue::List(patterns)) => patterns
                        .iter()
                        .map(|pattern| match pattern {
                            SdkValue::Text(pattern) => Ok(pattern.clone()),
                            _ => Err(invalid("a unit pattern must be text")),
                        })
                        .collect::<Result<_, _>>()?,
                    _ => return Err(invalid("a unit's patterns must be a list")),
                };
                Ok(MergeUnit {
                    name: text(unit, "name")?.to_owned(),
                    patterns,
                })
            }
            _ => Err(invalid("a unit must be a pattern or { name, patterns }")),
        })
        .collect()
}
