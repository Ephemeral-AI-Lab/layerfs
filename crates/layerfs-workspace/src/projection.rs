use crate::worker::WorkspaceWorker;
use crate::{Kind, NodeId, Workspace, WorkspaceError, WorkspacePlacement, WorkspaceResult, ROOT};
use layerfs_fuse::{FilesystemPort, PortError};
use layerfs_materialization::{
    Attr as MaterializedAttr, CaptureSink, Entry, Kind as MaterializedKind, MaterializationError,
    MaterializationSource, NodeId as MaterializedNode, Result as MaterializedResult,
};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};

pub(crate) enum ProjectionHandle {
    Materialized(PathBuf),
    Docker(Box<crate::docker::DockerProjection>),
    #[cfg(all(target_os = "linux", feature = "host-fuse"))]
    Fuse(layerfs_fuse::HostMount),
}

pub(crate) fn attach(worker: &Arc<WorkspaceWorker>) -> WorkspaceResult<ProjectionHandle> {
    if let WorkspacePlacement::Container { container_id, root } = &worker.request.placement {
        if worker.projection != crate::WorkspaceProjection::Fuse {
            return Err(WorkspaceError::InvalidPlacement);
        }
        let port: Arc<dyn FilesystemPort> = Arc::new(FuseView(Arc::downgrade(worker)));
        let runtime = worker
            .workspace
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?
            .spool
            .parent()
            .ok_or(WorkspaceError::InvalidPlacement)?
            .to_owned();
        return crate::docker::DockerProjection::attach(
            worker.id,
            container_id.clone(),
            root.clone(),
            port,
            &runtime,
        )
        .map(|projection| ProjectionHandle::Docker(Box::new(projection)));
    }
    let WorkspacePlacement::Host { root } = &worker.request.placement else {
        unreachable!()
    };
    match worker.projection {
        crate::WorkspaceProjection::Materialize => {
            let source = MaterializedView(Arc::downgrade(worker));
            materialize_atomic(&source, root)?;
            Ok(ProjectionHandle::Materialized(root.clone()))
        }
        crate::WorkspaceProjection::Fuse => {
            #[cfg(all(target_os = "linux", feature = "host-fuse"))]
            {
                std::fs::create_dir_all(root)?;
                let port: Arc<dyn FilesystemPort> = Arc::new(FuseView(Arc::downgrade(worker)));
                let mount = layerfs_fuse::mount_host(port, root, 0, 0)?;
                Ok(ProjectionHandle::Fuse(mount))
            }
            #[cfg(not(all(target_os = "linux", feature = "host-fuse")))]
            Err(WorkspaceError::InvalidPlacement)
        }
    }
}

