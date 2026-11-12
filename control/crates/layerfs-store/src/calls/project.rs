//! Saved-tree project statistics through the ordinary Store-call route.
use super::op::{on_project, StoreOp};
use layerfs_home::{events::Events, outcome::Outcome};
use std::path::Path;

/// One pinned project's entry-count request.
#[derive(Clone, Debug)]
pub struct ProjectStats {
    /// `<store>/<project>`.
    pub project: String,
    /// Branch context, `main` by default.
    pub branch: String,
    /// Optional full Commit or Layer id admitted by that context.
    pub commit: Option<String>,
}

/// Count one immutable saved tree, excluding the root directory.
pub fn stats(home: &Path, events: &mut Events<'_>, request: &ProjectStats) -> Outcome {
    on_project(
        home,
        events,
        &request.project,
        StoreOp::ProjectStats(request),
    )
}

/// One project's explicit immutable saved-snapshot comparison.
#[derive(Clone, Debug)]
pub struct ProjectStorageReport {
    /// `<store>/<project>`; injected into every normalized snapshot.
    pub project: String,
    /// JSON `[{branch,commit}]`; project fields are refused.
    pub snapshots: layerfs_bridge::control::SdkValue,
    /// Global object-visit budget, at most 10,000.
    pub max_objects: Option<i64>,
    /// Visited canonical output budget, at most 64 MiB; not total physical/dependency I/O.
    pub max_canonical_bytes: Option<i64>,
    /// Observe exact selected WholeFile/Chunk record bytes; excludes shared framing/dependencies.
    pub include_encoding: Option<bool>,
}

/// Restrict one report to a project without a second traversal implementation.
pub fn storage_report(
    home: &Path,
    events: &mut Events<'_>,
    request: &ProjectStorageReport,
) -> Outcome {
    use layerfs_bridge::control::SdkValue;
    use layerfs_home::{home::with_field, outcome::refused};
    let (store, _) = layerfs_home::names::project(&request.project)
        .map_err(|why| refused("InvalidInput", why))?;
    let SdkValue::List(pins) = &request.snapshots else {
        return Err(refused(
            "InvalidInput",
            "snapshots must be [{branch,commit}]",
        ));
    };
    if pins.is_empty() || pins.len() > 64 {
        return Err(refused(
            "InvalidInput",
            "snapshots must contain 1..=64 explicit immutable pins",
        ));
    }
    let mut snapshots = Vec::with_capacity(pins.len());
    for pin in pins {
        let SdkValue::Map(fields) = pin else {
            return Err(refused(
                "InvalidInput",
                "each project snapshot must be {branch,commit}",
            ));
        };
        if fields
            .iter()
            .any(|(key, _)| !matches!(key.as_str(), "branch" | "commit"))
        {
            return Err(refused(
                "InvalidInput",
                "project snapshots accept only branch and commit; the call supplies the project",
            ));
        }
        snapshots.push(with_field(
            pin.clone(),
            "project",
            SdkValue::text(&request.project),
        ));
    }
    let result = crate::storage_report::storage_report(
        home,
        events,
        &crate::storage_report::StorageReport {
            name: store.to_owned(),
            snapshots: SdkValue::List(snapshots),
            max_objects: request.max_objects,
            max_canonical_bytes: request.max_canonical_bytes,
            include_encoding: request.include_encoding,
        },
    )?;
    let result = with_field(result, "scope", SdkValue::text("project"));
    let SdkValue::Map(mut fields) = with_field(result, "project", SdkValue::text(&request.project))
    else {
        return Err(refused("Failed", "project storage report is not a record"));
    };
    fields.retain(|(name, _)| name != "name");
    Ok(SdkValue::Map(fields))
}
