use crate::capture::{capture_workspace_candidate, SemanticDigestCache};
use crate::driver::{ProjectionDriver, ProjectionWorkspace, WorkspacePolicy};
use crate::managed_edit::{mutate_native, rename_native};
use crate::materialize::materialize_workspace_working;
use crate::refresh::refresh_workspace_working;
use crate::{OperationCounters, RootId, VfsResult};
use layerfs_core::CanonicalPath;
use layerfs_workspace::{
    DiskTable, Presentation, Result as WorkspaceResult, VersionRef, WorkingStore, WorkspaceDriver,
    WorkspaceError,
};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct MaterializationDriver<'a> {
    working: &'a WorkingStore,
    workspace: Option<Box<dyn ProjectionWorkspace>>,
    view: PathBuf,
    base_root: RootId,
    presentation_root: RootId,
    operation_id: layerfs_workspace::OperationId,
    candidate: Option<RootId>,
    counters: OperationCounters,
    refresh_counters: OperationCounters,
    live_scratch: Option<DiskTable>,
    digest_cache: SemanticDigestCache,
    exclusive_managed: bool,
    poisoned: bool,
}

impl<'a> MaterializationDriver<'a> {
    pub fn start(
        working: &'a WorkingStore,
        driver: &dyn ProjectionDriver,
        view: PathBuf,
        base_root: RootId,
        operation_id: layerfs_workspace::OperationId,
    ) -> VfsResult<Self> {
        Self::start_with_custody(working, driver, view, base_root, operation_id, false)
    }

    pub fn start_managed(
        working: &'a WorkingStore,
        driver: &dyn ProjectionDriver,
        view: PathBuf,
        base_root: RootId,
        operation_id: layerfs_workspace::OperationId,
    ) -> VfsResult<Self> {
        Self::start_with_custody(working, driver, view, base_root, operation_id, true)
    }

    fn start_with_custody(
        working: &'a WorkingStore,
        driver: &dyn ProjectionDriver,
        view: PathBuf,
        base_root: RootId,
        operation_id: layerfs_workspace::OperationId,
        exclusive_managed: bool,
    ) -> VfsResult<Self> {
        let policy = if exclusive_managed {
            WorkspacePolicy::ManagedPrivate
        } else {
            WorkspacePolicy::ExternalCooperative
        };
        let workspace = driver.open_workspace(&view, policy, working.storage_id())?;
        let digest_cache = SemanticDigestCache::default();
        let (counters, live_scratch) =
            materialize_workspace_working(working, &digest_cache, workspace.as_ref(), base_root)?;
        Ok(Self {
            working,
            workspace: Some(workspace),
            view,
            base_root,
            presentation_root: base_root,
            operation_id,
            candidate: exclusive_managed.then_some(base_root),
            counters,
            refresh_counters: OperationCounters::default(),
            live_scratch: Some(live_scratch),
            digest_cache,
            exclusive_managed,
            poisoned: false,
        })
    }

    pub fn candidate_root(&self) -> Option<RootId> {
        self.candidate
    }

    pub fn counters(&self) -> OperationCounters {
        self.counters
    }

    pub fn refresh_counters(&self) -> OperationCounters {
        self.refresh_counters
    }

    pub fn refresh_to(&mut self, target: VersionRef) -> VfsResult<OperationCounters> {
        if !self.exclusive_managed || self.poisoned {
            return Err(crate::VfsError::InvalidState);
        }
        let current = self.presentation_root;
        let target_root = target.root();
        let counters = match refresh_workspace_working(
            self.working,
            self.workspace
                .as_ref()
                .ok_or(crate::VfsError::InvalidState)?
                .as_ref(),
            self.live_scratch
                .as_ref()
                .ok_or(crate::VfsError::InvalidState)?,
            current,
            target_root,
        ) {
            Ok(counters) => counters,
            Err(error) => {
                self.poisoned = true;
                self.candidate = None;
                return Err(error);
            }
        };
        self.counters = self.counters.merge(counters)?;
        self.refresh_counters = counters;
        self.candidate = Some(target_root);
        if target_root != current {
            if let Err(error) = self
                .working
                .checkpoint_version_operation_candidate(self.operation_id, target)
            {
                self.poisoned = true;
                self.candidate = None;
                return Err(crate::VfsError::Io(std::io::Error::other(
                    error.to_string(),
                )));
            }
        }
        self.presentation_root = target_root;
        Ok(counters)
    }