pub(crate) fn capture(worker: &Arc<WorkspaceWorker>) -> WorkspaceResult<()> {
    let root = {
        let handle = worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        match handle.as_ref() {
            Some(ProjectionHandle::Materialized(root)) => Some(root.clone()),
            Some(ProjectionHandle::Docker(projection)) if !projection.healthy() => {
                let detail = projection.failure().map_or_else(
                    || "unknown".to_owned(),
                    |(request, error)| format!("{request} {error:?}"),
                );
                return Err(WorkspaceError::Io(std::io::Error::other(format!(
                    "FUSE projection write failed: {detail}"
                ))));
            }
            _ => None,
        }
    };
    let Some(root) = root else {
        layerfs_storage::note_workspace_capture(0, 0);
        return Ok(());
    };
    let changed =
        !layerfs_materialization::matches(&MaterializedView(Arc::downgrade(worker)), &root)
            .map_err(materialization_error)?;
    let mut workspace = worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    static CAPTURE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let capture_spool = workspace
        .spool
        .parent()
        .ok_or(WorkspaceError::InvalidPlacement)?
        .join(format!(
            "capture-spool-{}",
            CAPTURE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
    if capture_spool.exists() {
        std::fs::remove_dir_all(&capture_spool)?;
    }
    let mut captured = workspace.clean_copy(&capture_spool)?;
    let mut sink = WorkspaceCapture {
        workspace: &mut captured,
        files: 0,
        bytes: 0,
    };
    if let Err(error) = layerfs_materialization::capture(&root, &mut sink) {
        let _ = std::fs::remove_dir_all(capture_spool);
        return Err(materialization_error(error));
    }
    layerfs_storage::note_workspace_capture(sink.files, sink.bytes);
    captured.mutation_generation = workspace
        .mutation_generation
        .checked_add(u64::from(changed))
        .ok_or(WorkspaceError::Storage(
            layerfs_storage::StorageError::Integrity("Workspace mutation generation"),
        ))?;
    captured.mutation_paths = workspace.mutation_paths.clone();
    captured.resolution = workspace.resolution.take();
    let mut previous = std::mem::replace(&mut *workspace, captured);
    previous.discard()?;
    Ok(())
}

pub(crate) fn pause(worker: &WorkspaceWorker) -> WorkspaceResult<()> {
    let handle = worker
        .projection_handle
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    if let Some(ProjectionHandle::Docker(projection)) = handle.as_ref() {
        projection.pause()?;
    }
    Ok(())
}

pub(crate) fn resume(worker: &WorkspaceWorker) -> WorkspaceResult<()> {
    let handle = worker
        .projection_handle
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    if let Some(ProjectionHandle::Docker(projection)) = handle.as_ref() {
        projection.resume()?;
    }
    Ok(())
}

pub(crate) fn make_read_only(worker: &WorkspaceWorker) -> WorkspaceResult<()> {
    let root = {
        let handle = worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        match handle.as_ref() {
            Some(ProjectionHandle::Materialized(root)) => Some(root.clone()),
            Some(ProjectionHandle::Docker(projection)) => {
                projection.make_read_only()?;
                None
            }
            _ => None,
        }
    };
    if let Some(root) = root {
        read_only_tree(&root)?;
    }
    Ok(())
}

pub(crate) fn is_dirty(worker: &Arc<WorkspaceWorker>) -> WorkspaceResult<bool> {
    let root = {
        let handle = worker
            .projection_handle
            .lock()
            .map_err(|_| WorkspaceError::WorkspaceBusy)?;
        match handle.as_ref() {
            Some(ProjectionHandle::Materialized(root)) => Some(root.clone()),
            _ => None,
        }
    };
    if let Some(root) = root {
        return layerfs_materialization::matches(&MaterializedView(Arc::downgrade(worker)), &root)
            .map(|matches| !matches)
            .map_err(materialization_error);
    }
    Ok(worker
        .workspace
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?
        .mutation_generation
        != 0)
}

pub(crate) fn end(worker: &WorkspaceWorker) -> WorkspaceResult<()> {
    let mut handle = worker
        .projection_handle
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)?;
    if let Some(ProjectionHandle::Materialized(root)) = handle.as_mut() {
        cleanup_materialized(root)?;
        *handle = None;
        return Ok(());
    }
    #[cfg(all(target_os = "linux", feature = "host-fuse"))]
    if let Some(ProjectionHandle::Fuse(mount)) = handle.as_mut() {
        mount.unmount()?;
        *handle = None;
        return Ok(());
    }
    if let Some(ProjectionHandle::Docker(projection)) = handle.as_mut() {
        projection.end()?;
        *handle = None;
    }
    Ok(())
}

fn cleanup_materialized(root: &mut PathBuf) -> WorkspaceResult<()> {
    if !root.exists() {
        return Ok(());
    }
    static CLEANUP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let quarantine = root.with_file_name(format!(
        ".layerfs-cleanup-{}-{}",
        std::process::id(),
        CLEANUP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    if let Err(error) = std::fs::rename(&*root, &quarantine) {
        let _ = read_only_tree(root);
        return Err(error.into());
    }
    *root = quarantine;
    if let Err(error) = writable_tree(root).and_then(|()| std::fs::remove_dir_all(&*root)) {
        let _ = read_only_tree(root);
        return Err(error.into());
    }
    Ok(())
}

fn materialize_atomic(
    source: &dyn MaterializationSource,
    destination: &Path,
) -> WorkspaceResult<()> {
    let parent = destination
        .parent()
        .ok_or(WorkspaceError::InvalidPlacement)?;
    std::fs::create_dir_all(parent)?;
    static STAGE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let stage = parent.join(format!(
        ".layerfs-stage-{}-{}",
        std::process::id(),
        STAGE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let materialized = layerfs_materialization::materialize(source, &stage);
    if let Err(error) = materialized {
        discard_stage(&stage);
        return Err(materialization_error(error));
    }
    let published = (|| {
        if destination.exists() {
            if !destination.is_dir() || std::fs::read_dir(destination)?.next().is_some() {
                return Err(WorkspaceError::InvalidPlacement);
            }
            std::fs::remove_dir(destination)?;
        }
        std::fs::rename(&stage, destination)?;
        Ok(())
    })();
    if published.is_err() {
        discard_stage(&stage);
    }
    published
}

fn discard_stage(stage: &Path) {
    if stage.exists() {
        let _ = writable_tree(stage);
        if std::fs::remove_dir_all(stage).is_err() {
            let _ = read_only_tree(stage);
        }
    }
}

pub(crate) fn refresh(worker: &Arc<WorkspaceWorker>) -> WorkspaceResult<()> {
    end(worker)?;
    let handle = attach(worker)?;
    *worker
        .projection_handle
        .lock()
        .map_err(|_| WorkspaceError::WorkspaceBusy)? = Some(handle);
    Ok(())
}

struct FuseView(Weak<WorkspaceWorker>);

impl FuseView {
    fn worker(&self) -> Result<Arc<WorkspaceWorker>, PortError> {
        self.0.upgrade().ok_or(PortError::Io)
    }

    fn with<T>(
        &self,
        operation: impl FnOnce(&mut Workspace) -> layerfs_storage::Result<T>,
    ) -> Result<T, PortError> {
        let worker = self.worker()?;
        let _callback = worker.enter_callback().map_err(workspace_port_error)?;
        let mut workspace = worker.workspace.lock().map_err(|_| PortError::Busy)?;
        operation(&mut workspace).map_err(storage_port_error)
    }
}

impl FilesystemPort for FuseView {
    fn lookup(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
    ) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        self.with(|workspace| workspace.lookup(NodeId(parent.0), name))
            .map(fuse_attr)
    }

    fn attr(&self, node: layerfs_fuse::NodeId) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        self.with(|workspace| workspace.attr(NodeId(node.0)))
            .map(fuse_attr)
    }

    fn readlink(&self, node: layerfs_fuse::NodeId) -> layerfs_fuse::PortResult<Vec<u8>> {
        self.with(|workspace| workspace.readlink(NodeId(node.0)))
    }

    fn readdir(
        &self,
        node: layerfs_fuse::NodeId,
    ) -> layerfs_fuse::PortResult<Vec<(layerfs_fuse::NodeId, layerfs_fuse::Kind, Vec<u8>)>> {
        self.with(|workspace| workspace.readdir(NodeId(node.0)))
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|(node, kind, name)| (layerfs_fuse::NodeId(node.0), fuse_kind(kind), name))
                    .collect()
            })
    }

    fn create_file(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
        mode: u32,
    ) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        self.with(|workspace| workspace.create_file(NodeId(parent.0), name, mode))
            .map(fuse_attr)
    }

    fn create_file_open(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
        mode: u32,
    ) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        let worker = self.worker()?;
        let _callback = worker.enter_callback().map_err(workspace_port_error)?;
        let attr = {
            let mut workspace = worker.workspace.lock().map_err(|_| PortError::Busy)?;
            let attr = workspace
                .create_file(NodeId(parent.0), name, mode)
                .map_err(storage_port_error)?;
            workspace
                .pin(attr.node, false)
                .map_err(storage_port_error)?;
            attr
        };
        worker.note_writer(true).map_err(workspace_port_error)?;
        Ok(fuse_attr(attr))
    }

    fn reserve_nodes(&self, count: u32) -> layerfs_fuse::PortResult<layerfs_fuse::NodeId> {
        self.with(|workspace| workspace.reserve_nodes(count))
            .map(|node| layerfs_fuse::NodeId(node.0))
    }

    fn create_file_open_reserved(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
        mode: u32,
        node: layerfs_fuse::NodeId,
    ) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        let worker = self.worker()?;
        let _callback = worker.enter_callback().map_err(workspace_port_error)?;
        let attr = {
            let mut workspace = worker.workspace.lock().map_err(|_| PortError::Busy)?;
            let attr = workspace
                .create_file_reserved(NodeId(parent.0), name, mode, NodeId(node.0))
                .map_err(storage_port_error)?;
            workspace
                .pin(attr.node, false)
                .map_err(storage_port_error)?;
            attr
        };
        worker.note_writer(true).map_err(workspace_port_error)?;
        Ok(fuse_attr(attr))
    }

    fn create_files_closed_reserved(
        &self,
        entries: &[(
            layerfs_fuse::NodeId,
            Vec<u8>,
            u32,
            layerfs_fuse::NodeId,
            Vec<(u64, Vec<u8>)>,
            Option<(i64, u32)>,
        )],
    ) -> layerfs_fuse::PortResult<()> {
        let worker = self.worker()?;
        let _callback = worker.enter_callback().map_err(workspace_port_error)?;
        let mut workspace = worker.workspace.lock().map_err(|_| PortError::Busy)?;
        for (parent, name, mode, node, writes, mtime) in entries {
            let attr = workspace
                .create_file_reserved(NodeId(parent.0), name, *mode, NodeId(node.0))
                .map_err(storage_port_error)?;
            workspace
                .pin(attr.node, false)
                .map_err(storage_port_error)?;
            for (offset, bytes) in writes {
                workspace
                    .write(attr.node, *offset, bytes)
                    .map_err(storage_port_error)?;
            }
            if let Some((seconds, nanos)) = mtime {
                workspace
                    .set_mtime(attr.node, *seconds, *nanos)
                    .map_err(storage_port_error)?;
            }
            workspace.unpin(attr.node).map_err(storage_port_error)?;
        }
        Ok(())
    }

    fn mkdir(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
        mode: u32,
    ) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        self.with(|workspace| workspace.mkdir(NodeId(parent.0), name, mode))
            .map(fuse_attr)
    }

    fn mkdir_reserved(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
        mode: u32,
        node: layerfs_fuse::NodeId,
    ) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        self.with(|workspace| {
            workspace.mkdir_reserved(NodeId(parent.0), name, mode, NodeId(node.0))
        })
        .map(fuse_attr)
    }

    fn symlink(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
        target: Vec<u8>,
    ) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        self.with(|workspace| workspace.symlink(NodeId(parent.0), name, target))
            .map(fuse_attr)
    }

    fn link(
        &self,
        node: layerfs_fuse::NodeId,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
    ) -> layerfs_fuse::PortResult<layerfs_fuse::Attr> {
        self.with(|workspace| workspace.link(NodeId(node.0), NodeId(parent.0), name))
            .map(fuse_attr)
    }

    fn unlink(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
        directory: bool,
    ) -> layerfs_fuse::PortResult<()> {
        self.with(|workspace| workspace.unlink(NodeId(parent.0), name, directory))
    }

    fn unlink_batch(
        &self,
        entries: &[(layerfs_fuse::NodeId, Vec<u8>)],
    ) -> layerfs_fuse::PortResult<()> {
        self.with(|workspace| {
            for (parent, name) in entries {
                workspace.unlink(NodeId(parent.0), name, false)?;
            }
            Ok(())
        })
    }

    fn rename(
        &self,
        parent: layerfs_fuse::NodeId,
        name: &[u8],
        new_parent: layerfs_fuse::NodeId,
        new_name: &[u8],
        no_replace: bool,
    ) -> layerfs_fuse::PortResult<()> {
        self.with(|workspace| {
            workspace.rename(
                NodeId(parent.0),
                name,
                NodeId(new_parent.0),
                new_name,
                no_replace,
            )
        })
    }

    fn pin(
        &self,
        node: layerfs_fuse::NodeId,
        truncate: bool,
        writable: bool,
    ) -> layerfs_fuse::PortResult<()> {
        let worker = self.worker()?;
        let _callback = worker.enter_callback().map_err(workspace_port_error)?;
        worker
            .workspace
            .lock()
            .map_err(|_| PortError::Busy)?
            .pin(NodeId(node.0), truncate)
            .map_err(storage_port_error)?;
        if writable {
            worker.note_writer(true).map_err(workspace_port_error)?;
        }
        Ok(())
    }

    fn unpin(&self, node: layerfs_fuse::NodeId, writable: bool) -> layerfs_fuse::PortResult<()> {
        let worker = self.worker()?;
        let _callback = worker.enter_callback().map_err(workspace_port_error)?;
        worker
            .workspace
            .lock()
            .map_err(|_| PortError::Busy)?
            .unpin(NodeId(node.0))
            .map_err(storage_port_error)?;
        if writable {
            worker.note_writer(false).map_err(workspace_port_error)?;
        }
        Ok(())
    }

    fn read(
        &self,
        node: layerfs_fuse::NodeId,
        offset: u64,
        size: usize,
    ) -> layerfs_fuse::PortResult<Vec<u8>> {
        self.with(|workspace| workspace.read(NodeId(node.0), offset, size))
    }

    fn write(
        &self,
        node: layerfs_fuse::NodeId,
        offset: u64,
        bytes: &[u8],
    ) -> layerfs_fuse::PortResult<usize> {
        self.with(|workspace| workspace.write(NodeId(node.0), offset, bytes))
    }

    fn write_zero(
        &self,
        node: layerfs_fuse::NodeId,
        offset: u64,
        len: usize,
    ) -> layerfs_fuse::PortResult<usize> {
        self.with(|workspace| workspace.write_zero(NodeId(node.0), offset, len))
    }

    fn truncate(&self, node: layerfs_fuse::NodeId, size: u64) -> layerfs_fuse::PortResult<()> {
        self.with(|workspace| workspace.truncate(NodeId(node.0), size))
    }

    fn chmod(&self, node: layerfs_fuse::NodeId, mode: u32) -> layerfs_fuse::PortResult<()> {
        self.with(|workspace| workspace.chmod(NodeId(node.0), mode))
    }

    fn set_mtime(
        &self,
        node: layerfs_fuse::NodeId,
        seconds: i64,
        nanos: u32,
    ) -> layerfs_fuse::PortResult<()> {
        self.with(|workspace| workspace.set_mtime(NodeId(node.0), seconds, nanos))
    }

    fn fsync(&self, node: Option<layerfs_fuse::NodeId>) -> layerfs_fuse::PortResult<()> {
        self.with(|workspace| workspace.fsync(node.map(|node| NodeId(node.0))))
    }
}

