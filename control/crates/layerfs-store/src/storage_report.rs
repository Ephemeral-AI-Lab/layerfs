//! Bounded canonical sharing among explicit immutable saved snapshots of one
//! Store's projects, on the Store's existing in-process or worker route.
//!
//! The caller names every snapshot; nothing here lists history or heads. The
//! Store resolves each pin, walks its objects under the call's budgets and
//! answers in its own project names, which this file qualifies again.
use crate::{call, calls::StoreOp, record::StoreRecord, resolve::Target};
use layerfs_bridge::control::SdkValue;
use layerfs_home::{
    events::Events,
    home::with_field,
    outcome::{refused, Outcome},
};
use std::path::Path;

/// Most snapshots one report accepts.
pub const MAXIMUM_SNAPSHOTS: usize = 64;

/// One explicit population of saved snapshots and the report's budgets.
#[derive(Clone, Debug)]
pub struct StorageReport {
    /// The Store.
    pub name: String,
    /// `[{project, branch, commit}]`, each project as `<store>/<project>`.
    pub snapshots: SdkValue,
    /// Global object-visit budget, 1..=10,000 (10,000 when absent).
    pub max_objects: Option<i64>,
    /// Canonical bytes read, 1..=64 MiB (64 MiB when absent); not physical I/O.
    pub max_canonical_bytes: Option<i64>,
    /// Also observe the selected payload objects' own encoded records.
    pub include_encoding: Option<bool>,
}

/// Report `request`'s population; start no worker and refuse a Store being moved.
pub fn storage_report(home: &Path, events: &mut Events<'_>, request: &StorageReport) -> Outcome {
    let record = StoreRecord::of(home, &request.name)?;
    let snapshots = store_snapshots(&request.name, &request.snapshots)?;
    let target = Target {
        store: request.name.clone(),
        // the snapshots name their projects; the call itself needs none
        project: String::new(),
        store_ref: record.store_ref()?.clone(),
        record,
    };
    let sent = StorageReport {
        snapshots,
        ..request.clone()
    };
    let value = call::store_call(home, events, &target, &StoreOp::StorageReport(&sent))?;
    let SdkValue::Map(fields) = value else {
        return Err(refused("Failed", "Store storage report is not a record"));
    };
    let mut fields: Vec<(String, SdkValue)> = fields
        .into_iter()
        .map(|(key, value)| {
            let value = match key.as_str() {
                "snapshots" | "projects" => qualified(&request.name, value),
                _ => value,
            };
            (key, value)
        })
        .collect();
    fields.push(("scope".to_owned(), SdkValue::text("store")));
    fields.push(("name".to_owned(), SdkValue::text(&request.name)));
    Ok(SdkValue::Map(fields))
}

/// The pins as the Store names them: each project without its Store.
fn store_snapshots(
    store: &str,
    snapshots: &SdkValue,
) -> Result<SdkValue, layerfs_bridge::control::SdkRefusal> {
    let SdkValue::List(pins) = snapshots else {
        return Err(refused(
            "InvalidInput",
            "snapshots must be [{project,branch,commit}]",
        ));
    };
    if pins.is_empty() || pins.len() > MAXIMUM_SNAPSHOTS {
        return Err(refused(
            "InvalidInput",
            "snapshots must contain 1..=64 explicit immutable pins",
        ));
    }
    let mut out = Vec::with_capacity(pins.len());
    for pin in pins {
        let SdkValue::Map(fields) = pin else {
            return Err(refused(
                "InvalidInput",
                "each snapshot must be {project,branch,commit}",
            ));
        };
        if fields
            .iter()
            .any(|(key, _)| !matches!(key.as_str(), "project" | "branch" | "commit"))
        {
            return Err(refused("InvalidInput", "unknown snapshot field"));
        }
        let text = |name: &str| {
            pin.get_text(name)
                .ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
        };
        let project = text("project")?;
        let (owner, name) = layerfs_home::names::project(project)
            .map_err(|_| refused("InvalidInput", "snapshot project must be <store>/<project>"))?;
        if owner != store {
            return Err(refused(
                "InvalidInput",
                "snapshot project belongs to another Store",
            ));
        }
        out.push(SdkValue::map([
            ("project", SdkValue::text(name)),
            ("branch", SdkValue::text(text("branch")?)),
            ("commit", SdkValue::text(text("commit")?)),
        ]));
    }
    Ok(SdkValue::List(out))
}

/// Each listed record's `project` written with its Store again.
fn qualified(store: &str, value: SdkValue) -> SdkValue {
    let SdkValue::List(items) = value else {
        return value;
    };
    SdkValue::List(
        items
            .into_iter()
            .map(|item| match item.get_text("project") {
                Some(name) => {
                    let project = format!("{store}/{name}");
                    with_field(item, "project", SdkValue::text(project))
                }
                None => item,
            })
            .collect(),
    )
}
