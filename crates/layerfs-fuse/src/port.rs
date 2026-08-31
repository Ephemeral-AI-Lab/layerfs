use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(pub u64);

pub const ROOT: NodeId = NodeId(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    File,
    Directory,
    Symlink,
}

#[derive(Clone, Copy, Debug)]
pub struct Attr {
    pub node: NodeId,
    pub size: u64,
    pub kind: Kind,
    pub mode: u32,
    pub links: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}

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

pub trait FilesystemPort: Send + Sync {
    fn note_fuse_max_write(&self, _bytes: u32) {}
    fn note_fuse_read_config(&self, _max_readahead: u32, _capabilities: u64) {}
    fn lookup(&self, parent: NodeId, name: &[u8]) -> PortResult<Attr>;
    fn attr(&self, node: NodeId) -> PortResult<Attr>;
    fn readlink(&self, node: NodeId) -> PortResult<Vec<u8>>;
    fn readdir(&self, node: NodeId) -> PortResult<Vec<(NodeId, Kind, Vec<u8>)>>;
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
    fn read(&self, node: NodeId, offset: u64, size: usize) -> PortResult<Vec<u8>>;
    fn write(&self, node: NodeId, offset: u64, bytes: &[u8]) -> PortResult<usize>;
    fn write_zero(&self, node: NodeId, offset: u64, len: usize) -> PortResult<usize> {
        self.write(node, offset, &vec![0; len])
    }
    fn truncate(&self, node: NodeId, size: u64) -> PortResult<()>;
    fn chmod(&self, node: NodeId, mode: u32) -> PortResult<()>;
    fn set_mtime(&self, node: NodeId, seconds: i64, nanos: u32) -> PortResult<()>;
    fn fsync(&self, node: Option<NodeId>) -> PortResult<()>;
}

pub type SharedPort = Arc<dyn FilesystemPort>;
