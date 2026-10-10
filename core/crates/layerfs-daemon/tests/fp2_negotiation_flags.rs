//! FP-2, the flag half (R8b track T-G; also R2-STEP-2): the negotiated
//! profile of one real kernel mount compared, flag by flag and limit by
//! limit, with specification section 8.1
//! (`core/docs/issues/307/S8-SPECIFICATION-20261008.md`).
//!
//! What is compared, explicitly:
//!
//! - The Ready receipt's `selected` set is exactly `FUSE_ASYNC_READ |
//!   FUSE_BIG_WRITES`, plus `FUSE_MAX_PAGES` when the kernel offered it: the
//!   three flags section 8.1 names as defaults of the pinned library "that
//!   cannot be removed". Every other flag the pinned library knows is asserted
//!   absent by name, and the twelve flags section 8.1 forbids are asserted
//!   absent one by one. The names and bit values below are those of
//!   `core/vendor/fuser-0.18.0/src/ll/flags/init_flags.rs`; the daemon package
//!   has no direct dependency on that library, so they are written here.
//! - The receipt's limits are section 8.1's: `max_write` and `max_readahead`
//!   131072, `max_background` 1, congestion threshold 1.
//! - The kernel's own mount-table row of the mount (`/proc/self/mountinfo`)
//!   has exactly the options of section 8.1 and the receipt's mount id and
//!   device, and the connection's own `/sys/fs/fuse/connections/<minor>/`
//!   files read back the receipt's background limits.
//! - The page size is recorded in the test output, beside the kernel's own.
//!
//! Limit of the first comparison, by source: the receipt's `selected` is a
//! value the product computes (`layerfs-fuse` `mount/profile.rs`), not a
//! read-back of the INIT reply. The second test therefore asks the kernel:
//! four of the forbidden capabilities have a request signature, and the
//! connection's own opcode counts in its drain receipt show none of them in
//! effect. The other forbidden flags have no single-request signature and
//! stay at receipt scope.
//!
//! NOT in scope, and not asserted here: the observed maximum READ and WRITE
//! request sizes of the FP-2 row. No public observation of a request's size
//! exists, and whether a system-call fixture on the test's own threads is
//! acceptable is a pending owner ruling (plan section 6).
//!
//! Reported, not resolved: section 8.1 says "Loops | 2"; the receipt and
//! `layerfs-fuse` `dispatch/types.rs` (`RECEIVE_SLOTS = 1`) say one. The test
//! prints the conflict and asserts only that the receipt, control Status and
//! the process's own thread table agree with each other.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/fusectl.rs"]
mod fusectl;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
use layerfs_bridge::control::{NativePhase, Reply};
use layerfs_fuse::request::Opcode;
use mounted::{mount_entry, until};
use nix::{
    libc,
    unistd::{Gid, Uid},
};
use rig::{root, Rig};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    process::Command as Process,
};

