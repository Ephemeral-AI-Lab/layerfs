//! `sandbox.list`: one page of the Sandbox records.
use super::family::{page, Page};
#[cfg(unix)]
#[cfg(unix)]
use super::inspect::Live;
use super::record::SandboxRecord;
use layerfs_bridge::control::SdkRefusal;
use std::path::Path;

/// `sandbox.list(project?, { page_size, page_pos, live })`.
#[derive(Clone, Debug, Default)]
pub struct ListRequest {
    /// Only this project's Sandboxes, when it names one.
    pub project: Option<String>,
    /// The page it asks for.
    pub page: Page,
    /// Whether each Sandbox of the page is checked for a running daemon
    /// process on its machine (layerfs-project#164).
    pub live: bool,
}

/// One page of the Sandbox records and the next page's position, if any.
pub struct Listed {
    /// The page's records, in the home's order.
    pub items: Vec<SandboxRecord>,
    /// The next page's position; `None` after the last.
    pub next: Option<usize>,
    /// With `live`: whether each of `items`' daemon process runs, in their
    /// order.
    #[cfg(unix)]
    pub live: Option<Vec<Live>>,
}

/// One page of the Sandbox records (of `request`'s project, when it names
/// one). With `live`, one inspect request per machine among the page's
/// Sandboxes says which daemon processes run
/// ([`super::inspect::sandboxes_live`]): no daemon is asked and nothing is
/// started, and a machine that does not answer leaves its Sandboxes
/// unknown with the reason. Without it the records alone are read.
pub fn list(home: &Path, request: &ListRequest) -> Result<Listed, SdkRefusal> {
    let only = request.project.as_deref();
    let items: Vec<SandboxRecord> = super::record::sandboxes(home)?
        .into_iter()
        .filter(|r| only.is_none() || r.project() == only)
        .collect();
    let (items, next) = page(items, request.page);
#[cfg(unix)]
    let live = (request.live).then(|| super::inspect::sandboxes_live(home, &items));
    // Windows: the live face waits for the monitoring port (#179).
    #[cfg(windows)]
    let live: Option<Vec<super::inspect::Live>> = None;
    #[cfg(unix)]
    let listed = Listed { items, next, live };
    // Windows: no live face (the monitoring port is the #179 follow-up).
    #[cfg(windows)]
    let listed = Listed { items, next };
    Ok(listed)
}
