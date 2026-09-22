//! Failed attachment custody and explicit cleanup of its acquired resources.
use super::{host::WorkspaceHost, state::Inner};
use crate::{
    backing::{directory::identity, directory::Directory, metadata::Arena},
    Attachment, AttachmentCleanupProgress, AttachmentFailure, Workspace, WorkspaceError,
};
use std::{fs, io, path::Path, sync::Arc, sync::TryLockError, time::Instant};

pub(crate) enum EntryState {
    Attaching,
    Attached(Arc<Inner>),
    Failed {
        cause: WorkspaceError,
        cleanup: Option<WorkspaceError>,
        resources: Option<AttachResources>,
    },
}
#[derive(Default)]
pub(crate) struct AttachResources {
    pub directory: Option<Arc<Directory>>,
    pub arena: Option<Arc<Arena>>,
    mount: MountLeaf,
}
#[derive(Default)]
struct MountLeaf {
    created: bool,
    ambiguous: bool,
    identity: Option<(u64, u64)>,
}
pub(super) fn clock(deadline: Instant) -> Result<(), WorkspaceError> {
    if Instant::now() >= deadline {
        return Err(WorkspaceError::Deadline);
    }
    Ok(())
}
pub(super) fn lock_error<T>(error: TryLockError<T>) -> WorkspaceError {
    match error {
        TryLockError::WouldBlock => WorkspaceError::Busy,
        TryLockError::Poisoned(_) => WorkspaceError::Io,
    }
}
impl AttachResources {
    pub fn create_mount(&mut self, path: &Path, deadline: Instant) -> Result<(), WorkspaceError> {
        clock(deadline)?;
        self.mount.ambiguous = true;
        match super::host::create_owned_directory(path) {
            Ok(()) => {
                self.mount.created = true;
                self.mount.ambiguous = false;
            }
            Err(error) => {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    self.mount.ambiguous = false;
                }
                return Err(error.into());
            }
        }
        clock(deadline)?;
        let observed = fs::symlink_metadata(path)?;
        if !observed.is_dir() || observed.file_type().is_symlink() {
            return Err(WorkspaceError::Denied);
        }
        self.mount.identity = Some(identity(&observed));
        Ok(())
    }
    fn progress(&self) -> AttachmentCleanupProgress {
        AttachmentCleanupProgress::Retained {
            mount_directory: self.mount.created || self.mount.ambiguous,
            metadata_arena: self.arena.is_some(),
            backing_directory: self.directory.is_some(),
        }
    }
    fn cleanup(
        &mut self,
        host: &WorkspaceHost,
        path: &Path,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        if let Some(arena) = &self.arena {
            clock(deadline)?;
            host.inner
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?
                .close(arena, deadline)?;
            self.arena = None;
        }
        if let Some(directory) = &self.directory {
            clock(deadline)?;
            directory.close()?;
            self.directory = None;
        }
        if self.mount.created || self.mount.ambiguous {
            clock(deadline)?;
            match fs::symlink_metadata(path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(metadata) => {
                    if !self.mount.created
                        || !metadata.is_dir()
                        || metadata.file_type().is_symlink()
                        || self.mount.identity != Some(identity(&metadata))
                    {
                        return Err(WorkspaceError::Denied);
                    }
                    clock(deadline)?;
                    fs::remove_dir(path)?;
                }
            }
            self.mount = MountLeaf::default();
        }
        Ok(())
    }
}
impl WorkspaceHost {
    /// Observes only this exact incarnation. Missing and stale identities return
    /// NotFound; a successful attachment returns its existing semantic owner.
    pub fn attachment(
        &self,
        id: &str,
        incarnation: [u8; 32],
    ) -> Result<Attachment, WorkspaceError> {
        super::host::validate_id(id)?;
        if incarnation == [0; 32] {
            return Err(WorkspaceError::InvalidInput);
        }
        let registry = self.inner.registry.try_lock().map_err(lock_error)?;
        let entry = registry
            .entries
            .iter()
            .find(|entry| entry.id.as_ref() == id && entry.incarnation == incarnation)
            .ok_or(WorkspaceError::NotFound)?;
        Ok(match &entry.state {
            EntryState::Attaching => Attachment::Attaching,
            EntryState::Attached(inner) => Attachment::Attached(Workspace {
                inner: inner.clone(),
                host: self.inner.clone(),
            }),
            EntryState::Failed {
                cause,
                cleanup,
                resources,
            } => Attachment::Failed(AttachmentFailure {
                cause: cause.clone(),
                cleanup: cleanup.clone(),
                progress: resources.as_ref().map_or(
                    AttachmentCleanupProgress::Running,
                    AttachResources::progress,
                ),
            }),
        })
    }
    /// One explicit attempt with the original absolute deadline. Failed cleanup
    /// retains its resources and original cause. Admission never waits; final
    /// custody publication uses the short registry lock without I/O. Syscalls
    /// and lock scheduling have deadline observation points, not preemption.
    pub fn cleanup_failed_attach(
        &self,
        id: &str,
        incarnation: [u8; 32],
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        super::host::validate_id(id)?;
        if incarnation == [0; 32] {
            return Err(WorkspaceError::InvalidInput);
        }
        clock(deadline)?;
        let (resources, _scratch) = {
            let mut registry = self.inner.registry.try_lock().map_err(lock_error)?;
            let entry = registry
                .entries
                .iter_mut()
                .find(|entry| entry.id.as_ref() == id && entry.incarnation == incarnation)
                .ok_or(WorkspaceError::NotFound)?;
            clock(deadline)?;
            match &mut entry.state {
                EntryState::Failed { resources, .. } if resources.is_some() => {
                    let scratch = self.inner.budget.reserve(super::state::CALL_SCRATCH)?;
                    (resources.take().ok_or(WorkspaceError::Busy)?, scratch)
                }
                _ => return Err(WorkspaceError::Busy),
            }
        };
        let path = self.inner.config.root.join("workspace").join(id);
        self.finish_attach_cleanup(id, incarnation, resources, &path, deadline)
    }
    pub(super) fn fail_attach(
        &self,
        id: &str,
        incarnation: [u8; 32],
        cause: WorkspaceError,
        resources: AttachResources,
        path: &Path,
        deadline: Instant,
    ) {
        {
            // Poison cannot discard an acquired owner. Preserve it for inspection;
            // public admission still refuses a poisoned registry with Io.
            let (mut registry, poisoned) = match self.inner.registry.lock() {
                Ok(registry) => (registry, false),
                Err(error) => (error.into_inner(), true),
            };
            let entry = registry
                .entries
                .iter_mut()
                .find(|entry| entry.id.as_ref() == id && entry.incarnation == incarnation)
                .expect("an admitted attachment retains its registry entry");
            if poisoned {
                entry.state = EntryState::Failed {
                    cause,
                    cleanup: Some(WorkspaceError::Io),
                    resources: Some(resources),
                };
                return;
            }
            entry.state = EntryState::Failed {
                cause,
                cleanup: None,
                resources: None,
            };
        }
        let _ = self.finish_attach_cleanup(id, incarnation, resources, path, deadline);
    }
    fn finish_attach_cleanup(
        &self,
        id: &str,
        incarnation: [u8; 32],
        mut resources: AttachResources,
        path: &Path,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        let mut result = resources.cleanup(self, path, deadline);
        let mut registry = match self.inner.registry.lock() {
            Ok(registry) => registry,
            Err(error) => {
                result = Err(WorkspaceError::Io);
                error.into_inner()
            }
        };
        if result.is_ok() {
            result = clock(deadline);
        }
        if result.is_ok() {
            registry
                .entries
                .retain(|entry| entry.id.as_ref() != id || entry.incarnation != incarnation);
        } else {
            let entry = registry
                .entries
                .iter_mut()
                .find(|entry| entry.id.as_ref() == id && entry.incarnation == incarnation)
                .expect("cleanup owns the retained attachment entry");
            let EntryState::Failed {
                cleanup,
                resources: retained,
                ..
            } = &mut entry.state
            else {
                unreachable!("only failed attachment cleanup moves these resources")
            };
            *cleanup = result.as_ref().err().cloned();
            *retained = Some(resources);
        }
        result
    }
}