struct MaterializedView(Weak<WorkspaceWorker>);

impl MaterializedView {
    fn with<T>(
        &self,
        operation: impl FnOnce(&mut Workspace) -> layerfs_storage::Result<T>,
    ) -> MaterializedResult<T> {
        let worker = self
            .0
            .upgrade()
            .ok_or(MaterializationError::Port("Workspace ended"))?;
        let mut workspace = worker
            .workspace
            .lock()
            .map_err(|_| MaterializationError::Port("Workspace busy"))?;
        operation(&mut workspace).map_err(|_| MaterializationError::Port("Workspace"))
    }
}

impl MaterializationSource for MaterializedView {
    fn root(&self) -> MaterializedAttr {
        self.with(|workspace| workspace.attr(ROOT))
            .map(materialized_attr)
            .expect("Workspace root")
    }

    fn entries(&self, node: MaterializedNode) -> MaterializedResult<Vec<Entry>> {
        self.with(|workspace| workspace.readdir(NodeId(node.0)))
            .map(|entries| {
                entries
                    .into_iter()
                    .filter(|(_, _, name)| name != b"." && name != b"..")
                    .map(|(node, _, name)| {
                        self.with(|workspace| workspace.attr(node))
                            .map(|attr| Entry {
                                name,
                                attr: materialized_attr(attr),
                            })
                    })
                    .collect::<MaterializedResult<Vec<_>>>()
            })?
    }