/// Every init flag the pinned library names for Linux, with its bit.
const NAMED: [(&str, u32); 43] = [
    ("FUSE_ASYNC_READ", 0),
    ("FUSE_POSIX_LOCKS", 1),
    ("FUSE_FILE_OPS", 2),
    ("FUSE_ATOMIC_O_TRUNC", 3),
    ("FUSE_EXPORT_SUPPORT", 4),
    ("FUSE_BIG_WRITES", 5),
    ("FUSE_DONT_MASK", 6),
    ("FUSE_SPLICE_WRITE", 7),
    ("FUSE_SPLICE_MOVE", 8),
    ("FUSE_SPLICE_READ", 9),
    ("FUSE_FLOCK_LOCKS", 10),
    ("FUSE_HAS_IOCTL_DIR", 11),
    ("FUSE_AUTO_INVAL_DATA", 12),
    ("FUSE_DO_READDIRPLUS", 13),
    ("FUSE_READDIRPLUS_AUTO", 14),
    ("FUSE_ASYNC_DIO", 15),
    ("FUSE_WRITEBACK_CACHE", 16),
    ("FUSE_NO_OPEN_SUPPORT", 17),
    ("FUSE_PARALLEL_DIROPS", 18),
    ("FUSE_HANDLE_KILLPRIV", 19),
    ("FUSE_POSIX_ACL", 20),
    ("FUSE_ABORT_ERROR", 21),
    ("FUSE_MAX_PAGES", 22),
    ("FUSE_CACHE_SYMLINKS", 23),
    ("FUSE_NO_OPENDIR_SUPPORT", 24),
    ("FUSE_EXPLICIT_INVAL_DATA", 25),
    ("FUSE_MAP_ALIGNMENT", 26),
    ("FUSE_SUBMOUNTS", 27),
    ("FUSE_HANDLE_KILLPRIV_V2", 28),
    ("FUSE_SETXATTR_EXT", 29),
    ("FUSE_INIT_EXT", 30),
    ("FUSE_INIT_RESERVED", 31),
    ("FUSE_SECURITY_CTX", 32),
    ("FUSE_HAS_INODE_DAX", 33),
    ("FUSE_CREATE_SUPP_GROUP", 34),
    ("FUSE_HAS_EXPIRE_ONLY", 35),
    ("FUSE_DIRECT_IO_ALLOW_MMAP", 36),
    ("FUSE_PASSTHROUGH", 37),
    ("FUSE_NO_EXPORT_SUPPORT", 38),
    ("FUSE_HAS_RESEND", 39),
    ("FUSE_ALLOW_IDMAP", 40),
    ("FUSE_OVER_IO_URING", 41),
    ("FUSE_REQUEST_TIMEOUT", 42),
];
/// Section 8.1, "Request window" row and the note of its last flag row:
/// "ASYNC_READ, BIG_WRITES and MAX_PAGES are fuser defaults that cannot be
/// removed". The first two are required of the kernel; the third is selected
/// exactly when the kernel offers it.
const REQUIRED: [&str; 2] = ["FUSE_ASYNC_READ", "FUSE_BIG_WRITES"];
const WHEN_OFFERED: &str = "FUSE_MAX_PAGES";
/// Section 8.1: "Writeback | `FUSE_WRITEBACK_CACHE` not requested" and "Not
/// requested, absence asserted | `AUTO_INVAL_DATA`, `ATOMIC_O_TRUNC`, open-less
/// flags, both `KILLPRIV` flags, READDIRPLUS, PARALLEL_DIROPS, CACHE_SYMLINKS,
/// EXPLICIT_INVAL_DATA". "Open-less flags" are the two no-open capabilities,
/// and READDIRPLUS is its capability and its automatic mode.
const FORBIDDEN: [&str; 12] = [
    "FUSE_WRITEBACK_CACHE",
    "FUSE_AUTO_INVAL_DATA",
    "FUSE_ATOMIC_O_TRUNC",
    "FUSE_NO_OPEN_SUPPORT",
    "FUSE_NO_OPENDIR_SUPPORT",
    "FUSE_HANDLE_KILLPRIV",
    "FUSE_HANDLE_KILLPRIV_V2",
    "FUSE_DO_READDIRPLUS",
    "FUSE_READDIRPLUS_AUTO",
    "FUSE_PARALLEL_DIROPS",
    "FUSE_CACHE_SYMLINKS",
    "FUSE_EXPLICIT_INVAL_DATA",
];
/// Section 8.1, "Request window".
const WINDOW: u32 = 131_072;

