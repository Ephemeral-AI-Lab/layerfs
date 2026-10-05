//! Public read-only view-lease methods over one attached Workspace.
use crate::workspace::route_error;
use crate::WorkspaceApi;
use layerfs_api_core::{
    WorkspaceError, WorkspaceId, WorkspaceViewDirectoryPage, WorkspaceViewEntry,
    WorkspaceViewLease, WorkspaceViewRead, WorkspaceViewRelease, WorkspaceViewStatus,
};
use layerfs_bridge::contract::{Code, Operation, Response};

/// Largest one view read may request.
pub const WORKSPACE_VIEW_READ_BYTES: usize = 128 * 1024;
/// Largest one view listing page may request.
pub const WORKSPACE_VIEW_LIST_ENTRIES: usize = 128;
impl WorkspaceApi<'_> {
    /// Pins the current selected view of the attached Workspace as one
    /// charged read-only lease. The view cannot mutate or Commit; mounting,
    /// executing, committing and unmounting are unchanged.
    pub fn pin_view(&self, id: &WorkspaceId) -> Result<WorkspaceViewLease, WorkspaceError> {
        let route = self.owner.workspace(id).map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        let response = self
            .owner
            .control_call(route, 5_000, || Operation::WorkspacePinView {
                workspace,
                incarnation,
            })
            .map_err(route_error)?;
        let Response::WorkspaceViewLease(result) = response else {
            return Err(Code::Integrity.into());
        };
        WorkspaceViewLease::from_wire(id, &result)
    }

    /// Resolves one name component through the pinned view, under a parent
    /// directory entry the same lease issued.
    pub fn view_lookup(
        &self,
        lease: &WorkspaceViewLease,
        parent: &WorkspaceViewEntry,
        name: &[u8],
    ) -> Result<WorkspaceViewEntry, WorkspaceError> {
        let route = self
            .owner
            .workspace(&lease.workspace)
            .map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        if parent.lease != lease.token {
            return Err(WorkspaceError::Stale);
        }
        let response = self
            .owner
            .control_call(route, 5_000, || Operation::WorkspaceViewLookup {
                workspace,
                incarnation,
                view: lease.token.to_vec(),
                parent: parent.serial,
                name: name.to_vec(),
            })
            .map_err(route_error)?;
        match response {
            Response::WorkspaceViewEntry(result) => {
                WorkspaceViewEntry::from_wire(&lease.token, &result)
            }
            _ => Err(Code::Integrity.into()),
        }
    }

    /// One bounded listing page through the pinned view.
    pub fn view_list(
        &self,
        lease: &WorkspaceViewLease,
        dir: &WorkspaceViewEntry,
        after: Option<&[u8]>,
        max_entries: usize,
    ) -> Result<WorkspaceViewDirectoryPage, WorkspaceError> {
        let route = self
            .owner
            .workspace(&lease.workspace)
            .map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        if dir.lease != lease.token {
            return Err(WorkspaceError::Stale);
        }
        if max_entries == 0 || max_entries > WORKSPACE_VIEW_LIST_ENTRIES {
            return Err(WorkspaceError::Failure(Code::InvalidInput.into()));
        }
        let response = self
            .owner
            .control_call(route, 5_000, || Operation::WorkspaceViewList {
                workspace,
                incarnation,
                view: lease.token.to_vec(),
                directory: dir.serial,
                after: after.map(<[u8]>::to_vec),
                entries: max_entries as u16,
            })
            .map_err(route_error)?;
        match response {
            Response::WorkspaceViewList(result) => {
                Ok(WorkspaceViewDirectoryPage::from_wire(&result))
            }
            _ => Err(Code::Integrity.into()),
        }
    }

    /// Bounded pinned bytes of one file entry the same lease issued.
    pub fn view_read(
        &self,
        lease: &WorkspaceViewLease,
        file: &WorkspaceViewEntry,
        offset: u64,
        max_bytes: usize,
    ) -> Result<WorkspaceViewRead, WorkspaceError> {
        let route = self
            .owner
            .workspace(&lease.workspace)
            .map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        if file.lease != lease.token {
            return Err(WorkspaceError::Stale);
        }
        if max_bytes > WORKSPACE_VIEW_READ_BYTES {
            return Err(WorkspaceError::Failure(Code::InvalidInput.into()));
        }
        let response = self
            .owner
            .control_call(route, 5_000, || Operation::WorkspaceViewRead {
                workspace,
                incarnation,
                view: lease.token.to_vec(),
                file: file.serial,
                offset,
                bytes: max_bytes as u32,
            })
            .map_err(route_error)?;
        match response {
            Response::WorkspaceViewRead(result) => Ok(WorkspaceViewRead::from_wire(&result)),
            _ => Err(Code::Integrity.into()),
        }
    }

    /// The exact pinned symlink target of one link entry the same lease
    /// issued.
    pub fn view_readlink(
        &self,
        lease: &WorkspaceViewLease,
        link: &WorkspaceViewEntry,
    ) -> Result<Vec<u8>, WorkspaceError> {
        let route = self
            .owner
            .workspace(&lease.workspace)
            .map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        if link.lease != lease.token {
            return Err(WorkspaceError::Stale);
        }
        let response = self
            .owner
            .control_call(route, 5_000, || Operation::WorkspaceViewReadlink {
                workspace,
                incarnation,
                view: lease.token.to_vec(),
                link: link.serial,
            })
            .map_err(route_error)?;
        match response {
            Response::WorkspaceViewReadlink(result) => Ok(result.target),
            _ => Err(Code::Integrity.into()),
        }
    }

    /// Read-only custody observation of the held lease.
    pub fn view_status(
        &self,
        lease: &WorkspaceViewLease,
    ) -> Result<WorkspaceViewStatus, WorkspaceError> {
        let route = self
            .owner
            .workspace(&lease.workspace)
            .map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        let response = self
            .owner
            .control_call(route, 5_000, || Operation::WorkspaceViewStatus {
                workspace,
                incarnation,
                view: lease.token.to_vec(),
            })
            .map_err(route_error)?;
        match response {
            Response::WorkspaceViewStatus(result) => Ok(WorkspaceViewStatus::from_wire(&result)),
            _ => Err(Code::Integrity.into()),
        }
    }

    /// The checked remote release of the held lease. `Drop` alone is a
    /// best-effort release and never a checked one; this call runs the
    /// retirement selector and reports retained custody instead of hiding it.
    pub fn release_view(
        &self,
        lease: &WorkspaceViewLease,
    ) -> Result<WorkspaceViewRelease, WorkspaceError> {
        let route = self
            .owner
            .workspace(&lease.workspace)
            .map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        let response = self
            .owner
            .control_call(route, 15_000, || Operation::WorkspaceReleaseView {
                workspace,
                incarnation,
                view: lease.token.to_vec(),
            })
            .map_err(route_error)?;
        match response {
            Response::WorkspaceViewRelease(result) => {
                Ok(WorkspaceViewRelease::from_wire(result.outcome))
            }
            _ => Err(Code::Integrity.into()),
        }
    }
}
