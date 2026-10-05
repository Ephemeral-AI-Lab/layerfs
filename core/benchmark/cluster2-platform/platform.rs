//! Owning-platform prerequisite proof, not an overlay/Workspace qualification.
use fuser::{
    Config, Errno, FileAttr, FileHandle, FileType, Filesystem, INodeNo, LockOwner, MountOption,
    OpenFlags, ReplyAttr, ReplyData, ReplyEntry, Request,
};
use std::{
    ffi::OsStr,
    time::{Duration, UNIX_EPOCH},
};

struct Probe;
fn attributes(ino: INodeNo) -> FileAttr {
    FileAttr {
        ino,
        size: if ino == INodeNo::ROOT { 0 } else { 8 },
        blocks: 1,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: if ino == INodeNo::ROOT {
            FileType::Directory
        } else {
            FileType::RegularFile
        },
        perm: 0o755,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        flags: 0,
        blksize: 4096,
    }
}
impl Filesystem for Probe {
    fn lookup(&self, _: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        if parent == INodeNo::ROOT && name == "probe" {
            reply.entry(
                &Duration::ZERO,
                &attributes(INodeNo(2)),
                fuser::Generation(1),
            );
        } else {
            reply.error(Errno::ENOENT);
        }
    }
    fn getattr(&self, _: &Request, ino: INodeNo, _: Option<FileHandle>, reply: ReplyAttr) {
        if ino == INodeNo::ROOT || ino == INodeNo(2) {
            reply.attr(&Duration::ZERO, &attributes(ino));
        } else {
            reply.error(Errno::ENOENT);
        }
    }
    fn read(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        offset: u64,
        size: u32,
        _: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyData,
    ) {
        let data = b"fuse-ok\n";
        let start = (offset as usize).min(data.len());
        let end = start.saturating_add(size as usize).min(data.len());
        reply.data(&data[start..end]);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("mount path required")?;
    let db = rusqlite::Connection::open_in_memory()?;
    db.execute(
        "CREATE TABLE payload(id INTEGER PRIMARY KEY, data BLOB) STRICT",
        [],
    )?;
    db.execute(
        "INSERT INTO payload VALUES(1, ?1)",
        [b"\0\xffbytes".as_slice()],
    )?;
    let bytes: Vec<u8> = db.query_row("SELECT data FROM payload WHERE id=1", [], |r| r.get(0))?;
    assert_eq!(bytes, b"\0\xffbytes");
    let id = layerfs_content::ObjectId::for_bytes(&bytes);
    println!("content-id={id:?}; bundled-sqlite={}", rusqlite::version());
    std::fs::create_dir_all(&path)?;
    let mut config = Config::default();
    config.mount_options = vec![
        MountOption::DefaultPermissions,
        MountOption::FSName("cluster2-prerequisite".into()),
    ];
    config.n_threads = Some(2);
    let session = fuser::spawn_mount(Probe, &path, &config)?;
    assert_eq!(
        std::fs::read(std::path::Path::new(&path).join("probe"))?,
        b"fuse-ok\n"
    );
    session.umount_and_join()?;
    assert!(!std::path::Path::new(&path).join("probe").exists());
    std::fs::remove_dir(path)?;
    println!("native Linux FUSE mount/read/detach/join PASS; no product qualification claimed");
    Ok(())
}