fn bit(name: &str) -> u64 {
    let (_, shift) = NAMED
        .iter()
        .find(|(known, _)| *known == name)
        .unwrap_or_else(|| panic!("{name} is not a flag of the pinned library"));
    1 << shift
}
/// The names of the set bits; a bit the pinned library does not name is
/// shown by its position.
fn names(flags: u64) -> Vec<String> {
    (0..64)
        .filter(|shift| flags & (1 << shift) != 0)
        .map(|shift| {
            NAMED
                .iter()
                .find(|(_, known)| *known == shift)
                .map_or_else(|| format!("(1 << {shift})"), |(name, _)| (*name).to_owned())
        })
        .collect()
}
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "FP-2 GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}
/// The kernel's own row of one mount point, read with no daemon cooperation.
#[derive(Debug)]
struct Row {
    id: u64,
    device: (u32, u32),
    options: BTreeSet<String>,
    filesystem: String,
    source: String,
    super_options: BTreeSet<String>,
}
fn row(directory: &str) -> Row {
    let table = fs::read_to_string("/proc/self/mountinfo").unwrap();
    let found = table.lines().rev().find_map(|line| {
        let (before, after) = line.split_once(" - ")?;
        let fields: Vec<&str> = before.split(' ').collect();
        if fields.get(4) != Some(&directory) {
            return None;
        }
        let (major, minor) = fields.get(2)?.split_once(':')?;
        let set = |text: &str| text.split(',').map(str::to_owned).collect();
        let mut tail = after.split(' ');
        Some(Row {
            id: fields.first()?.parse().ok()?,
            device: (major.parse().ok()?, minor.parse().ok()?),
            options: set(fields.get(5)?),
            filesystem: tail.next()?.to_owned(),
            source: tail.next()?.to_owned(),
            super_options: set(tail.next()?),
        })
    });
    found.unwrap_or_else(|| panic!("no mount-table row for {directory}"))
}
fn set(options: &[String]) -> BTreeSet<String> {
    options.iter().cloned().collect()
}
/// Receive-loop threads of this process, by the name the pinned library
/// gives them (`fuser-<index>`), from the kernel's thread table.
fn loop_threads() -> usize {
    fs::read_dir("/proc/self/task")
        .unwrap()
        .filter(|task| {
            let name = fs::read_to_string(task.as_ref().unwrap().path().join("comm"));
            name.is_ok_and(|name| name.trim_end().starts_with("fuser-"))
        })
        .count()
}
/// Per-opcode frame counts out of a Debug-rendered drain receipt.
fn opcodes(receipt: &str) -> Vec<u64> {
    let key = "OpcodeWork { opcodes: [";
    let at = receipt.find(key).unwrap_or_else(|| panic!("{receipt}")) + key.len();
    let end = at + receipt[at..].find(']').unwrap();
    receipt[at..end]
        .split(", ")
        .map(|count| count.parse().unwrap())
        .collect()
}

