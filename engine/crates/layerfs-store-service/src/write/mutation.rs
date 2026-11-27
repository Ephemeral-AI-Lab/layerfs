//! Private Workspace mutations used by one atomic public filesystem call.
use crate::history::lookup as args;
use crate::store::{BoundWorkspace, StoreOperation};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::filesystem::PathName;
use layerfs_content::object::inode_leaf::InodeKind;
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
/// A caller's `mode` for the entry of `kind` at `path`: the permission bits
/// LayerFS stores, as `project.init` accepts them. Set-user-ID, set-group-ID
/// and the sticky bit on a file are refused as `InvalidInput` with reason
/// `Mode`; a symbolic link has no mode of its own.
fn permitted(path: &str, kind: InodeKind, mode: u32) -> Result<u32> {
    let mask = match kind {
        InodeKind::Directory => 0o1777,
        InodeKind::RegularFile => 0o777,
        InodeKind::Symlink => {
            return Err(args::refuse(
                "NotFile",
                format!("`{path}` is a symbolic link, which has no mode of its own"),
                SdkValue::Null,
            ))
        }
    };
    if mode & !mask != 0 {
        return Err(args::refuse(
            "InvalidInput",
            format!(
                "mode {mode:04o} has bits LayerFS cannot store (set-user-ID, set-group-ID, or sticky on a file)"
            ),
            SdkValue::map([
                ("reason", SdkValue::text("Mode")),
                ("path", SdkValue::text(path)),
                ("mode", SdkValue::text(format!("{mode:04o}"))),
            ]),
        ));
    }
    Ok(mode)
}
/// The refusal an engine error is to a call, with a plain sentence for each
/// definite refusal; only an unexpected engine error keeps its own text.
pub fn failure(e: WorkspaceError) -> SdkRefusal {
    use layerfs_workspace::Refusal as R;
    let (code, message) = match &e {
        WorkspaceError::Refused(r) => match r {
            R::Missing => ("NoSuchPath", "no such file or folder"),
            R::Exists => ("Exists", "the destination already exists"),
            R::IsDirectory => ("IsDirectory", "expected a file, found a folder"),
            R::NotDirectory => ("NotDirectory", "a parent in the path is not a folder"),
            R::NotEmpty => ("NotEmpty", "the folder is not empty"),
            R::TooLarge => ("InvalidInput", "the file would pass the largest size"),
            R::Invalid => ("InvalidInput", "the change is not valid here"),
            _ => ("InvalidInput", "the change is not allowed on this entry"),
        },
        WorkspaceError::Content(layerfs_content::ContentError::PathNotFound) => {
            ("NoSuchPath", "no such file or folder")
        }
        WorkspaceError::Service(inner) if capacity(inner.as_ref()) => {
            ("Capacity", "the Store has no space for this change")
        }
        _ => return args::refuse("Unknown", e.to_string(), SdkValue::Null),
    };
    args::refuse(code, message, SdkValue::Null)
}
/// `NotDirectory` at `at`, an entry that is not a folder.
pub fn not_folder(at: &str) -> SdkRefusal {
    args::refuse(
        "NotDirectory",
        format!("{at} is not a folder"),
        SdkValue::Null,
    )
}
/// A refused view read. A read changes nothing in the overlay, so an engine
/// error is a known `Failed`, never the uncertain `Unknown` that keeps the
/// call's Workspace.
pub fn read_failure(e: WorkspaceError) -> SdkRefusal {
    let mut refusal = failure(e);
    if refusal.code == "Unknown" {
        refusal.code = "Failed".to_owned();
    }
    refusal
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
    /// The entry at `path`. A name below an entry that is not a directory (a
    /// file or a symlink, which is not followed) is `NotDirectory` at that
    /// entry, as `mkdir` answers, not an engine failure.
    pub fn resolve(&self, path: &str) -> Result<ViewStat> {
        match self.walk(path, true)? {
            (_, Some(stat)) => Ok(stat),
            (serial, None) => (self.view)
                .stat(self.op.overlay(), serial)
                .map_err(read_failure),
        }
    }
    /// The serial at `path`, each component bound by kind only (#818 W6):
    /// with `stat_last`, the last one's full stat too. A walk needs only
    /// the kinds of the directories it passes; the checks keep their order
    /// (the name, then `NotDirectory` at the entry before it, then
    /// `NoSuchPath`), so the answers are the full-stat walk's.
    fn walk(&self, path: &str, stat_last: bool) -> Result<(u64, Option<ViewStat>)> {
        let overlay = self.op.overlay();
        let (mut serial, mut kind) = (self.view.root_serial(), InodeKind::Directory);
        let names: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut at = String::new();
        for (index, name) in names.iter().enumerate() {
            let name = checked(path, name)?;
            if kind != InodeKind::Directory {
                return Err(not_folder(&at));
            }
            at.push('/');
            at.push_str(name.as_str());
            let missing = |e| match read_failure(e) {
                r if r.code == "NoSuchPath" => {
                    args::refuse("NoSuchPath", format!("{at} does not exist"), SdkValue::Null)
                }
                r => r,
            };
            if stat_last && index + 1 == names.len() {
                let stat = (self.view.lookup(overlay, serial, &name)).map_err(missing)?;
                return Ok((stat.serial, Some(stat)));
            }
            (serial, kind) = (self.view.lookup_kind(overlay, serial, &name)).map_err(missing)?;
        }
        Ok((serial, None))
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
        let (parent, _) = self.walk(if parent.is_empty() { "/" } else { parent }, false)?;
        Ok((parent, checked(path, name)?))
    }
    pub fn read(&self, serial: u64, offset: u64, length: u32) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        self.view
            .read(self.op.overlay(), serial, offset, length, &mut bytes)
            .map_err(read_failure)?;
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
    pub fn write(&mut self, path: &str, bytes: &[u8], mode: Option<u32>) -> Result<()> {
        let (serial, same) = self.target(path, bytes.len() as u64, mode)?;
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
    fn set_mode(&mut self, serial: u64, mode: u32) -> Result<()> {
        self.apply(Operation::SetAttributes {
            serial,
            mode: Some(mode),
            mtime: None,
            size: None,
        })?;
        Ok(())
    }
    /// Sets the permission bits of the entry at `path`.
    pub fn chmod(&mut self, path: &str, mode: u32) -> Result<()> {
        let stat = self.resolve(path)?;
        let mode = permitted(path, stat.kind, mode)?;
        self.set_mode(stat.serial, mode)
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
    /// The regular file a write of `length` bytes goes to, created when absent
    /// with `mode` (default 0o644), and whether it already has that length. An
    /// existing file takes `mode` only when one is given.
    fn target(&mut self, path: &str, length: u64, mode: Option<u32>) -> Result<(u64, bool)> {
        Ok(match self.optional(path)? {
            Some(stat) => {
                if stat.kind != InodeKind::RegularFile {
                    return Err(args::refuse(
                        if stat.kind == InodeKind::Directory {
                            "IsDirectory"
                        } else {
                            "NotFile"
                        },
                        "expected a regular file",
                        SdkValue::Null,
                    ));
                }
                if let Some(mode) = mode {
                    let mode = permitted(path, InodeKind::RegularFile, mode)?;
                    self.set_mode(stat.serial, mode)?;
                }
                (stat.serial, stat.logical_len == length)
            }
            None => {
                let mode = permitted(path, InodeKind::RegularFile, mode.unwrap_or(0o644))?;
                let (parent, name) = self.parent(path)?;
                let serial = self
                    .apply(Operation::Create { parent, name, mode })?
                    .ok_or_else(|| args::failed("created inode"))?
                    .serial;
                (serial, false)
            }
        })
    }
    /// Creates the directory `path` with `mode` (default 0o755) and its
    /// missing parents with 0o755; an existing directory keeps its mode.
    pub fn mkdir(&mut self, path: &str, mode: Option<u32>) -> Result<()> {
        let mode = permitted(path, InodeKind::Directory, mode.unwrap_or(0o755))?;
        let mut at = String::new();
        let mut names = path.split('/').filter(|s| !s.is_empty()).peekable();
        while let Some(name) = names.next() {
            at.push('/');
            at.push_str(name);
            match self.optional(&at)? {
                Some(s) if s.kind == InodeKind::Directory => {}
                Some(_) => return Err(not_folder(&at)),
                None => {
                    let (parent, name) = self.parent(&at)?;
                    let mode = if names.peek().is_none() { mode } else { 0o755 };
                    self.apply(Operation::Mkdir { parent, name, mode })?;
                }
            }
        }
        Ok(())
    }
}