    fn read(
        &self,
        node: MaterializedNode,
        sink: &mut dyn std::io::Write,
    ) -> MaterializedResult<()> {
        let mut offset = 0;
        loop {
            let bytes =
                self.with(|workspace| workspace.read(NodeId(node.0), offset, 1024 * 1024))?;
            if bytes.is_empty() {
                return Ok(());
            }
            sink.write_all(&bytes)?;
            offset += bytes.len() as u64;
        }
    }

    fn readlink(&self, node: MaterializedNode) -> MaterializedResult<Vec<u8>> {
        self.with(|workspace| workspace.readlink(NodeId(node.0)))
    }
}

struct WorkspaceCapture<'a> {
    workspace: &'a mut Workspace,
    files: u64,
    bytes: u64,
}

impl CaptureSink for WorkspaceCapture<'_> {
    fn reset(&mut self, mode: u32, seconds: i64, nanos: u32) -> MaterializedResult<()> {
        clear_directory(self.workspace, ROOT)?;
        self.workspace
            .chmod(ROOT, mode)
            .and_then(|_| self.workspace.set_mtime(ROOT, seconds, nanos))
            .map_err(|_| MaterializationError::Port("Workspace"))
    }

    fn directory(
        &mut self,
        path: &layerfs_content::CanonicalPath,
        mode: u32,
        seconds: i64,
        nanos: u32,
    ) -> MaterializedResult<()> {
        let (parent, name) = parent(self.workspace, path)?;
        let attr = self
            .workspace
            .mkdir(parent, &name, mode)
            .map_err(|_| MaterializationError::Port("Workspace"))?;
        self.workspace
            .set_mtime(attr.node, seconds, nanos)
            .map_err(|_| MaterializationError::Port("Workspace"))
    }

    fn file(
        &mut self,
        path: &layerfs_content::CanonicalPath,
        source: &mut dyn Read,
        mode: u32,
        seconds: i64,
        nanos: u32,
    ) -> MaterializedResult<()> {
        let (parent, name) = parent(self.workspace, path)?;
        let attr = self
            .workspace
            .create_file(parent, &name, mode)
            .map_err(|_| MaterializationError::Port("Workspace"))?;
        let mut offset = 0;
        let mut bytes = [0; 64 * 1024];
        loop {
            let read = source.read(&mut bytes)?;
            if read == 0 {
                break;
            }
            self.workspace
                .write(attr.node, offset, &bytes[..read])
                .map_err(|_| MaterializationError::Port("Workspace"))?;
            offset += read as u64;
        }
        self.files = self.files.saturating_add(1);
        self.bytes = self.bytes.saturating_add(offset);
        self.workspace
            .set_mtime(attr.node, seconds, nanos)
            .map_err(|_| MaterializationError::Port("Workspace"))
    }

    fn symlink(
        &mut self,
        path: &layerfs_content::CanonicalPath,
        target: Vec<u8>,
        seconds: i64,
        nanos: u32,
    ) -> MaterializedResult<()> {
        let (parent, name) = parent(self.workspace, path)?;
        let attr = self
            .workspace
            .symlink(parent, &name, target)
            .map_err(|_| MaterializationError::Port("Workspace"))?;
        self.workspace
            .set_mtime(attr.node, seconds, nanos)
            .map_err(|_| MaterializationError::Port("Workspace"))
    }

    fn hard_link(
        &mut self,
        source: &layerfs_content::CanonicalPath,
        target: &layerfs_content::CanonicalPath,
    ) -> MaterializedResult<()> {
        let source = node(self.workspace, source)?;
        let (parent, name) = parent(self.workspace, target)?;
        self.workspace
            .link(source, parent, &name)
            .map(drop)
            .map_err(|_| MaterializationError::Port("Workspace"))
    }
}