/// FP-2, receipt half: selected flags, forbidden flags, limits, the mount
/// table, the connection's own control files and the page size.
#[test]
fn fp2_the_ready_receipt_is_exactly_the_section_8_1_profile_with_every_forbidden_flag_absent() {
    fusectl::mount();
    let rig = Rig::new("fp2-flags");
    let helper = rig.harness.bind(1);
    assert_eq!(loop_threads(), 0, "no receive loop before the Attach");
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let receipt = ready.receipt;
    let (offered, selected) = (receipt.offered, receipt.selected);
    println!(
        "FP-2 receipt: abi={}.{} offered={offered:#x} selected={selected:#x} max_write={} max_readahead={} max_background={} congestion_threshold={} page_size={} loops={} abort_bound={}",
        receipt.abi_major,
        receipt.abi_minor,
        receipt.max_write,
        receipt.max_readahead,
        receipt.max_background,
        receipt.congestion_threshold,
        receipt.page_size,
        receipt.loops,
        receipt.abort_bound
    );
    println!("FP-2 offered flags: {:?}", names(offered));
    println!("FP-2 selected flags: {:?}", names(selected));

    // Selected: the two required defaults, and MAX_PAGES exactly when offered.
    for name in REQUIRED {
        assert_ne!(offered & bit(name), 0, "{name} is offered by the kernel");
        assert_ne!(selected & bit(name), 0, "{name} is selected");
    }
    assert_eq!(
        selected & bit(WHEN_OFFERED),
        offered & bit(WHEN_OFFERED),
        "{WHEN_OFFERED} is selected exactly when the kernel offers it"
    );
    let expected = bit(REQUIRED[0]) | bit(REQUIRED[1]) | (offered & bit(WHEN_OFFERED));
    assert_eq!(
        selected,
        expected,
        "selected is exactly {:?}, not {:?}",
        names(expected),
        names(selected)
    );
    assert_eq!(selected & !offered, 0, "nothing unoffered is selected");

    // Forbidden: each flag of section 8.1, by name.
    let mut offered_forbidden = Vec::new();
    for name in FORBIDDEN {
        assert_eq!(
            selected & bit(name),
            0,
            "{name} is forbidden by section 8.1 and is in the selected set"
        );
        if offered & bit(name) != 0 {
            offered_forbidden.push(name);
        }
    }
    println!(
        "FP-2 forbidden flags: {} asserted absent by name; offered by this kernel and not selected: {offered_forbidden:?}",
        FORBIDDEN.len()
    );
    // Every other flag the pinned library names, by name, and no unnamed bit.
    let mut known = 0;
    let mut absent = 0;
    for (name, shift) in NAMED {
        known |= 1u64 << shift;
        if REQUIRED.contains(&name) || name == WHEN_OFFERED {
            continue;
        }
        assert_eq!(
            selected & (1 << shift),
            0,
            "{name} is in the selected set and is not part of the profile"
        );
        absent += 1;
    }
    assert_eq!(
        selected & !known,
        0,
        "a selected bit the pinned library does not name: {:?}",
        names(selected & !known)
    );
    assert_eq!(absent, NAMED.len() - 3);
    println!(
        "FP-2 selected set: exactly {:?}; {absent} other named flags asserted absent; unnamed selected bits=0",
        names(selected)
    );

    // Limits of section 8.1.
    assert_eq!(receipt.abi_major, 7);
    assert_eq!(
        (receipt.max_write, receipt.max_readahead),
        (WINDOW, WINDOW),
        "request window"
    );
    assert_eq!(
        (receipt.max_background, receipt.congestion_threshold),
        (1, 1),
        "background"
    );

    // Page size: recorded, and the kernel's own.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    println!(
        "FP-2 page size: receipt={} sysconf(_SC_PAGESIZE)={page}",
        receipt.page_size
    );
    assert_eq!(i64::from(receipt.page_size), page);

    // The kernel's mount table: exactly the options of section 8.1, and the
    // receipt's own mount identity.
    let table = row(&ready.directory);
    println!("FP-2 mountinfo: {table:?}");
    assert_eq!(
        (table.filesystem.as_str(), table.source.as_str()),
        ("fuse", "layerfs")
    );
    assert_eq!(table.id, receipt.mount_id, "mount id");
    assert_eq!(
        table.device,
        (receipt.device_major, receipt.device_minor),
        "connection device"
    );
    assert_eq!(
        table.options,
        set(&["rw", "nosuid", "nodev", "noatime"].map(str::to_owned)),
        "per-mount options"
    );
    assert_eq!(
        table.super_options,
        set(&[
            "rw".to_owned(),
            format!("user_id={}", Uid::effective()),
            format!("group_id={}", Gid::effective()),
            "default_permissions".to_owned(),
            "allow_other".to_owned(),
            format!("max_read={}", receipt.max_readahead),
        ]),
        "connection options"
    );
    assert!(
        table.super_options.contains(&format!("max_read={WINDOW}")),
        "the kernel's own READ bound is section 8.1's window"
    );

    // The connection's own control files agree with the receipt.
    let connection = fusectl::own(&ready);
    assert!(connection.exists());
    let (background, congestion) = (
        connection.max_background(),
        connection.congestion_threshold(),
    );
    println!(
        "FP-2 fusectl: connection={} max_background={background} congestion_threshold={congestion} waiting={}",
        connection.minor,
        connection.waiting()
    );
    assert_eq!(background, u64::from(receipt.max_background));
    assert_eq!(congestion, u64::from(receipt.congestion_threshold));
    assert!(receipt.abort_bound, "the abort control was bound at Attach");

    // Loops: reported against section 8.1, asserted only for agreement of
    // the receipt, control Status and the kernel's thread table.
    let status = rig.harness.status(ready.token);
    let native = status.native.unwrap();
    assert_eq!(native.phase, NativePhase::Ready);
    let work = native.work.unwrap();
    let threads = loop_threads();
    println!(
        "FP-2 SPEC_CONFLICT loops: specification section 8.1 says \"Loops | 2\"; receipt.loops={} status(loops_configured={} loops_entered={}) receive-loop threads in /proc/self/task={threads}; reported, not resolved",
        receipt.loops, work.loops_configured, work.loops_entered
    );
    assert_eq!(
        (work.loops_configured, work.loops_entered),
        (receipt.loops, receipt.loops),
        "{work:?}"
    );
    assert_eq!(threads, usize::from(receipt.loops), "thread count");
    // Reported, not judged. The pinned library hides FUSE_INIT_EXT from the
    // offered set it shows the filesystem (`KernelConfig::capabilities`) and
    // always adds it to its INIT reply when the kernel sent it
    // (`ll/request.rs`, `Init::reply`). An offered bit above 31 can only
    // have come from the extended field, so it shows the kernel sent it.
    println!(
        "FP-2 NOTE receipt.selected is computed by the product (mount/profile.rs), not read back from the INIT reply. FUSE_INIT_EXT: in receipt.offered={} in receipt.selected={} extended offered bits present={} (the kernel sent it and the library's reply carries it; neither receipt set lists it)",
        offered & bit("FUSE_INIT_EXT") != 0,
        selected & bit("FUSE_INIT_EXT") != 0,
        offered >> 32 != 0
    );

    rig.unmount(&ready);
    drop(guard);
    // Joined by the drain; its row leaves the thread table a moment later.
    until("the receive loop left the thread table", || {
        loop_threads() == 0
    });
    let closed = rig.harness.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
}

