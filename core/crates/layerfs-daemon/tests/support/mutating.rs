//! Shared observations for mounted mutation proofs: uncached daemon reads,
//! unregistered command processes and the refusals that remain on purpose.
use super::{
    fixture,
    mounted::{until, Harness, COMMAND},
};
use layerfs_bridge::control::{ReadyMount, WorkspaceToken};
use layerfs_daemon::control::Success;
use layerfs_history::BranchId;
use nix::{
    errno::Errno,
    libc,
    sys::stat::{mknod, Mode, SFlag},
    sys::statvfs::statvfs,
    unistd::{chown, Gid, Uid},
};
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
            process::CommandExt,
        },
    },
    path::Path,
    process::{Command, Output, Stdio},
};

pub fn harness(label: &str) -> (fixture::Fixture, Harness) {
    let f = fixture::Fixture::labeled(fixture::Shape::Mixed, label);
    let branch = BranchId::from_slice(&f.manifest.branch).unwrap();
    let h = Harness::new(f.opened.store.clone(), &f.directory, branch);
    (f, h)
}
pub fn errno(error: &io::Error) -> Errno {
    Errno::from_raw(error.raw_os_error().unwrap())
}
pub fn c(path: &Path) -> CString {
    CString::new(path.as_os_str().as_bytes()).unwrap()
}
/// The file's bytes read with `O_DIRECT`: every byte is a READ served by the
/// daemon from published state, never a page the kernel already held.
pub fn direct(path: &Path) -> Vec<u8> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECT)
        .open(path)
        .unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}