fn clear_directory(workspace: &mut Workspace, directory: NodeId) -> MaterializedResult<()> {
    let entries = workspace
        .readdir(directory)
        .map_err(|_| MaterializationError::Port("Workspace"))?;
    for (node, kind, name) in entries
        .into_iter()
        .filter(|(_, _, name)| name != b"." && name != b"..")
    {
        if kind == Kind::Directory {
            clear_directory(workspace, node)?;
        }
        workspace
            .unlink(directory, &name, kind == Kind::Directory)
            .map_err(|_| MaterializationError::Port("Workspace"))?;
    }
    Ok(())
}

fn node(
    workspace: &mut Workspace,
    path: &layerfs_content::CanonicalPath,
) -> MaterializedResult<NodeId> {
    let mut node = ROOT;
    for component in path.as_bytes().split(|byte| *byte == b'/') {
        if !component.is_empty() {
            node = workspace
                .lookup(node, component)
                .map_err(|_| MaterializationError::Port("Workspace path"))?
                .node;
        }
    }
    Ok(node)
}

fn parent(
    workspace: &mut Workspace,
    path: &layerfs_content::CanonicalPath,
) -> MaterializedResult<(NodeId, Vec<u8>)> {
    let bytes = path.as_bytes();
    let split = bytes.iter().rposition(|byte| *byte == b'/');
    let (parent_path, name) = match split {
        Some(index) => (&bytes[..index], &bytes[index + 1..]),
        None => (&[][..], bytes),
    };
    let parent = layerfs_content::CanonicalPath::from_bytes(parent_path)?;
    Ok((node(workspace, &parent)?, name.to_vec()))
}

