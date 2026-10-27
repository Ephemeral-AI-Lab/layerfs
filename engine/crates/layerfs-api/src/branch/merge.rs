//! `branch.merge`.
use crate::base::args::{invalid, project, text, Answer};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::merge::MergeUnit;
use layerfs_store_service::{history::branch, StoreService};

/// `merge(a, b, { name, units? })`: a new Branch from two, fast-forward or one merge Commit; a conflict creates nothing.
pub fn merge(service: &StoreService, args: &SdkValue) -> Answer {
    let stack = project(service, args)?;
    let (left, right, name) = (
        text(args, "branch")?,
        text(args, "with")?,
        text(args, "name")?,
    );
    branch::merge(
        &service.store,
        &stack,
        left,
        right,
        name,
        args.get_text("message"),
        &units(args)?,
    )
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