/// An ordinary process that was never registered with the daemon: launched
/// here by plain exec under the given identity.
pub fn shell(identity: u32, script: &str) -> Output {
    Command::new("sh")
        .args(["-c", script])
        .uid(identity)
        .gid(identity)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
/// Bounded wait for asynchronous RELEASE and post-reply releases to finish.
pub fn quiet(h: &Harness, token: WorkspaceToken) {
    until("connection quiescent", || {
        let work = h.status(token).native.unwrap().work.unwrap();
        work.received == 0 && work.admitted == 0
    });
}
/// Normal terminal unmount with a complete drain; returns its receipt text.
pub fn unmounted(h: &Harness, ready: &ReadyMount) -> String {
    quiet(h, ready.token);
    let done: Success = h.unmount(ready);
    format!("{:?}", done.native)
}
/// One named counter out of a Debug-rendered receipt.
pub fn counter(receipt: &str, name: &str) -> u64 {
    let key = format!("{name}: ");
    let at = receipt
        .find(&key)
        .unwrap_or_else(|| panic!("{name}: {receipt}"));
    receipt[at + key.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}
fn check(result: libc::c_long) -> Result<libc::c_long, Errno> {
    Errno::result(result)
}
/// What stays refused once ordinary mutation is served. None of these changes
/// the Workspace; every one is an ordinary errno to the caller.
pub fn refusals(root: &Path) {
    let long = "n".repeat(256);
    assert_eq!(
        errno(&fs::symlink_metadata(root.join(&long)).unwrap_err()),
        Errno::ENAMETOOLONG
    );
    assert_eq!(
        errno(&File::create(root.join(&long)).unwrap_err()),
        Errno::ENAMETOOLONG
    );
    let file = root.join("empty-file");
    let before = fs::metadata(&file).unwrap();
    // Only regular files, directories and symlinks exist in the format.
    for kind in [SFlag::S_IFIFO, SFlag::S_IFSOCK] {
        assert_eq!(
            mknod(
                &root.join("special"),
                kind,
                Mode::from_bits_truncate(0o600),
                0
            )
            .unwrap_err(),
            Errno::EPERM,
            "{kind:?}"
        );
    }
    assert_eq!(
        errno(&fs::symlink_metadata(root.join("special")).unwrap_err()),
        Errno::ENOENT
    );
    // Set-id bits, and a sticky bit on a regular file, are not representable.
    for mode in [0o4755, 0o2755, 0o1644] {
        assert_eq!(
            errno(&fs::set_permissions(&file, fs::Permissions::from_mode(mode)).unwrap_err()),
            Errno::EPERM,
            "{mode:o}"
        );
    }
    // Ownership is the configured command identity and is not stored: naming
    // it changes nothing, any other owner is refused.
    chown(
        &file,
        Some(Uid::from_raw(COMMAND)),
        Some(Gid::from_raw(COMMAND)),
    )
    .unwrap();
    assert_eq!(
        chown(&file, Some(Uid::from_raw(0)), None).unwrap_err(),
        Errno::EPERM
    );
    assert_eq!(
        chown(&file, None, Some(Gid::from_raw(1))).unwrap_err(),
        Errno::EPERM
    );
    // A hard link to a symlink is not permitted; to a directory the kernel
    // itself refuses.
    let link = |from: &str, flags| {
        check(unsafe {
            libc::linkat(
                libc::AT_FDCWD,
                c(&root.join(from)).as_ptr(),
                libc::AT_FDCWD,
                c(&root.join("refused-link")).as_ptr(),
                flags,
            )
        } as libc::c_long)
    };
    assert_eq!(link("broken", 0).unwrap_err(), Errno::EPERM);
    assert_eq!(link("empty", 0).unwrap_err(), Errno::EPERM);
    // Exchange renames are not implemented.
    assert_eq!(
        check(unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                c(&file).as_ptr(),
                libc::AT_FDCWD,
                c(&root.join("ignored.bin")).as_ptr(),
                libc::RENAME_EXCHANGE,
            )
        } as libc::c_long)
        .unwrap_err(),
        Errno::EINVAL
    );
    // No extended attributes and no preallocation.
    let name = CString::new("user.layerfs").unwrap();
    let mut value = [0_u8; 8];
    assert_eq!(
        check(unsafe {
            libc::setxattr(
                c(&file).as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        } as libc::c_long)
        .unwrap_err(),
        Errno::EOPNOTSUPP
    );
    assert_eq!(
        check(unsafe {
            libc::getxattr(
                c(&file).as_ptr(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
            )
        } as libc::c_long)
        .unwrap_err(),
        Errno::EOPNOTSUPP
    );
    let writable = OpenOptions::new().write(true).open(&file).unwrap();
    assert_eq!(
        check(unsafe { libc::fallocate(writable.as_raw_fd(), 0, 0, 4096) } as libc::c_long)
            .unwrap_err(),
        Errno::EOPNOTSUPP
    );
    // FLUSH/FSYNC/FSYNCDIR succeed with no engine or durability work.
    writable.sync_all().unwrap();
    writable.sync_data().unwrap();
    drop(writable);
    File::open(root.join(".git")).unwrap().sync_all().unwrap();
    let after = fs::metadata(&file).unwrap();
    assert_eq!(
        (
            after.len(),
            after.mode(),
            after.nlink(),
            after.mtime(),
            after.mtime_nsec()
        ),
        (
            before.len(),
            before.mode(),
            before.nlink(),
            before.mtime(),
            before.mtime_nsec()
        ),
        "a refusal changes nothing"
    );
    assert_eq!(
        errno(&fs::symlink_metadata(root.join("refused-link")).unwrap_err()),
        Errno::ENOENT
    );
    let statistics = statvfs(root).unwrap();
    assert_eq!(statistics.block_size(), 4096);
    assert_eq!(statistics.fragment_size(), 4096);
    assert_eq!(statistics.name_max(), 255);
    assert!(statistics.blocks_free() > 0 && statistics.blocks_available() > 0);
    assert!(statistics.files_free() > 0);
}
pub fn bytes_of(length: usize, seed: u8) -> Vec<u8> {
    (0..length)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}
/// Requests the daemon has received on this mount so far, of every kind.
pub fn frames(h: &Harness, token: WorkspaceToken) -> u64 {
    quiet(h, token);
    let work = h.status(token).native.unwrap().work.unwrap();
    work.handoffs + work.inline + work.refused
}
/// Per-opcode frame counts out of a Debug-rendered drain receipt.
pub fn opcodes(receipt: &str) -> Vec<u64> {
    let key = "OpcodeWork { opcodes: [";
    let at = receipt.find(key).unwrap_or_else(|| panic!("{receipt}")) + key.len();
    let end = at + receipt[at..].find(']').unwrap();
    receipt[at..end]
        .split(", ")
        .map(|count| count.parse().unwrap())
        .collect()
}
/// Attributes the kernel fetched from the daemon for this call, never the
/// kernel's cached copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Forced {
    pub inode: u64,
    pub links: u32,
    pub size: u64,
    pub mode: u32,
}
pub fn forced(file: &File) -> Result<Forced, Errno> {
    let mut raw = std::mem::MaybeUninit::<libc::statx>::zeroed();
    check(unsafe {
        libc::statx(
            file.as_raw_fd(),
            c"".as_ptr(),
            libc::AT_EMPTY_PATH | libc::AT_STATX_FORCE_SYNC,
            libc::STATX_BASIC_STATS,
            raw.as_mut_ptr(),
        )
    } as libc::c_long)?;
    let raw = unsafe { raw.assume_init() };
    Ok(Forced {
        inode: raw.stx_ino,
        links: raw.stx_nlink,
        size: raw.stx_size,
        mode: u32::from(raw.stx_mode),
    })
}
/// A reference with no open-file custody: kernel lookup ownership only.
pub fn path_only(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)
        .unwrap()
}
/// One `MAP_SHARED` read-write mapping from offset 0.
pub struct Mapping {
    address: *mut u8,
    length: usize,
}
impl Mapping {
    pub fn shared(file: &File, length: usize) -> Self {
        let address = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        assert_ne!(address, libc::MAP_FAILED, "{}", io::Error::last_os_error());
        Self {
            address: address.cast(),
            length,
        }
    }
    pub fn store(&self, offset: usize, bytes: &[u8]) {
        assert!(offset + bytes.len() <= self.length);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.address.add(offset), bytes.len())
        }
    }
    pub fn load(&self, offset: usize, length: usize) -> Vec<u8> {
        assert!(offset + length <= self.length);
        unsafe { std::slice::from_raw_parts(self.address.add(offset), length) }.to_vec()
    }
    pub fn sync(&self) {
        check(
            unsafe { libc::msync(self.address.cast(), self.length, libc::MS_SYNC) } as libc::c_long,
        )
        .unwrap();
    }
    /// Unmaps with no `msync`.
    pub fn unmap(self) {
        check(unsafe { libc::munmap(self.address.cast(), self.length) } as libc::c_long).unwrap();
    }
}
