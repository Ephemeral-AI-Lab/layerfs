pub(crate) const DIRECTORY_PAGE_ENTRIES: usize = 128;

use std::sync::Arc;

pub use layerfs_workspace_core::{Attr, Kind, NodeId, ROOT};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortError {
    NotFound,
    NotEmpty,
    Exists,
    NoSpace,
    ReadOnly,
    Busy,
    Invalid,
    Io,
}

pub type PortResult<T> = Result<T, PortError>;

/// Actual kernel callback observations; independent of proxy cache hits.
#[derive(Clone, Copy, Debug)]
pub enum KernelOperation {
    Lookup,
    Getattr,
    Setattr,
    Readlink,
    Mknod,
    Mkdir,
    Unlink,
    Rmdir,
    Symlink,
    Rename,
    Link,
    Open,
    Read,
    Write,
    Flush,
    Release,
    Fsync,
    Opendir,
    Readdir,
    Readdirplus,
    Releasedir,
    Fsyncdir,
    Statfs,
    Access,
    Create,
}

#[derive(Default)]
pub struct CallbackGuard {
    #[cfg(feature = "live")]
    pub(crate) _gate: Option<tokio::sync::OwnedRwLockReadGuard<()>>,
    #[cfg(feature = "live")]
    pub(crate) _admission: Option<crate::live_runtime::RequestAdmission>,
}

#[cfg(feature = "live")]
pub type PortFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = PortResult<T>> + Send + 'a>>;

#[cfg(feature = "live")]
#[derive(Default)]
pub struct KernelReferences {
    pub(crate) owner: Option<KernelOwner>,
    pub(crate) nodes: Vec<NodeId>,
}

#[cfg(feature = "live")]
pub(crate) enum KernelOwner {
    Live(crate::live_owner::LiveOwner),
    Host(crate::host_client::ReferenceCleanup),
}
#[cfg(feature = "live")]
impl From<crate::live_owner::LiveOwner> for KernelOwner {
    fn from(owner: crate::live_owner::LiveOwner) -> Self {
        Self::Live(owner)
    }
}

#[cfg(feature = "live")]
impl KernelReferences {
    /// Count only nondot READDIRPLUS entries actually added to the reply buffer.
    pub fn release_unemitted(&mut self, emitted: usize) -> PortResult<()> {
        if emitted > self.nodes.len() {
            return Err(PortError::Invalid);
        }
        if let Some(KernelOwner::Host(cleanup)) = &mut self.owner {
            // The one reserved cleanup job is dispatched by submitted/Drop.
            // No await/cancellation point separates this update from the reply.
            cleanup.emitted = emitted;
            return Ok(());
        }
        while self.nodes.len() > emitted {
            let node = self.nodes.pop().unwrap();
            if let Some(KernelOwner::Live(owner)) = &self.owner {
                owner.kernel_forget(node, 1)?;
            }
        }
        Ok(())
    }

    /// The reply was attempted; FORGET or drained detach now owns cleanup.
    pub fn submitted(mut self) {
        if let Some(KernelOwner::Host(cleanup)) = self.owner.take() {
            cleanup.release(std::mem::take(&mut self.nodes), true);
            return;
        }
        self.nodes.clear();
        self.owner.take();
    }
}

#[cfg(feature = "live")]
impl Drop for KernelReferences {
    fn drop(&mut self) {
        match self.owner.take() {
            Some(KernelOwner::Host(cleanup)) => {
                cleanup.release(std::mem::take(&mut self.nodes), false);
                return;
            }
            other => self.owner = other,
        }
        while let Some(node) = self.nodes.pop() {
            if let Some(KernelOwner::Live(owner)) = &self.owner {
                let _ = owner.kernel_forget(node, 1);
            }
        }
    }
}

#[cfg(feature = "live")]
pub enum KernelEntry {
    Lookup,
    Create { mode: u32 },
    Mkdir { mode: u32 },
    Symlink { target: Vec<u8> },
    Link { node: NodeId },
}

#[cfg(feature = "live")]
pub type DirectoryPage = Vec<(u64, Attr, Vec<u8>)>;

