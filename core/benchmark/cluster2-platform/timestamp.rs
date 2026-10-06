//! S0 dependency capability diagnostic through actual Linux setattr requests.
use fuser::{
    BsdFileFlags, Config, Errno, FileAttr, FileHandle, FileType, Filesystem, INodeNo, MountOption,
    ReplyAttr, ReplyEntry, Request, TimeOrNow,
};
use std::{
    ffi::OsStr,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Probe {
    mtime: Mutex<SystemTime>,
}
impl Probe {
    fn attr(&self, ino: INodeNo) -> FileAttr {
        FileAttr {
            ino,
            size: 0,
            blocks: 0,
            atime: UNIX_EPOCH,
            mtime: *self.mtime.lock().unwrap(),
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: if ino == INodeNo::ROOT {
                FileType::Directory
            } else {
                FileType::RegularFile
            },
            perm: 0o777,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            flags: 0,
            blksize: 4096,
        }
    }
}
impl Filesystem for Probe {
    fn lookup(&self, _: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        if parent == INodeNo::ROOT && name == "probe" {
            reply.entry(
                &Duration::ZERO,
                &self.attr(INodeNo(2)),
                fuser::Generation(1),
            );
        } else {
            reply.error(Errno::ENOENT);
        }
    }
    fn getattr(&self, _: &Request, ino: INodeNo, _: Option<FileHandle>, reply: ReplyAttr) {
        reply.attr(&Duration::ZERO, &self.attr(ino));
    }
    fn setattr(
        &self,
        _: &Request,
        ino: INodeNo,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u64>,
        _: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _: Option<SystemTime>,
        _: Option<FileHandle>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        if let Some(TimeOrNow::SpecificTime(time)) = mtime {
            println!("received mtime={time:?}");
            *self.mtime.lock().unwrap() = time;
        }
        reply.attr(&Duration::ZERO, &self.attr(ino));
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("mount path")?;
    let mode = std::env::args().nth(2).ok_or("timestamp mode")?;
    let desired = match mode.as_str() {
        "negative" => UNIX_EPOCH - Duration::new(1, 200_000_000),
        "minimum-whole" => UNIX_EPOCH
            .checked_sub(Duration::new(i64::MIN.unsigned_abs(), 0))
            .ok_or("platform min")?,
        "minimum" => UNIX_EPOCH
            .checked_sub(Duration::new(i64::MIN.unsigned_abs(), 0))
            .ok_or("platform min")?
            .checked_add(Duration::new(0, 200_000_000))
            .ok_or("platform min+fraction")?,
        _ => return Err("unknown timestamp case".into()),
    };
    std::fs::create_dir(&path)?;
    let mut config = Config::default();
    config.n_threads = Some(2);
    config.mount_options = vec![
        MountOption::DefaultPermissions,
        MountOption::FSName("cluster2-time-risk".into()),
    ];
    let session = fuser::spawn_mount(
        Probe {
            mtime: Mutex::new(UNIX_EPOCH),
        },
        &path,
        &config,
    )?;
    println!("requested mtime={desired:?}");
    let operation = (|| -> std::io::Result<bool> {
        let file = std::fs::File::open(std::path::Path::new(&path).join("probe"))?;
        let outcome = file.set_times(std::fs::FileTimes::new().set_modified(desired));
        println!("set_times outcome={outcome:?}");
        let observed = file.metadata().and_then(|m| m.modified());
        println!("observed mtime={observed:?}");
        Ok(outcome.is_ok() && observed.as_ref().ok() == Some(&desired))
    })();
    let cleanup = session.umount_and_join();
    println!("native cleanup={cleanup:?}");
    std::fs::remove_dir(path)?;
    if !operation? || cleanup.is_err() {
        return Err("pinned FUSE timestamp capability FAILED".into());
    }
    Ok(())
}
