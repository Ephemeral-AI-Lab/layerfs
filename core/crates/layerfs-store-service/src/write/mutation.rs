//! Private Workspace mutations used by one atomic public filesystem call.
use crate::history::lookup as args;
use crate::store::{BoundWorkspace, StoreOperation};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::filesystem::PathName;
use layerfs_workspace::{
    Operation, Outcome, OverlayJobs, SourceView, Time, ViewStat, WorkspaceError,
};
pub type Result<T> = std::result::Result<T, SdkRefusal>;
pub struct Mutation<'a> {
    pub workspace: &'a BoundWorkspace,
    pub op: StoreOperation,
    pub view: SourceView,
    pub now: Time,
    serials: (u64, u64),
}
/// A name of the caller's `path`; one the name grammar refuses (a `\` or
/// more than 255 bytes) is invalid input, as on the read route.
fn checked(path: &str, name: &str) -> Result<PathName> {
    PathName::new(name).map_err(|_| args::invalid(format!("`{path}` is not valid")))
}
pub fn failure(e: WorkspaceError) -> SdkRefusal {
    let code = match &e {
        WorkspaceError::Refused(r) => match r {
            layerfs_workspace::Refusal::Missing => "NoSuchPath",
            layerfs_workspace::Refusal::Exists => "Exists",
            layerfs_workspace::Refusal::IsDirectory => "IsDirectory",
            layerfs_workspace::Refusal::NotDirectory => "NotDirectory",
            layerfs_workspace::Refusal::NotEmpty => "NotEmpty",
            _ => "InvalidInput",
        },
        layerfs_workspace::WorkspaceError::Content(layerfs_content::ContentError::PathNotFound) => {
            "NoSuchPath"
        }
        WorkspaceError::Service(inner) if capacity(inner.as_ref()) => "Capacity",
        _ => "Unknown",
    };
    args::refuse(code, e.to_string(), SdkValue::Null)
}
/// A Store port's definite refusal for lack of space (#379): the write had no
/// effect, so it is `Capacity`, not `Unknown`.
fn capacity(inner: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    match inner
        .downcast_ref::<std::sync::Arc<crate::store::PortError>>()
        .map(std::sync::Arc::as_ref)
    {
        Some(crate::store::PortError::History(error)) => {
            matches!(error.cause(), layerfs_history::HistoryError::Capacity(_))
        }
        Some(crate::store::PortError::Storage(error)) => error.is_capacity_refusal(),
        _ => false,
    }
}
impl<'a> Mutation<'a> {
    pub fn new(workspace: &'a BoundWorkspace) -> Result<Self> {
        let op = workspace.operation().map_err(failure)?;
        let source = op
            .overlay()
            .acquire_diagnostic_source(workspace.route())
            .map_err(args::failed)?;
        let view = op.workspace().view_for_source(source).map_err(failure)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Ok(Self {
            workspace,
            op,
            view,
            now: Time {
                seconds: now.as_secs() as i64,
                nanoseconds: now.subsec_nanos(),
            },
            serials: (0, 0),
        })
    }
    pub fn finish(self) -> Result<()> {
        self.op
            .overlay()
            .release_base_source(self.workspace.route(), self.view.source())
            .map_err(args::failed)
    }
    fn serial(&mut self) -> Result<u64> {
        if self.serials.0 == self.serials.1 {
            let (start, count) = self.op.ports().reserve_pooled(64).map_err(failure)?;
            self.serials = (start, start + count);
        }
        let value = self.serials.0;
        self.serials.0 += 1;
        Ok(value)
    }
    pub fn apply(&mut self, operation: Operation) -> Result<Option<ViewStat>> {
        let serial = if operation.creates() {
            Some(self.serial()?)
        } else {
            None
        };
        let mut plan = self
            .op
            .workspace()
            .prepare_mutation(&self.view, operation, self.now, serial)
            .map_err(|e| failure(e.error))?;
        loop {
            let result = OverlayJobs::namespace(
                self.op.overlay(),
                plan.job().ok_or_else(|| args::failed("mutation stage"))?,
            );
            match plan.accept(result).map_err(failure)? {
                Some(Outcome::Applied { publication, stat }) => {
                    self.op
                        .overlay()
                        .reply_attempted(publication)
                        .map_err(args::failed)?;
                    return Ok(stat);
                }
                Some(Outcome::Unchanged { stat }) => return Ok(stat),
                None => plan.supply(&self.view).map_err(failure)?,
            }
        }
    }
    pub fn resolve(&self, path: &str) -> Result<ViewStat> {
        let mut stat = self
            .view
            .stat(self.op.overlay(), self.view.root_serial())
            .map_err(failure)?;
        for name in path.split('/').filter(|s| !s.is_empty()) {
            let name = checked(path, name)?;
            stat = self
                .view
                .lookup(self.op.overlay(), stat.serial, &name)
                .map_err(failure)?;
        }
        Ok(stat)
    }
    pub fn optional(&self, path: &str) -> Result<Option<ViewStat>> {
        match self.resolve(path) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.code == "NoSuchPath" => Ok(None),
            Err(e) => Err(e),
        }
    }
    pub fn parent(&self, path: &str) -> Result<(u64, PathName)> {
        if path == "/" {
            return Err(args::invalid("cannot replace the root"));
        }
        let (parent, name) = path
            .rsplit_once('/')
            .ok_or_else(|| args::invalid("absolute path"))?;
        let parent = self.resolve(if parent.is_empty() { "/" } else { parent })?;
        Ok((parent.serial, checked(path, name)?))
    }
    pub fn read(&self, serial: u64, offset: u64, length: u32) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.view
            .read(self.op.overlay(), serial, offset, length, &mut bytes)
            .map_err(failure)?;
        Ok(bytes)
    }
    pub fn write_at(&mut self, serial: u64, offset: u64, bytes: &[u8]) -> Result<()> {
        self.apply(Operation::Write {
            serial,
            position: layerfs_workspace::Position::At(offset),
            data: bytes.into(),
        })?;
        Ok(())
    }
    pub fn write(&mut self, path: &str, bytes: &[u8]) -> Result<()> {
        let (serial, same) = self.target(path, bytes.len() as u64)?;
        let mut changed = !same;
        let mut offset = 0;
        for piece in bytes.chunks(layerfs_bridge::duplex::MAX_PLAINTEXT_BYTES) {
            changed |= self.put(serial, same, offset, piece)?;
            offset += piece.len() as u64;
        }
        if changed {
            self.truncate(serial, bytes.len() as u64)?;
        }
        Ok(())
    }
    /// Writes `piece` at `offset` unless the file, of the same length, holds
    /// it already; true when written. Pieces are at most one record, so a
    /// write's overlay jobs stay bounded whatever the file's size.
    fn put(&mut self, serial: u64, same: bool, offset: u64, piece: &[u8]) -> Result<bool> {
        if same && self.read(serial, offset, piece.len() as u32)? == piece {
            return Ok(false);
        }
        self.write_at(serial, offset, piece)?;
        Ok(true)
    }
    fn truncate(&mut self, serial: u64, size: u64) -> Result<()> {
        self.apply(Operation::SetAttributes {
            serial,
            mode: None,
            mtime: None,
            size: Some(size),
        })?;
        Ok(())
    }
    /// The regular file a write of `length` bytes goes to, created when absent,
    /// and whether it already has that length.
    fn target(&mut self, path: &str, length: u64) -> Result<(u64, bool)> {
        Ok(match self.optional(path)? {
            Some(stat) => {
                if stat.kind != layerfs_content::object::inode_leaf::InodeKind::RegularFile {
                    return Err(args::refuse(
                        if stat.kind == layerfs_content::object::inode_leaf::InodeKind::Directory {
                            "IsDirectory"
                        } else {
                            "NotFile"
                        },
                        "expected a regular file",
                        SdkValue::Null,
                    ));
                }
                (stat.serial, stat.logical_len == length)
            }
            None => {
                let (parent, name) = self.parent(path)?;
                let serial = self
                    .apply(Operation::Create {
                        parent,
                        name,
                        mode: 0o644,
                    })?
                    .ok_or_else(|| args::failed("created inode"))?
                    .serial;
                (serial, false)
            }
        })
    }
    pub fn mkdir(&mut self, path: &str) -> Result<()> {
        let mut at = String::new();
        for name in path.split('/').filter(|s| !s.is_empty()) {
            at.push('/');
            at.push_str(name);
            match self.optional(&at)? {
                Some(s) if s.kind == layerfs_content::object::inode_leaf::InodeKind::Directory => {}
                Some(_) => return Err(args::refuse("NotDirectory", at, SdkValue::Null)),
                None => {
                    let (parent, name) = self.parent(&at)?;
                    self.apply(Operation::Mkdir {
                        parent,
                        name,
                        mode: 0o755,
                    })?;
                }
            }
        }
        Ok(())
    }
}
