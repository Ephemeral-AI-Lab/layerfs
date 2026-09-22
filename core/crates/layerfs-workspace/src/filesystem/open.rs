//! Portable file-open rights and atomic admission of truncating handles.
use crate::{
    backing::budget::Charge,
    runtime::state::{Handle, OperationGuard, State, HANDLE_LIMIT},
    *,
};
use std::{mem::size_of, time::Instant};

pub(super) struct OpenReservation {
    workspace: Workspace,
    pub id: HandleId,
    pub serial: u64,
    options: FileOpenOptions,
    scope: ReferenceScope,
    finished: bool,
    _charge: Charge,
}
impl OpenReservation {
    pub fn validate(&self, state: &State, serial: u64) -> Result<usize, WorkspaceError> {
        if self.finished || serial != self.serial {
            return Err(WorkspaceError::BadHandle);
        }
        if self.scope == ReferenceScope::Projection && !state.mounted {
            return Err(WorkspaceError::Busy);
        }
        let index = state
            .handles
            .iter()
            .position(|handle| {
                handle.id == self.id
                    && !handle.ready
                    && !handle.directory
                    && handle.serial == serial
                    && handle.scope == self.scope
                    && handle.options == self.options
            })
            .ok_or(WorkspaceError::BadHandle)?;
        let node = state.node(serial)?;
        super::namespace::check_access(
            node.attr,
            self.workspace.root().uid,
            mask(self.options.access),
        )?;
        Ok(index)
    }
    pub fn publish(&mut self, state: &mut State, index: usize) {
        state.handles[index].ready = true;
        self.finished = true;
    }
}
impl Drop for OpenReservation {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if let Ok(mut state) = self.workspace.state() {
            let Some(handle) = state.handles.iter().position(|handle| {
                handle.id == self.id && !handle.ready && handle.serial == self.serial
            }) else {
                return;
            };
            let Some(node) = state
                .nodes
                .iter()
                .position(|node| node.attr.serial == self.serial && node.handles > 0)
            else {
                return;
            };
            state.handles.swap_remove(handle);
            state.nodes[node].handles -= 1;
            state.collect(self.workspace.root().serial);
        }
    }
}
fn mask(access: FileAccess) -> u8 {
    match access {
        FileAccess::ReadOnly => 4,
        FileAccess::WriteOnly => 2,
        FileAccess::ReadWrite => 6,
    }
}
pub(super) fn check_options(
    options: FileOpenOptions,
    access: WorkspaceAccess,
) -> Result<(), WorkspaceError> {
    if options.access == FileAccess::ReadOnly && (options.append || options.truncate) {
        return Err(WorkspaceError::InvalidInput);
    }
    if options.access != FileAccess::ReadOnly && access == WorkspaceAccess::ReadOnly {
        return Err(WorkspaceError::ReadOnly);
    }
    Ok(())
}
pub(super) fn handle_slot(state: &State) -> Result<(HandleId, u64), WorkspaceError> {
    if state.handles.len() == HANDLE_LIMIT || state.handles.len() == state.handles.capacity() {
        return Err(WorkspaceError::Capacity);
    }
    let id = state.next_handle;
    Ok((id, id.checked_add(1).ok_or(WorkspaceError::Capacity)?))
}
impl Workspace {
    /// Opens an existing cached regular inode. Truncation and handle publication
    /// form one operation; append intent is retained for the handle's lifetime.
    /// A later projection failure returns Coherence with the published handle;
    /// the caller can inspect or release it without replaying the truncation.
    pub fn open_file(
        &self,
        serial: u64,
        options: FileOpenOptions,
        scope: ReferenceScope,
        deadline: Instant,
    ) -> Result<HandleId, WorkspaceError> {
        check_options(options, self.inner.access)?;
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        self.open_file_admitted(serial, options, scope, deadline, &mut operation)
            .map(|(_, handle)| handle)
    }
    pub(super) fn open_file_admitted(
        &self,
        serial: u64,
        options: FileOpenOptions,
        scope: ReferenceScope,
        deadline: Instant,
        operation: &mut OperationGuard,
    ) -> Result<(NodeAttributes, HandleId), WorkspaceError> {
        check_options(options, self.inner.access)?;
        if !options.truncate {
            let mut state = self.state()?;
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            let attr = state.node(serial)?.attr;
            let handle = self.insert_handle(&mut state, serial, false, scope, options, true)?;
            return Ok((attr, handle));
        }
        let charge = self.host.budget.reserve(size_of::<OpenReservation>())?;
        let mut reserved = {
            let mut state = self.state()?;
            crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
            let id = self.insert_handle(&mut state, serial, false, scope, options, false)?;
            OpenReservation {
                workspace: self.clone(),
                id,
                serial,
                options,
                scope,
                finished: false,
                _charge: charge,
            }
        };
        let attr = self.truncate_open(&mut reserved, deadline, operation)?;
        Ok((attr, reserved.id))
    }
    pub(crate) fn open_directory_handle(
        &self,
        serial: u64,
        scope: ReferenceScope,
    ) -> Result<HandleId, WorkspaceError> {
        let mut state = self.state()?;
        self.insert_handle(
            &mut state,
            serial,
            true,
            scope,
            FileOpenOptions::default(),
            true,
        )
    }
    fn insert_handle(
        &self,
        state: &mut State,
        serial: u64,
        directory: bool,
        scope: ReferenceScope,
        options: FileOpenOptions,
        ready: bool,
    ) -> Result<HandleId, WorkspaceError> {
        self.available(state)?;
        if scope == ReferenceScope::Projection && !state.mounted {
            return Err(WorkspaceError::Busy);
        }
        let node = state
            .nodes
            .iter()
            .position(|node| node.attr.serial == serial)
            .ok_or(WorkspaceError::NotFound)?;
        let attr = state.nodes[node].attr;
        if directory && attr.kind != NodeKind::Directory {
            return Err(WorkspaceError::NotDirectory);
        }
        if !directory && attr.kind == NodeKind::Directory {
            return Err(WorkspaceError::IsDirectory);
        }
        if !directory && attr.kind != NodeKind::File {
            return Err(WorkspaceError::WrongKind);
        }
        super::namespace::check_access(attr, self.root().uid, mask(options.access))?;
        let (id, next) = handle_slot(state)?;
        let references = state.nodes[node]
            .handles
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        state.next_handle = next;
        state.nodes[node].handles = references;
        let view = directory.then(|| super::namespace_view::View {
            base: state.base,
            root: state.overlay.clone(),
        });
        state.handles.push(Handle {
            id,
            serial,
            directory,
            scope,
            options,
            ready,
            view,
        });
        Ok(id)
    }
}
