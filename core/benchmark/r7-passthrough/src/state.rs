use fuser::{Errno, FileAttr, FileType, INodeNo};
use std::{
    collections::HashMap,
    ffi::{CString, OsStr},
    fs::{File, Metadata},
    io,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt},
        },
    },
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const WINDOW: usize = 128 * 1024;
pub const TTL: Duration = Duration::from_secs(60);
pub type Result<T> = std::result::Result<T, Errno>;
pub fn error(e: io::Error) -> Errno {
    Errno::from_i32(e.raw_os_error().unwrap_or(libc::EIO))
}
pub fn checked(code: i32) -> Result<()> {
    if code == -1 {
        Err(error(io::Error::last_os_error()))
    } else {
        Ok(())
    }
}
pub fn cstring(name: &OsStr) -> Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| Errno::EINVAL)
}
pub fn owned_fd(fd: i32) -> Result<File> {
    checked(fd)?;
    // Each successful native open transfers its sole descriptor here.
    Ok(unsafe { File::from_raw_fd(fd) })
}
pub fn reopen(file: &File, flags: i32) -> Result<File> {
    let name = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap();
    owned_fd(unsafe { libc::open(name.as_ptr(), flags | libc::O_CLOEXEC) })
}
pub struct Node {
    pub file: Arc<File>,
    pub lookups: u64,
    pub key: (u64, u64),
}
pub struct State {
    pub nodes: HashMap<u64, Node>,
    pub identities: HashMap<(u64, u64), u64>,
    pub handles: HashMap<u64, Arc<File>>,
    pub next: u64,
    pub opcodes: [u64; 64],
}
pub struct Passthrough {
    pub state: Mutex<State>,
}
impl Passthrough {
    pub fn new(root: File) -> io::Result<Self> {
        let meta = root.metadata()?;
        let key = (meta.dev(), meta.ino());
        let mut nodes = HashMap::new();
        nodes.insert(
            1,
            Node {
                file: Arc::new(root),
                lookups: 1,
                key,
            },
        );
        Ok(Self {
            state: Mutex::new(State {
                nodes,
                identities: HashMap::from([(key, 1)]),
                handles: HashMap::new(),
                next: 2,
                opcodes: [0; 64],
            }),
        })
    }
    pub fn count(&self, opcode: usize) {
        self.state.lock().unwrap().opcodes[opcode] += 1;
    }
    pub fn node(&self, ino: INodeNo) -> Result<Arc<File>> {
        self.state
            .lock()
            .unwrap()
            .nodes
            .get(&ino.0)
            .map(|n| n.file.clone())
            .ok_or(Errno::ESTALE)
    }
    pub fn handle(&self, id: u64) -> Result<Arc<File>> {
        self.state
            .lock()
            .unwrap()
            .handles
            .get(&id)
            .cloned()
            .ok_or(Errno::EBADF)
    }
    pub fn put_handle(&self, file: File) -> Result<u64> {
        let mut state = self.state.lock().unwrap();
        let id = state.next;
        state.next = id.checked_add(1).ok_or(Errno::EOVERFLOW)?;
        state.handles.insert(id, Arc::new(file));
        Ok(id)
    }
    pub fn release_handle(&self, id: u64) -> Result<()> {
        self.state
            .lock()
            .unwrap()
            .handles
            .remove(&id)
            .map(|_| ())
            .ok_or(Errno::EBADF)
    }
    pub fn insert(&self, file: File) -> Result<FileAttr> {
        let meta = file.metadata().map_err(error)?;
        let key = (meta.dev(), meta.ino());
        let mut state = self.state.lock().unwrap();
        let id = match state.identities.get(&key) {
            Some(id) => *id,
            None => {
                // One backing filesystem, stable native identity on remount.
                if key.0 != state.nodes.get(&1).ok_or(Errno::ESTALE)?.key.0 {
                    return Err(Errno::EXDEV);
                }
                let id = key
                    .1
                    .checked_add(1)
                    .filter(|id| *id != 1)
                    .ok_or(Errno::EOVERFLOW)?;
                state.identities.insert(key, id);
                state.nodes.insert(
                    id,
                    Node {
                        file: Arc::new(file),
                        lookups: 0,
                        key,
                    },
                );
                id
            }
        };
        let node = state.nodes.get_mut(&id).ok_or(Errno::ESTALE)?;
        node.lookups = node.lookups.checked_add(1).ok_or(Errno::EOVERFLOW)?;
        attr(&meta, INodeNo(id))
    }
    pub fn lookup_native(&self, parent: INodeNo, name: &OsStr) -> Result<FileAttr> {
        let parent = self.node(parent)?;
        let name = cstring(name)?;
        let file = owned_fd(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        self.insert(file)
    }
    pub fn forget_native(&self, ino: INodeNo, count: u64) {
        if ino == INodeNo::ROOT {
            return;
        }
        let mut state = self.state.lock().unwrap();
        if let Some(node) = state.nodes.get_mut(&ino.0) {
            node.lookups = node
                .lookups
                .checked_sub(count)
                .expect("kernel lookup underflow");
            if node.lookups == 0 {
                let key = node.key;
                state.nodes.remove(&ino.0);
                state.identities.remove(&key);
            }
        }
    }
}
fn timestamp(seconds: i64, nanos: i64) -> SystemTime {
    if seconds >= 0 {
        UNIX_EPOCH + Duration::new(seconds as u64, nanos as u32)
    } else {
        UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
            + Duration::from_nanos(nanos as u64)
    }
}
pub fn kind(meta: &Metadata) -> Result<FileType> {
    let t = meta.file_type();
    if t.is_dir() {
        Ok(FileType::Directory)
    } else if t.is_file() {
        Ok(FileType::RegularFile)
    } else if t.is_symlink() {
        Ok(FileType::Symlink)
    } else if t.is_fifo() {
        Ok(FileType::NamedPipe)
    } else if t.is_socket() {
        Ok(FileType::Socket)
    } else if t.is_char_device() {
        Ok(FileType::CharDevice)
    } else if t.is_block_device() {
        Ok(FileType::BlockDevice)
    } else {
        Err(Errno::EIO)
    }
}
pub fn attr(meta: &Metadata, ino: INodeNo) -> Result<FileAttr> {
    Ok(FileAttr {
        ino,
        size: meta.size(),
        blocks: meta.blocks(),
        atime: timestamp(meta.atime(), meta.atime_nsec()),
        mtime: timestamp(meta.mtime(), meta.mtime_nsec()),
        ctime: timestamp(meta.ctime(), meta.ctime_nsec()),
        crtime: UNIX_EPOCH,
        kind: kind(meta)?,
        perm: (meta.mode() & 0o7777) as u16,
        nlink: meta.nlink().try_into().map_err(|_| Errno::EOVERFLOW)?,
        uid: meta.uid(),
        gid: meta.gid(),
        rdev: meta.rdev().try_into().map_err(|_| Errno::EOVERFLOW)?,
        flags: 0,
        blksize: meta.blksize().try_into().map_err(|_| Errno::EOVERFLOW)?,
    })
}