/// FP-2, kernel half of four forbidden capabilities. The kernel changes what
/// it sends when one of them is in effect, so the connection's own opcode
/// counts are an observation that does not rest on the receipt:
///
/// - READDIRPLUS: a listing whose entries are then stat-ed is served by
///   READDIR frames and no READDIRPLUS frame.
/// - CACHE_SYMLINKS: two `readlink` calls of one symlink are two READLINK
///   frames; a cached target would have made one.
/// - ATOMIC_O_TRUNC: `open(O_TRUNC)` of a file with bytes sends one size-0
///   SETATTR beside its OPEN.
/// - WRITEBACK_CACHE: five `write` calls of one byte each are five WRITE
///   frames, each before its call returned; a writeback cache would have
///   kept them for one later frame.
#[test]
fn fp2_the_kernel_sends_the_requests_of_a_connection_without_four_forbidden_capabilities() {
    let rig = Rig::new("fp2-behaviour");
    let helper = rig.harness.bind(1);
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let mount = root(&ready);

    let mut listed = BTreeSet::new();
    for entry in fs::read_dir(mount.join("node_modules/pkg")).unwrap() {
        let entry = entry.unwrap();
        let metadata = fs::symlink_metadata(entry.path()).unwrap();
        listed.insert((entry.file_name().into_string().unwrap(), metadata.ino()));
    }
    let listing: Vec<&str> = listed.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(listing, ["index.js", "lib", "package.json"]);

    let link = mount.join("dangling");
    let targets = [fs::read_link(&link).unwrap(), fs::read_link(&link).unwrap()];
    assert_eq!(targets[0].to_str(), Some("missing/target"));
    assert_eq!(targets[0], targets[1]);

    let path = mount.join("docs/guide.md");
    assert_eq!(fs::metadata(&path).unwrap().len(), 5_000);
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();
    assert_eq!(file.metadata().unwrap().len(), 0, "truncated at open");
    for _ in 0..5 {
        assert_eq!(file.write(b"x").unwrap(), 1);
    }
    drop(file);
    assert_eq!(fs::read(&path).unwrap(), b"xxxxx");

    let work = rig
        .harness
        .status(ready.token)
        .native
        .unwrap()
        .work
        .unwrap();
    assert_eq!((work.retained, work.terminal, work.unadmitted), (0, 0, 0));
    let receipt = rig.unmount(&ready);
    let counts = opcodes(&receipt);
    let count = |opcode: Opcode| counts[opcode as usize];
    println!(
        "FP-2 kernel behaviour: readdir={} readdirplus={} readlink={} open={} setattr={} write={} flush={} selected={:?}",
        count(Opcode::Readdir),
        count(Opcode::Readdirplus),
        count(Opcode::Readlink),
        count(Opcode::Open),
        count(Opcode::Setattr),
        count(Opcode::Write),
        count(Opcode::Flush),
        names(ready.receipt.selected)
    );
    assert!(count(Opcode::Readdir) >= 1, "{receipt}");
    assert_eq!(
        count(Opcode::Readdirplus),
        0,
        "READDIRPLUS is not in effect: {receipt}"
    );
    assert_eq!(
        count(Opcode::Readlink),
        2,
        "CACHE_SYMLINKS is not in effect: {receipt}"
    );
    assert_eq!(
        count(Opcode::Setattr),
        1,
        "ATOMIC_O_TRUNC is not in effect: {receipt}"
    );
    assert_eq!(
        count(Opcode::Write),
        5,
        "WRITEBACK_CACHE is not in effect: {receipt}"
    );
    drop(guard);
    let closed = rig.harness.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
}
