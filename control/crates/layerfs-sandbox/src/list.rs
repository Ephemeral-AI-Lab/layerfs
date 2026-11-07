//! `sandbox.list`: one page of the Sandbox records.
use super::family::{page, Page};
use super::record::SandboxRecord;
use layerfs_bridge::control::SdkRefusal;
use std::path::Path;

/// `sandbox.list(project?, { page_size, page_pos })`.
#[derive(Clone, Debug, Default)]
pub struct ListRequest {
    /// Only this project's Sandboxes, when it names one.
    pub project: Option<String>,
    /// The page it asks for.
    pub page: Page,
}

/// One page of the Sandbox records and the next page's position, if any.
pub struct Listed {
    /// The page's records, in the home's order.
    pub items: Vec<SandboxRecord>,
    /// The next page's position; `None` after the last.
    pub next: Option<usize>,
}

/// One page of the Sandbox records (of `request`'s project, when it names
/// one).
pub fn list(home: &Path, request: &ListRequest) -> Result<Listed, SdkRefusal> {
    let only = request.project.as_deref();
    let items: Vec<SandboxRecord> = super::record::sandboxes(home)?
        .into_iter()
        .filter(|r| only.is_none() || r.project() == only)
        .collect();
    let (items, next) = page(items, request.page);
    Ok(Listed { items, next })
}
