use crate::engine::{now, Engine, Node};
use fuser::*;
use rusqlite::params;
use std::{
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const TTL: Duration = Duration::ZERO;
pub struct SqlFs {
    pub engine: Arc<Mutex<Engine>>,
}
fn attr(n: &Node) -> FileAttr {
    let time = UNIX_EPOCH + Duration::new(n.seconds.max(0) as u64, n.nanos);
    FileAttr {
        ino: INodeNo(n.id as u64),
        size: n.size as u64,
        blocks: (n.size as u64).div_ceil(512),
        atime: time,
        mtime: time,
        ctime: time,
        crtime: UNIX_EPOCH,
        kind: if n.kind == 2 {
            FileType::Directory
        } else {
            FileType::RegularFile
        },
        perm: n.mode as u16,
        nlink: if n.links == 0 {
            0
        } else if n.kind == 2 {
            2 + n.subdirs as u32
        } else {
            n.links as u32
        },
        uid: crate::command_identity::UID,
        gid: crate::command_identity::GID,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    }
}
fn error(e: String) -> Errno {
    eprintln!("SQL FUSE: {e}");
    match e.as_str() {
        "ENOENT" => Errno::ENOENT,
        "EEXIST" => Errno::EEXIST,
        "ENOTDIR" => Errno::ENOTDIR,
        "EISDIR" => Errno::EISDIR,
        "ENOTEMPTY" => Errno::ENOTEMPTY,
        "ENOSPC" => Errno::ENOSPC,
        "EMFILE" => Errno::EMFILE,
        "EINVAL" => Errno::EINVAL,
        "EBADF" => Errno::EBADF,
        "EFBIG" => Errno::EFBIG,
        _ => Errno::EIO,
    }
}
impl SqlFs {
    fn call<T>(
        &self,
        request: &Request,
        f: impl FnOnce(&mut Engine) -> Result<T, String>,
    ) -> Result<T, Errno> {
        if request.uid() != crate::command_identity::UID && request.uid() != 0 {
            return Err(Errno::EACCES);
        }
        let mut e = self.engine.lock().map_err(|_| Errno::EIO)?;
        e.callbacks += 1;
        f(&mut e).map_err(error)
    }
}
impl Filesystem for SqlFs {
    fn lookup(&self, request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        match self.call(request, |e| {
            let id = e
                .lookup(parent.0 as i64, name.as_bytes())?
                .ok_or("ENOENT")?;
            e.node(id)
        }) {
            Ok(n) => reply.entry(&TTL, &attr(&n), Generation(1)),
            Err(e) => reply.error(e),
        }
    }
    fn getattr(&self, request: &Request, ino: INodeNo, _: Option<FileHandle>, reply: ReplyAttr) {
        match self.call(request, |e| e.node(ino.0 as i64)) {
            Ok(n) => reply.attr(&TTL, &attr(&n)),
            Err(e) => reply.error(e),
        }
    }
    fn create(
        &self,
        request: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        match self.call(request, |e| {
            if flags & (libc::O_SYNC | libc::O_DSYNC) != 0 {
                return Err("EINVAL".into());
            }
            let n = e.create_node(parent.0 as i64, name.as_bytes(), 1, mode & !umask)?;
            let h = e.open(n.id, flags)?;
            Ok((n, h))
        }) {
            Ok((n, h)) => reply.created(
                &TTL,
                &attr(&n),
                Generation(1),
                FileHandle(h as u64),
                FopenFlags::FOPEN_DIRECT_IO,
            ),
            Err(e) => reply.error(e),
        }
    }
    fn mkdir(
        &self,
        request: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        reply: ReplyEntry,
    ) {
        match self.call(request, |e| {
            e.create_node(parent.0 as i64, name.as_bytes(), 2, mode & !umask)
        }) {
            Ok(n) => reply.entry(&TTL, &attr(&n), Generation(1)),
            Err(e) => reply.error(e),
        }
    }
    fn open(&self, request: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        match self.call(request, |e| {
            let flags = flags.0;
            if flags & (libc::O_SYNC | libc::O_DSYNC) != 0 {
                return Err("EINVAL".into());
            }
            if e.node(ino.0 as i64)?.kind != 1 {
                return Err("EISDIR".into());
            }
            e.open(ino.0 as i64, flags)
        }) {
            Ok(h) => reply.opened(FileHandle(h as u64), FopenFlags::FOPEN_DIRECT_IO),
            Err(e) => reply.error(e),
        }
    }
    fn read(
        &self,
        request: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyData,
    ) {
        match self.call(request, |e| {
            if size > 128 * 1024 || offset > i64::MAX as u64 {
                return Err("EINVAL".into());
            }
            e.handle(fh.0 as i64, ino.0 as i64, false)?;
            let mut out = vec![0; size as usize];
            let n = e.read(ino.0 as i64, offset as i64, &mut out)?;
            out.truncate(n);
            Ok(out)
        }) {
            Ok(out) => reply.data(&out),
            Err(e) => reply.error(e),
        }
    }
    fn write(
        &self,
        request: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        _: WriteFlags,
        _: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        match self.call(request, |e| {
            if offset > i64::MAX as u64 {
                return Err("EFBIG".into());
            }
            e.handle(fh.0 as i64, ino.0 as i64, true)?;
            e.write(ino.0 as i64, offset as i64, data)
        }) {
            Ok(()) => reply.written(data.len() as u32),
            Err(e) => reply.error(e),
        }
    }
    fn flush(
        &self,
        request: &Request,
        ino: INodeNo,
        fh: FileHandle,
        _: LockOwner,
        reply: ReplyEmpty,
    ) {
        match self.call(request, |e| e.handle(fh.0 as i64, ino.0 as i64, false)) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn release(
        &self,
        request: &Request,
        ino: INodeNo,
        fh: FileHandle,
        _: OpenFlags,
        _: Option<LockOwner>,
        _: bool,
        reply: ReplyEmpty,
    ) {
        match self.call(request, |e| e.close(fh.0 as i64, ino.0 as i64)) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn setattr(
        &self,
        request: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _: Option<SystemTime>,
        fh: Option<FileHandle>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        match self.call(request,|e| {
            if uid.is_some() || gid.is_some() || flags.is_some() {
                return Err("EINVAL".into());
            }
            let id = ino.0 as i64;
            let n = e.node(id)?;
            if let Some(s) = size {
                if n.kind != 1 || s > i64::MAX as u64 {
                    return Err("EINVAL".into());
                }
                if let Some(h) = fh {
                    e.handle(h.0 as i64, id, true)?
                }
                e.truncate(id, s as i64)?;
            }
            if let Some(m) = mode {
                e.db.execute(
                    "UPDATE inodes SET mode=?2,dirty=CASE WHEN links>0 OR published=1 THEN 1 ELSE 0 END WHERE id=?1",
                    params![id, m & 0o777],
                )
                .map_err(|e| e.to_string())?;
                e.revision += 1;
            }
            if let Some(t) = mtime {
                let (sec, nano) = match t {
                    TimeOrNow::Now => now(),
                    TimeOrNow::SpecificTime(time) => {
                        let t = time.duration_since(UNIX_EPOCH).map_err(|_| "EINVAL")?;
                        (t.as_secs() as i64, t.subsec_nanos())
                    }
                };
                e.db.execute(
                    "UPDATE inodes SET seconds=?2,nanos=?3,dirty=CASE WHEN links>0 OR published=1 THEN 1 ELSE 0 END WHERE id=?1",
                    params![id, sec, nano],
                )
                .map_err(|e| e.to_string())?;
                e.revision += 1;
            }
            e.node(id)
        }) {
            Ok(n) => reply.attr(&TTL, &attr(&n)),
            Err(e) => reply.error(e),
        }
    }
    fn unlink(&self, request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        match self.call(request, |e| {
            e.unlink(parent.0 as i64, name.as_bytes(), false)
        }) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn rmdir(&self, request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        match self.call(request, |e| {
            e.unlink(parent.0 as i64, name.as_bytes(), true)
        }) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn rename(
        &self,
        request: &Request,
        parent: INodeNo,
        name: &OsStr,
        new_parent: INodeNo,
        new_name: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        let result = self.call(request, |e| match flags.bits() {
            0 | 1 => e.rename(
                parent.0 as i64,
                name.as_bytes(),
                new_parent.0 as i64,
                new_name.as_bytes(),
                flags.bits() == 1,
            ),
            _ => Err("EINVAL".into()),
        });
        match result {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn opendir(&self, request: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        match self.call(request, |e| {
            if e.node(ino.0 as i64)?.kind != 2 {
                return Err("ENOTDIR".into());
            }
            e.open(ino.0 as i64, flags.0)
        }) {
            Ok(h) => reply.opened(FileHandle(h as u64), FopenFlags::empty()),
            Err(e) => reply.error(e),
        }
    }
    fn readdir(
        &self,
        request: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let result = self.call(request, |e| {
            let handle = fh.0 as i64;
            let id = ino.0 as i64;
            e.handle(handle, id, false)?;
            let node = e.node(id)?;
            if node.kind != 2 {
                return Err("ENOTDIR".into());
            }
            let mut after = i64::try_from(offset).map_err(|_| "EINVAL")?;
            if after == 0 {
                if reply.add(ino, 1, FileType::Directory, OsStr::new(".")) {
                    return Ok(());
                }
                after = 1;
            }
            if after == 1 {
                if reply.add(
                    INodeNo(node.parent as u64),
                    2,
                    FileType::Directory,
                    OsStr::new(".."),
                ) {
                    return Ok(());
                }
                after = 2;
            }
            // Finite callback work and resumable partial replies, no population vector.
            for _ in 0..8 {
                let page = e.directory_page(handle, id, after)?;
                if page.is_empty() {
                    break;
                }
                for row in page {
                    if reply.add(
                        INodeNo(row.inode as u64),
                        row.cookie as u64,
                        if row.kind == 2 {
                            FileType::Directory
                        } else {
                            FileType::RegularFile
                        },
                        OsStr::from_bytes(&row.name),
                    ) {
                        return Ok(());
                    }
                    after = row.cookie;
                }
            }
            Ok(())
        });
        match result {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn releasedir(
        &self,
        request: &Request,
        ino: INodeNo,
        fh: FileHandle,
        _: OpenFlags,
        reply: ReplyEmpty,
    ) {
        match self.call(request, |e| e.close(fh.0 as i64, ino.0 as i64)) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
}
