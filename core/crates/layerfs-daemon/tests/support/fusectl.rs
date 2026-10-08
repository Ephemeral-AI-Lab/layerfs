//! The kernel's FUSE control filesystem, mounted by the test and read only for
//! the test's own connection.
//!
//! SAFETY RULE. `/sys/fs/fuse/connections` lists every FUSE connection of the
//! host kernel, including those of other, protected containers. Nothing here
//! opens a file under it for writing, and no caller may. The directory itself
//! is never listed: a `Connection` is built from one mount's Ready receipt
//! after its device minor was checked against the test's own mount-table row,
//! and only `<minor>/…` of that connection is ever stat-ed or read. The one
//! write under this tree is the product's own abort control.
use layerfs_bridge::control::ReadyMount;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

pub const CONNECTIONS: &str = "/sys/fs/fuse/connections";

/// The mount-table row of `directory`: filesystem type and device numbers.
fn row(directory: &str) -> Option<(String, u32, u32)> {
    let table = fs::read_to_string("/proc/self/mountinfo").unwrap();
    table.lines().rev().find_map(|line| {
        let (before, after) = line.split_once(" - ")?;
        let fields: Vec<&str> = before.split(' ').collect();
        if fields.get(4) != Some(&directory) {
            return None;
        }
        let (major, minor) = fields.get(2)?.split_once(':')?;
        Some((
            after.split(' ').next()?.to_owned(),
            major.parse().ok()?,
            minor.parse().ok()?,
        ))
    })
}
/// True once this mount namespace has the control filesystem mounted.
pub fn mounted() -> bool {
    row(CONNECTIONS).is_some_and(|(filesystem, _, _)| filesystem == "fusectl")
}
/// Mounts the control filesystem once, before any Attach that should bind its
/// abort control. The `mount` process is launched and reaped here.
pub fn mount() {
    if mounted() {
        return;
    }
    let output = Command::new("mount")
        .args(["-t", "fusectl", "none", CONNECTIONS])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success() && mounted(),
        "mount -t fusectl: {:?} {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
/// The test's own connection: one device minor, established both ways.
#[derive(Clone, Debug)]
pub struct Connection {
    pub minor: u32,
}
/// Binds the accessors to `ready`'s connection. Panics unless the receipt's
/// device equals the mount-table row of the receipt's own directory.
pub fn own(ready: &ReadyMount) -> Connection {
    let (filesystem, major, minor) = row(&ready.directory)
        .unwrap_or_else(|| panic!("no mount-table row for {}", ready.directory));
    assert_eq!(filesystem, "fuse", "{}", ready.directory);
    assert_eq!(
        (major, minor),
        (ready.receipt.device_major, ready.receipt.device_minor),
        "mount-table device of {} differs from its Ready receipt",
        ready.directory
    );
    assert!(mounted(), "fusectl is not mounted");
    Connection { minor }
}
impl Connection {
    fn path(&self, name: &str) -> PathBuf {
        PathBuf::from(CONNECTIONS)
            .join(self.minor.to_string())
            .join(name)
    }
    /// One stat of this connection's own directory; nothing is listed.
    pub fn exists(&self) -> bool {
        fs::symlink_metadata(self.path("")).is_ok()
    }
    fn number(&self, name: &str) -> u64 {
        let path = self.path(name);
        fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
            .trim()
            .parse()
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }
    /// Requests the kernel holds for this connection, queued or in flight.
    pub fn waiting(&self) -> u64 {
        self.number("waiting")
    }
    pub fn max_background(&self) -> u64 {
        self.number("max_background")
    }
    pub fn congestion_threshold(&self) -> u64 {
        self.number("congestion_threshold")
    }
}