    pub fn managed_read(
        &self,
        path: &CanonicalPath,
        start: u64,
        length: usize,
    ) -> VfsResult<Vec<u8>> {
        if !self.exclusive_managed || self.poisoned || length > 1024 * 1024 {
            return Err(crate::VfsError::InvalidState);
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or(crate::VfsError::InvalidState)?;
        let (parent, name) = crate::managed_edit::native_parent(
            workspace.as_ref(),
            workspace.root_directory()?,
            path,
        )?;
        let mut file = workspace.open_regular_read_at(parent.as_ref(), name, None)?;
        file.seek(SeekFrom::Start(start))?;
        let mut output = vec![0; length];
        file.read_exact(&mut output)?;
        Ok(output)
    }

    pub fn managed_replace_range(
        &mut self,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        replacement: &[u8],
    ) -> VfsResult<OperationCounters> {
        if replacement.len() > 1024 * 1024 || self.poisoned {
            return Err(crate::VfsError::InvalidState);
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or(crate::VfsError::InvalidState)?;
        let (metadata, native, sync_required) =
            match mutate_native(workspace.as_ref(), path, start, delete_len, replacement) {
                Ok(result) => result,
                Err(error) => {
                    self.poisoned = true;
                    return Err(error);
                }
            };
        let mut counters = OperationCounters::default();
        counters.add_native(native)?;
        counters.full_fallback_files =
            u64::from(native.route == Some(crate::NativeRoute::FullFallback));
        self.counters = self.counters.merge(counters)?;
        let _ = (metadata, sync_required);
        Ok(counters)
    }

    pub fn managed_rename(
        &mut self,
        from: &CanonicalPath,
        to: &CanonicalPath,
    ) -> VfsResult<OperationCounters> {
        if self.poisoned {
            return Err(crate::VfsError::InvalidState);
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or(crate::VfsError::InvalidState)?;
        let (source_metadata, target_metadata) = match rename_native(workspace.as_ref(), from, to) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.poisoned = true;
                return Err(error);
            }
        };
        let mut counters = OperationCounters::default();
        counters.add_native(crate::NativeOperationCounters {
            route: Some(crate::NativeRoute::Rename),
            rename_calls: 1,
            ..Default::default()
        })?;
        self.counters = self.counters.merge(counters)?;
        let _ = (source_metadata, target_metadata);
        Ok(counters)
    }
}

impl WorkspaceDriver for MaterializationDriver<'_> {
    fn presentation(&self) -> Presentation {
        Presentation::Materialization
    }

    fn view_path(&self) -> Option<&Path> {
        Some(&self.view)
    }

    fn quiesce(&mut self, _timeout: Duration) -> WorkspaceResult<()> {
        if self.exclusive_managed {
            return Ok(());
        }
        if escaped_writer_present(&self.view).map_err(|_| WorkspaceError::InvalidState)? {
            Err(WorkspaceError::Busy)
        } else {
            Ok(())
        }
    }

    fn freeze(&mut self) -> WorkspaceResult<()> {
        if self.poisoned {
            return Err(WorkspaceError::InvalidState);
        }
        if self.exclusive_managed {
            return self
                .candidate
                .is_some()
                .then_some(())
                .ok_or(WorkspaceError::InvalidState);
        }
        let workspace = self
            .workspace
            .as_ref()
            .ok_or(WorkspaceError::InvalidState)?;
        let (candidate, counters) = capture_workspace_candidate(
            self.working,
            &self.digest_cache,
            workspace.as_ref(),
            self.base_root,
            Some(self.operation_id),
        )
        .map_err(|_| WorkspaceError::InvalidState)?;
        self.candidate = Some(candidate);
        self.counters = self
            .counters
            .merge(counters)
            .map_err(|_| WorkspaceError::InvalidState)?;
        Ok(())
    }

    fn cleanup(&mut self) -> WorkspaceResult<()> {
        self.workspace.take();
        if let Some(scratch) = self.live_scratch.take() {
            scratch.finish().map_err(|_| WorkspaceError::InvalidState)?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn escaped_writer_present(view: &Path) -> std::io::Result<bool> {
    use std::process::Command;

    let output = Command::new("/usr/sbin/lsof")
        .args(["-n", "-P", "-t", "+D"])
        .arg(view)
        .output()?;
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(std::io::Error::other("lsof failed"));
    }
    if output.stdout.len() > 1024 * 1024 {
        return Ok(true);
    }
    let current = std::process::id();
    if output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|line| std::str::from_utf8(line).ok()?.parse::<u32>().ok())
        .any(|pid| pid != current)
    {
        return Ok(true);
    }
    let own = Command::new("/usr/sbin/lsof")
        .args([
            "-n",
            "-P",
            "-a",
            "-p",
            &current.to_string(),
            "-F",
            "tn",
            "+D",
        ])
        .arg(view)
        .output()?;
    if !own.status.success() && own.status.code() != Some(1) {
        return Err(std::io::Error::other("lsof failed"));
    }
    if own.stdout.len() > 1024 * 1024 {
        return Ok(true);
    }
    Ok(own
        .stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|line| line.strip_prefix(b"t"))
        .any(|kind| kind != b"DIR"))
}

#[cfg(not(target_os = "macos"))]
fn escaped_writer_present(_view: &Path) -> std::io::Result<bool> {
    Ok(false)
}