fn fuse_attr(attr: crate::Attr) -> layerfs_fuse::Attr {
    layerfs_fuse::Attr {
        node: layerfs_fuse::NodeId(attr.node.0),
        size: attr.size,
        kind: fuse_kind(attr.kind),
        mode: attr.mode,
        links: attr.links,
        mtime_seconds: attr.mtime_seconds,
        mtime_nanoseconds: attr.mtime_nanoseconds,
    }
}

fn fuse_kind(kind: Kind) -> layerfs_fuse::Kind {
    match kind {
        Kind::File => layerfs_fuse::Kind::File,
        Kind::Directory => layerfs_fuse::Kind::Directory,
        Kind::Symlink => layerfs_fuse::Kind::Symlink,
    }
}

fn materialized_attr(attr: crate::Attr) -> MaterializedAttr {
    MaterializedAttr {
        node: MaterializedNode(attr.node.0),
        kind: match attr.kind {
            Kind::File => MaterializedKind::File,
            Kind::Directory => MaterializedKind::Directory,
            Kind::Symlink => MaterializedKind::Symlink,
        },
        mode: attr.mode,
        mtime_seconds: attr.mtime_seconds,
        mtime_nanoseconds: attr.mtime_nanoseconds,
    }
}

fn workspace_port_error(error: WorkspaceError) -> PortError {
    match error {
        WorkspaceError::WorkspaceBusy => PortError::Busy,
        WorkspaceError::ReadOnly => PortError::ReadOnly,
        _ => PortError::Io,
    }
}

