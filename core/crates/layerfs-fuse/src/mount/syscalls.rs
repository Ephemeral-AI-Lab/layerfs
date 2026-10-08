//! First-party device, direct mount, one plain detach and one abort write; no
//! helper or lazy path.
use nix::{
    errno::Errno,
    mount::{mount, umount2, MntFlags, MsFlags},
    unistd::write,
};
use std::{
    ffi::OsStr,
    fs::{DirBuilder, File, OpenOptions},
    io::{self, BufRead, BufReader},
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::{ffi::OsStrExt, fs::DirBuilderExt},
    },
    path::Path,
};

const DEVICE: &str = "/dev/fuse";
const MOUNT_TABLE: &str = "/proc/self/mountinfo";
const CONTROL: &str = "/sys/fs/fuse/connections";
/// The type and source this product mounts with, and the only mount-table row
/// whose connection it will ever bind an abort control for.
const FILESYSTEM: &str = "fuse";
const SOURCE: &str = "layerfs";

/// The original result of the one abort write. Only `Written` says the
/// kernel accepted the byte; it is not loop-exit or detach evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbortWrite {
    Written,
    /// The call returned without error and without the one byte.
    Short(usize),
    Failed(Errno),
}

/// The kernel's own record of this mount; nothing here is inferred from the
/// requested options or from an image/deployment identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountEntry {
    pub id: u64,
    /// Anonymous device major/minor; the minor names the control connection.
    pub device: (u32, u32),
    pub options: String,
    pub filesystem: String,
    pub source: String,
    pub super_options: String,
}

/// One create attempt; an existing directory is never adopted as a mount home.
pub(crate) fn create_directory(target: &Path) -> io::Result<()> {
    DirBuilder::new().mode(0o755).create(target)
}
pub(crate) fn open_device() -> io::Result<OwnedFd> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(DEVICE)
        .map(OwnedFd::from)
}
/// The daemon is the mount owner; ordinary commands enter through allow_other
/// under kernel permission checks, so they cannot abort this connection.
pub(crate) fn attach(device: &OwnedFd, target: &Path, uid: u32, gid: u32) -> Result<(), Errno> {
    let data = format!(
        "fd={},rootmode=40000,user_id={uid},group_id={gid},allow_other,default_permissions,max_read={}",
        device.as_raw_fd(),
        layerfs_overlay::READ_WINDOW
    );
    mount(
        Some(SOURCE),
        target,
        Some(FILESYSTEM),
        MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOATIME,
        Some(data.as_str()),
    )
}
/// Exactly one plain attempt. EBUSY is the kernel's reversible answer; no
/// MNT_DETACH, MNT_FORCE or second call follows any outcome.
pub(crate) fn detach(target: &Path) -> Result<(), Errno> {
    umount2(target, MntFlags::empty())
}
/// Reads the topmost entry mounted at exactly this path. It issues no request
/// to the filesystem being described.
pub(crate) fn mount_entry(target: &Path) -> io::Result<MountEntry> {
    let mut found = None;
    for line in BufReader::new(File::open(MOUNT_TABLE)?).split(b'\n') {
        let line = line?;
        if let Some(entry) = parse(&line, target) {
            found = Some(entry);
        }
    }
    found.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "native mount table entry"))
}
/// Bound to this exact connection at attach. Absence is reported, and forced
/// teardown later refuses before effects rather than reopening a guessed path.
/// The control tree lists every connection of the kernel, so only this
/// product's own row names one: an anonymous device (major 0, whose minor is
/// the connection number) of type `fuse` with this product's source. Any
/// other row binds nothing and opens nothing.
pub(crate) fn abort_control(entry: &MountEntry) -> io::Result<Option<File>> {
    if entry.device.0 != 0 || entry.filesystem != FILESYSTEM || entry.source != SOURCE {
        return Ok(None);
    }
    let path = Path::new(CONTROL)
        .join(entry.device.1.to_string())
        .join("abort");
    match OpenOptions::new().write(true).open(path) {
        Ok(file) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}
/// Exactly one `write(2)` of one byte on the bound control: no second call
/// after a short count, an interruption or any other error.
pub(crate) fn abort(control: &File) -> AbortWrite {
    match write(control, b"1") {
        Ok(1) => AbortWrite::Written,
        Ok(count) => AbortWrite::Short(count),
        Err(errno) => AbortWrite::Failed(errno),
    }
}
fn parse(line: &[u8], target: &Path) -> Option<MountEntry> {
    let text = |field: &[u8]| String::from_utf8(field.to_vec()).ok();
    let mut fields = line.split(|byte| *byte == b' ');
    let id = text(fields.next()?)?.parse().ok()?;
    fields.next()?;
    let device = text(fields.next()?)?;
    let (major, minor) = device.split_once(':')?;
    fields.next()?;
    if Path::new(OsStr::from_bytes(&unescape(fields.next()?))) != target {
        return None;
    }
    let options = text(fields.next()?)?;
    // Optional propagation fields end at the single dash separator.
    fields.find(|field| *field == b"-")?;
    Some(MountEntry {
        id,
        device: (major.parse().ok()?, minor.parse().ok()?),
        options,
        filesystem: text(fields.next()?)?,
        source: text(&unescape(fields.next()?))?,
        super_options: text(fields.next()?)?,
    })
}
/// The table writes space, tab, newline and backslash as three octal digits.
fn unescape(field: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(field.len());
    let mut index = 0;
    while index < field.len() {
        let digits = field.get(index + 1..index + 4);
        match digits {
            Some(digits)
                if field[index] == b'\\' && digits.iter().all(|d| (b'0'..=b'7').contains(d)) =>
            {
                let value = digits
                    .iter()
                    .fold(0u32, |sum, digit| sum * 8 + u32::from(digit - b'0'));
                out.push(value as u8);
                index += 4;
            }
            _ => {
                out.push(field[index]);
                index += 1;
            }
        }
    }
    out
}