pub trait FilesystemPort: Send + Sync {
    #[cfg(feature = "live")]
    fn supports_kernel_lifetime(&self) -> bool {
        false
    }
    #[cfg(feature = "live")]
    fn validate_kernel_open(&self, _node: NodeId) -> PortResult<()> {
        Err(PortError::Invalid)
    }
    #[cfg(feature = "live")]
    fn prepare_kernel_open(&self, node: NodeId, _writable: bool) -> PortFuture<'_, ()> {
        Box::pin(async move { self.validate_kernel_open(node) })
    }
    #[cfg(feature = "live")]
    fn kernel_entry_async<'a>(
        &'a self,
        _parent: NodeId,
        _name: &'a [u8],
        _operation: KernelEntry,
    ) -> PortFuture<'a, (Attr, KernelReferences)> {
        Box::pin(async { Err(PortError::Invalid) })
    }
    #[cfg(feature = "live")]
    fn kernel_lookup_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
    ) -> PortFuture<'a, (Attr, KernelReferences)> {
        self.kernel_entry_async(parent, name, KernelEntry::Lookup)
    }
    #[cfg(feature = "live")]
    fn kernel_directory_page_async<'a>(
        &'a self,
        _node: NodeId,
        _after: u64,
    ) -> PortFuture<'a, (DirectoryPage, KernelReferences)> {
        Box::pin(async { Err(PortError::Invalid) })
    }
    #[cfg(feature = "live")]
    fn kernel_forget(&self, _node: NodeId, _nlookup: u64) -> PortResult<()> {
        Err(PortError::Invalid)
    }
    #[cfg(feature = "live")]
    fn kernel_detach(&self) -> PortResult<()> {
        Ok(())
    }

    #[cfg(feature = "live")]
    fn directory_page_async<'a>(
        &'a self,
        node: NodeId,
        after: u64,
    ) -> PortFuture<'a, Vec<(u64, Attr, Vec<u8>)>> {
        Box::pin(async move {
            self.readdirplus_page_async(node, after as usize)
                .await
                .map(|entries| {
                    entries
                        .into_iter()
                        .enumerate()
                        .map(|(index, (attr, name))| (after + index as u64 + 1, attr, name))
                        .collect()
                })
        })
    }
    #[cfg(feature = "live")]
    fn callback_gate(
        &self,
        _operation: KernelOperation,
        _writeback: bool,
    ) -> PortFuture<'_, Option<tokio::sync::OwnedRwLockReadGuard<()>>> {
        Box::pin(async { Ok(None) })
    }
    #[cfg(feature = "live")]
    fn lookup_async<'a>(&'a self, parent: NodeId, name: &'a [u8]) -> PortFuture<'a, Attr> {
        Box::pin(async move { self.lookup(parent, name) })
    }
    #[cfg(feature = "live")]
    fn attr_async(&self, node: NodeId) -> PortFuture<'_, Attr> {
        Box::pin(async move { self.attr(node) })
    }
    #[cfg(feature = "live")]
    fn readlink_async(&self, node: NodeId) -> PortFuture<'_, Vec<u8>> {
        Box::pin(async move { self.readlink(node) })
    }
    #[cfg(feature = "live")]
    fn unpin_async(&self, node: NodeId, writable: bool) -> PortFuture<'_, ()> {
        Box::pin(async move { self.unpin(node, writable) })
    }
    #[cfg(feature = "live")]
    fn pin_directory_async(&self, node: NodeId) -> PortFuture<'_, ()> {
        Box::pin(async move { self.pin_directory(node) })
    }
    #[cfg(feature = "live")]
    fn unpin_directory_async(&self, node: NodeId) -> PortFuture<'_, ()> {
        Box::pin(async move { self.unpin_directory(node) })
    }
    #[cfg(feature = "live")]
    fn create_file_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> PortFuture<'a, Attr> {
        Box::pin(async move { self.create_file(parent, name, mode) })
    }
    #[cfg(feature = "live")]
    fn create_file_open_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> PortFuture<'a, Attr> {
        Box::pin(async move { self.create_file_open(parent, name, mode) })
    }
    #[cfg(feature = "live")]
    fn mkdir_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        mode: u32,
    ) -> PortFuture<'a, Attr> {
        Box::pin(async move { self.mkdir(parent, name, mode) })
    }
    #[cfg(feature = "live")]
    fn symlink_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        target: Vec<u8>,
    ) -> PortFuture<'a, Attr> {
        Box::pin(async move { self.symlink(parent, name, target) })
    }
    #[cfg(feature = "live")]
    fn link_async<'a>(
        &'a self,
        node: NodeId,
        parent: NodeId,
        name: &'a [u8],
    ) -> PortFuture<'a, Attr> {
        Box::pin(async move { self.link(node, parent, name) })
    }
    #[cfg(feature = "live")]
    fn unlink_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        directory: bool,
    ) -> PortFuture<'a, ()> {
        Box::pin(async move { self.unlink(parent, name, directory) })
    }
    #[cfg(feature = "live")]
    fn rename_async<'a>(
        &'a self,
        parent: NodeId,
        name: &'a [u8],
        new_parent: NodeId,
        new_name: &'a [u8],
        no_replace: bool,
    ) -> PortFuture<'a, ()> {
        Box::pin(async move { self.rename(parent, name, new_parent, new_name, no_replace) })
    }
    #[cfg(feature = "live")]
    fn pin_async<'a>(&'a self, node: NodeId, truncate: bool, writable: bool) -> PortFuture<'a, ()> {
        Box::pin(async move { self.pin(node, truncate, writable) })
    }
    #[cfg(feature = "live")]
    /// SETATTR-class mutations report the exact post-mutation attr (#144 R1a).
    /// A port whose mutation reply already carries it overrides this method;
    /// the default re-reads the same authority it just mutated.
    fn truncate_async<'a>(&'a self, node: NodeId, size: u64) -> PortFuture<'a, Attr> {
        Box::pin(async move {
            self.truncate(node, size)?;
            self.attr(node)
        })
    }
    #[cfg(feature = "live")]
    fn chmod_async<'a>(&'a self, node: NodeId, mode: u32) -> PortFuture<'a, Attr> {
        Box::pin(async move {
            self.chmod(node, mode)?;
            self.attr(node)
        })
    }
    #[cfg(feature = "live")]
    fn set_mtime_async<'a>(
        &'a self,
        node: NodeId,
        seconds: i64,
        nanos: u32,
    ) -> PortFuture<'a, Attr> {
        Box::pin(async move {
            self.set_mtime(node, seconds, nanos)?;
            self.attr(node)
        })
    }
    #[cfg(feature = "live")]
    fn fsync_async<'a>(&'a self, node: Option<NodeId>) -> PortFuture<'a, ()> {
        Box::pin(async move { self.fsync(node) })
    }
    #[cfg(feature = "live")]
    fn readdir_page_async<'a>(
        &'a self,
        node: NodeId,
        offset: usize,
    ) -> PortFuture<'a, Vec<(NodeId, Kind, Vec<u8>)>> {
        Box::pin(async move { self.readdir_page(node, offset) })
    }
    #[cfg(feature = "live")]
    fn readdirplus_page_async<'a>(
        &'a self,
        node: NodeId,
        offset: usize,
    ) -> PortFuture<'a, Vec<(Attr, Vec<u8>)>> {
        Box::pin(async move { self.readdirplus_page(node, offset) })
    }

    fn admit_callback(
        &self,
        _operation: KernelOperation,
        _bytes: usize,
        _writeback: bool,
    ) -> PortResult<CallbackGuard> {
        Ok(CallbackGuard::default())
    }
    fn note_cached_open(&self, _node: NodeId) {}
    fn note_kernel_operation(&self, _operation: KernelOperation) {}
    fn note_readdir_page(&self, _offset: u64, _entries: u64) {}
    fn note_fuse_max_write(&self, _bytes: u32) {}
    fn note_fuse_read_config(&self, _max_readahead: u32, _capabilities: u64) {}
    fn lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Attr>;
    fn attr(&self, node: NodeId) -> PortResult<Attr>;
    fn readlink(&self, node: NodeId) -> PortResult<Vec<u8>>;
    fn readdir(&self, node: NodeId) -> PortResult<Vec<(NodeId, Kind, Vec<u8>)>>;
    fn readdirplus(&self, node: NodeId) -> PortResult<Vec<(Attr, Vec<u8>)>> {
        self.readdir(node)?
            .into_iter()
            .map(|(node, _, name)| self.attr(node).map(|attr| (attr, name)))
            .collect()
    }
    fn readdir_page(
        &self,
        node: NodeId,
        offset: usize,
    ) -> PortResult<Vec<(NodeId, Kind, Vec<u8>)>> {
        Ok(self
            .readdir(node)?
            .into_iter()
            .skip(offset)
            .take(DIRECTORY_PAGE_ENTRIES)
            .collect())
    }
    fn readdirplus_page(&self, node: NodeId, offset: usize) -> PortResult<Vec<(Attr, Vec<u8>)>> {
        Ok(self
            .readdirplus(node)?
            .into_iter()
            .skip(offset)
            .take(DIRECTORY_PAGE_ENTRIES)
            .collect())
    }
    fn create_file(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr>;
    fn create_file_open(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr> {
        let attr = self.create_file(parent, name, mode)?;
        self.pin(attr.node, false, true)?;
        Ok(attr)
    }
    fn reserve_nodes(&self, _count: u32) -> PortResult<NodeId> {
        Err(PortError::Invalid)
    }
    fn create_file_open_reserved(
        &self,
        _parent: NodeId,
        _name: &[u8],
        _mode: u32,
        _node: NodeId,
    ) -> PortResult<Attr> {
        Err(PortError::Invalid)
    }
    #[allow(clippy::type_complexity)]
    fn create_files_closed_reserved(
        &self,
        entries: &[(
            NodeId,
            Vec<u8>,
            u32,
            NodeId,
            Vec<(u64, Vec<u8>)>,
            Option<(i64, u32)>,
        )],
    ) -> PortResult<()> {
        for (parent, name, mode, node, writes, mtime) in entries {
            self.create_file_open_reserved(*parent, name, *mode, *node)?;
            for (offset, bytes) in writes {
                self.write(*node, *offset, bytes)?;
            }
            if let Some((seconds, nanos)) = mtime {
                self.set_mtime(*node, *seconds, *nanos)?;
            }
            self.unpin(*node, true)?;
        }
        Ok(())
    }
    fn mkdir(&self, parent: NodeId, name: &[u8], mode: u32) -> PortResult<Attr>;
    fn mkdir_reserved(
        &self,
        _parent: NodeId,
        _name: &[u8],
        _mode: u32,
        _node: NodeId,
    ) -> PortResult<Attr> {
        Err(PortError::Invalid)
    }
    fn symlink(&self, parent: NodeId, name: &[u8], target: Vec<u8>) -> PortResult<Attr>;
    fn link(&self, node: NodeId, parent: NodeId, name: &[u8]) -> PortResult<Attr>;
    fn unlink(&self, parent: NodeId, name: &[u8], directory: bool) -> PortResult<()>;
    fn unlink_batch(&self, entries: &[(NodeId, Vec<u8>)]) -> PortResult<()> {
        for (parent, name) in entries {
            self.unlink(*parent, name, false)?;
        }
        Ok(())
    }
    fn rename(
        &self,
        parent: NodeId,
        name: &[u8],
        new_parent: NodeId,
        new_name: &[u8],
        no_replace: bool,
    ) -> PortResult<()>;
    fn pin(&self, node: NodeId, truncate: bool, writable: bool) -> PortResult<()>;
    fn unpin(&self, node: NodeId, writable: bool) -> PortResult<()>;
    fn pin_directory(&self, _node: NodeId) -> PortResult<()> {
        Ok(())
    }
    fn unpin_directory(&self, _node: NodeId) -> PortResult<()> {
        Ok(())
    }
    fn read(&self, node: NodeId, offset: u64, size: usize) -> PortResult<Vec<u8>>;
    fn write(&self, node: NodeId, offset: u64, bytes: &[u8]) -> PortResult<usize>;
    /// The adapter transfers the one-shot kernel reply. Implementations may park it
    /// while acquiring backing; reserve retained argument bytes before copying them.
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    fn submit_write(
        &self,
        node: NodeId,
        offset: u64,
        bytes: &[u8],
        _writeback: bool,
        reply: WriteReply,
    ) {
        reply.complete(self.write(node, offset, bytes));
    }
    #[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
    fn submit_read(&self, node: NodeId, offset: u64, size: usize, reply: ReadReply) {
        reply.complete(self.read(node, offset, size));
    }
    fn write_zero(&self, node: NodeId, offset: u64, len: usize) -> PortResult<usize> {
        self.write(node, offset, &vec![0; len])
    }
    fn truncate(&self, node: NodeId, size: u64) -> PortResult<()>;
    fn chmod(&self, node: NodeId, mode: u32) -> PortResult<()>;
    fn set_mtime(&self, node: NodeId, seconds: i64, nanos: u32) -> PortResult<()>;
    fn fsync(&self, node: Option<NodeId>) -> PortResult<()>;
}

pub type SharedPort = Arc<dyn FilesystemPort>;

/// Owned kernel completion; neither borrowed request bytes nor adapter state escape.
#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
pub struct WriteReply {
    pub(crate) reply: fuser::ReplyWrite,
    pub(crate) _guard: CallbackGuard,
    pub(crate) maximum: usize,
}

#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
impl WriteReply {
    pub fn complete(self, result: PortResult<usize>) {
        match result {
            Ok(size) if size <= self.maximum && u32::try_from(size).is_ok() => {
                self.reply.written(size as u32)
            }
            Ok(_) => self.reply.error(fuser::Errno::EIO),
            Err(error) => self.reply.error(crate::adapter::errno(error)),
        }
    }
}

#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
pub struct ReadReply {
    pub(crate) reply: fuser::ReplyData,
    pub(crate) _guard: CallbackGuard,
    pub(crate) maximum: usize,
}

#[cfg(all(target_os = "linux", any(feature = "host", feature = "proxy")))]
impl ReadReply {
    pub fn complete(self, result: PortResult<Vec<u8>>) {
        match result {
            Ok(bytes) if bytes.len() <= self.maximum => self.reply.data(&bytes),
            Ok(_) => self.reply.error(fuser::Errno::EIO),
            Err(error) => self.reply.error(crate::adapter::errno(error)),
        }
    }
}