fn storage_port_error(error: layerfs_storage::StorageError) -> PortError {
    match error {
        layerfs_storage::StorageError::NotFound(_) => PortError::NotFound,
        layerfs_storage::StorageError::InvalidInput("directory not empty") => PortError::NotEmpty,
        layerfs_storage::StorageError::InvalidInput("name exists") => PortError::Exists,
        layerfs_storage::StorageError::InvalidInput("workspace spool limit") => PortError::NoSpace,
        layerfs_storage::StorageError::InvalidInput("workspace inactive") => PortError::ReadOnly,
        layerfs_storage::StorageError::InvalidInput(_) => PortError::Invalid,
        _ => PortError::Io,
    }
}

fn materialization_error(error: MaterializationError) -> WorkspaceError {
    match error {
        MaterializationError::Io(error) => WorkspaceError::Io(error),
        _ => WorkspaceError::InvalidPlacement,
    }
}

fn read_only_tree(root: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for entry in std::fs::read_dir(root)? {
        let path = entry?.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_dir() {
            read_only_tree(&path)?;
        }
        if !metadata.file_type().is_symlink() {
            let mode = metadata.permissions().mode() & !0o222;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
        }
    }
    let metadata = std::fs::metadata(root)?;
    std::fs::set_permissions(
        root,
        std::fs::Permissions::from_mode(metadata.permissions().mode() & !0o222),
    )
}

