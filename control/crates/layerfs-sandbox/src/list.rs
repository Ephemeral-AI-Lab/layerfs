//! `sandbox.list`: one page of the Sandbox records, each with its state.
use super::family::page;
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{Outcome, View};
use std::path::Path;

/// `sandbox.list(project?, { page_size, page_pos })`: one page of the
/// Sandbox records, each with its state.
pub fn list(home: &Path, args: &SdkValue) -> Outcome {
    let only = args.get_text("project");
    let items: Vec<SdkValue> = super::record::sandboxes(home)?
        .iter()
        .filter(|r| only.is_none() || r.project() == only)
        .map(|record| record.to_sdk(View::List))
        .collect();
    let (items, next) = page(items, args);
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("next_page_pos", next),
    ]))
}
