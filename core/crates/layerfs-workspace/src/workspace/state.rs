//! Logical Workspace binding. SQL/physical state remains in the overlay crate.
use crate::{serials::Serials, BaseView, CanonicalClient, Refusal};
use layerfs_content::{
    filesystem::{FilesystemRootId, InodeScope},
    ContentError,
};
use layerfs_overlay::{Overlay, OverlayError, Route};
use std::{
    fmt,
    sync::{Arc, Mutex, RwLock},
};

pub struct Workspace {
    route: Route,
    pub(crate) base: Arc<RwLock<BaseView>>,
    pub(crate) serials: Arc<Mutex<Serials>>,
    operation_client: Option<Arc<CanonicalClient>>,
}
#[derive(Debug)]
pub enum WorkspaceError {
    Content(ContentError),
    Overlay(OverlayError),
    BindingPoisoned,
    MissingLengthProvider,
    Service(Box<dyn std::error::Error + Send + Sync>),
    /// A definite pre-effect refusal of an ordinary namespace operation.
    Refused(Refusal),
    BaseChanged {
        expected: [u8; 32],
        actual: [u8; 32],
    },
}
pub type WorkspaceResult<T> = Result<T, WorkspaceError>;
impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Content(error) => Some(error),
            Self::Overlay(error) => Some(error),
            Self::Service(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}
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
        Ok(Self::bind(route, base))
    }
    /// Binds a checked root to an already-open owner route. The source-read
    /// entry verifies the selected root before any effective read.
    pub fn bind(route: Route, base: BaseView) -> Self {
        Self {
            route,
            base: Arc::new(RwLock::new(base)),
            serials: Arc::new(Mutex::new(Serials::default())),
            operation_client: None,
        }
    }
    pub const fn route(&self) -> Route {
        self.route
    }
    /// One operation's provider over the original binding and serial owner.
    /// No new namespace, root acquisition or serial range is created. Every
    /// later base() observes the original cell's current checked root, while
    /// existing source/read plans retain their own immutable binding/client.
    /// The caller must authorize this client for the same Workspace context.
    pub fn scoped(&self, client: Arc<CanonicalClient>) -> WorkspaceResult<Self> {
        // Preserve the original poisoned-binding refusal before returning a
        // new view. This fixed metadata clone does not perform provider I/O.
        let _ = self.base()?;
        Ok(Self {
            route: self.route,
            base: self.base.clone(),
            serials: self.serials.clone(),
            operation_client: Some(client),
        })
    }
    /// Retains the selected immutable binding with no lock across content IO.
    pub fn base(&self) -> WorkspaceResult<BaseView> {
        let base = self
            .base
            .read()
            .map(|base| base.clone())
            .map_err(|_| WorkspaceError::BindingPoisoned)?;
        Ok(match &self.operation_client {
            Some(client) => base.with_client(client.clone()),
            None => base,
        })
    }
}

impl fmt::Debug for Workspace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Workspace")
            .field("route", &self.route)
            .finish_non_exhaustive()
    }
}