fn writable_tree(root: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.file_type().is_symlink() {
        std::fs::set_permissions(
            root,
            std::fs::Permissions::from_mode(metadata.permissions().mode() | 0o700),
        )?;
    }
    if metadata.file_type().is_dir() {
        for entry in std::fs::read_dir(root)? {
            writable_tree(&entry?.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingSource;
    struct EmptySource;

    impl MaterializationSource for FailingSource {
        fn root(&self) -> MaterializedAttr {
            MaterializedAttr {
                node: MaterializedNode(1),
                kind: MaterializedKind::Directory,
                mode: 0o755,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            }
        }

        fn entries(&self, node: MaterializedNode) -> MaterializedResult<Vec<Entry>> {
            if node != MaterializedNode(1) {
                return Err(MaterializationError::Port("directory"));
            }
            Ok(vec![Entry {
                name: b"partial".to_vec(),
                attr: MaterializedAttr {
                    node: MaterializedNode(2),
                    kind: MaterializedKind::File,
                    mode: 0o644,
                    mtime_seconds: 0,
                    mtime_nanoseconds: 0,
                },
            }])
        }

        fn read(
            &self,
            node: MaterializedNode,
            sink: &mut dyn std::io::Write,
        ) -> MaterializedResult<()> {
            assert_eq!(node, MaterializedNode(2));
            sink.write_all(b"partial")?;
            Err(MaterializationError::Port("injected read failure"))
        }

        fn readlink(&self, _: MaterializedNode) -> MaterializedResult<Vec<u8>> {
            Err(MaterializationError::Port("symlink"))
        }
    }

    impl MaterializationSource for EmptySource {
        fn root(&self) -> MaterializedAttr {
            FailingSource.root()
        }

        fn entries(&self, node: MaterializedNode) -> MaterializedResult<Vec<Entry>> {
            (node == MaterializedNode(1))
                .then(Vec::new)
                .ok_or(MaterializationError::Port("directory"))
        }

        fn read(&self, _: MaterializedNode, _: &mut dyn std::io::Write) -> MaterializedResult<()> {
            Err(MaterializationError::Port("file"))
        }

        fn readlink(&self, _: MaterializedNode) -> MaterializedResult<Vec<u8>> {
            Err(MaterializationError::Port("symlink"))
        }
    }

    #[test]
    fn failed_materialization_never_publishes_or_leaves_a_stage() {
        let parent = std::env::temp_dir().join(format!(
            "layerfs-materialize-atomic-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&parent).unwrap();
        let destination = parent.join("workspace");
        assert!(materialize_atomic(&FailingSource, &destination).is_err());
        assert!(!destination.exists());
        assert!(std::fs::read_dir(&parent).unwrap().next().is_none());
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn failed_cleanup_moves_the_projection_off_placement_and_retains_retry_path() {
        let parent = std::env::temp_dir().join(format!(
            "layerfs-materialize-cleanup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&parent).unwrap();
        let placement = parent.join("workspace");
        std::fs::write(&placement, b"not a projection directory").unwrap();
        let mut retry = placement.clone();
        assert!(cleanup_materialized(&mut retry).is_err());
        assert!(!placement.exists());
        assert_ne!(retry, placement);
        assert!(std::fs::symlink_metadata(&retry).is_ok());
        std::fs::remove_file(retry).unwrap();
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn failed_publish_removes_the_complete_stage() {
        let parent = std::env::temp_dir().join(format!(
            "layerfs-materialize-publish-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let destination = parent.join("workspace");
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("occupied"), b"keep").unwrap();
        assert!(materialize_atomic(&EmptySource, &destination).is_err());
        assert_eq!(
            std::fs::read(destination.join("occupied")).unwrap(),
            b"keep"
        );
        assert_eq!(std::fs::read_dir(&parent).unwrap().count(), 1);
        std::fs::remove_dir_all(parent).unwrap();
    }
}
