//! Logical Workspace binding. SQL/physical state remains in the overlay crate.
use crate::{BaseView, CanonicalClient};
use layerfs_content::{
    filesystem::{FilesystemRootId, InodeScope},
    ContentError,
};
use layerfs_overlay::{Overlay, OverlayError, Route};
use std::{fmt, sync::Arc};

pub struct Workspace {
    route: Route,
    base: BaseView,
}
#[derive(Debug)]
pub enum WorkspaceError {
    Content(ContentError),
    Overlay(OverlayError),
}
pub type WorkspaceResult<T> = Result<T, WorkspaceError>;
impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WorkspaceError {}
impl From<ContentError> for WorkspaceError {
    fn from(e: ContentError) -> Self {
        Self::Content(e)
    }
}
impl From<OverlayError> for WorkspaceError {
    fn from(e: OverlayError) -> Self {
        Self::Overlay(e)
    }
}
impl Workspace {
    /// Bind a real full root and local namespace. Native attachment is separate.
    pub fn open(
        overlay: &Overlay,
        client: Arc<CanonicalClient>,
        root: FilesystemRootId,
        scope: InodeScope,
        incarnation: [u8; 32],
    ) -> WorkspaceResult<Self> {
        let base = BaseView::open(client, root, scope)?;
        let route = overlay.open_workspace(incarnation, root.0.to_bytes())?;
        Ok(Self { route, base })
    }
    pub const fn route(&self) -> Route {
        self.route
    }
    pub fn base(&self) -> &BaseView {
        &self.base
    }
}
