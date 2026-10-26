//! Real kernel mounts: exact FORGET decrements with whole-file disposal of
//! lookups still outstanding at detach (FP-31).
//!
//! Staging, hook-free: the test asks the kernel for reclaim of the test
//! container's own memory cgroup only, which makes it evict unreferenced
//! dentries and inodes and send FORGET on a connection that stays mounted. No
//! system-wide cache is dropped.
//!
//! Observed, by counts: the mount's maintained counters through control
//! Status, the engine's maintained row counts and point states through
//! ordinary owner jobs, the owner's maintenance counters, the drain receipt's
//! opcode counts and the kernel's own answers. Nothing is timed.
//!
//! Not staged here and recorded as such: a FORGET of a foreign incarnation
//! and an underflowing FORGET stay at component scope (FP-31).
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/holds.rs"]
mod holds;
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
use layerfs_bridge::control::{MountPhase, MountWork, Reply, WorkspaceToken};
use layerfs_daemon::OwnerClient;
use layerfs_fuse::request::Opcode;
use layerfs_overlay::{CleanupState, StoredCounts};
use mounted::mount_entry;
use nix::libc;
use rig::{root, stamp_tree, Rig};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Command as Process,
    thread,
    time::{Duration, Instant},
};

/// Every bounded readiness or completion observation of the tests.
const WAIT: Duration = Duration::from_secs(5);
/// Bound of one retire-until-gone observation.
const LONG: Duration = Duration::from_secs(20);
/// Bound of one reclaim-until-returned observation. How many requests the
/// kernel needs before it walks its dentry and inode lists is its own
/// business: in attempt 1 the two thousand inodes were not all returned
/// within 20 s, and attempt 2 needed 162 requests.
const RECLAIM: Duration = Duration::from_secs(40);
/// Rows one maintenance turn may retire.
/// Engine rows that hold the kernel's lookup custody of `inodes` inodes, of
/// which `directories` are directories: each inode has its lookup row and
/// its file-custody row, and a directory its retained parent row. No `lease`
/// row stands for a lookup reference or a descriptor.
#[derive(Debug, Eq, PartialEq)]
struct Owners {
    sql: StoredCounts,
    memory: layerfs_overlay::OpenStateCounts,
}
impl Owners {
    fn entries(&self) -> u64 {
        self.memory.values().into_iter().sum()
    }
}
fn owners(rig: &Rig, helper: WorkspaceToken) -> Owners {
    let resources = rig.harness.engine_resources(helper);
    Owners {
        sql: resources.counts,
        memory: resources.open,
    }
}
const fn custody(inodes: u64, directories: u64) -> u64 {
    2 * inodes + directories
}

fn before(what: &str, wait: Duration, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + wait;
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn within(what: &str, condition: impl FnMut() -> bool) {
    before(what, WAIT, condition)
}
/// Never leaves a kernel mount behind: if the test unwinds, one lazy
/// umount(8) launched and reaped by the test detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "DRAIN_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}

fn work(rig: &Rig, token: WorkspaceToken) -> MountWork {
    rig.harness.status(token).kernel.unwrap().work.unwrap()
}
/// Two consecutive observations with nothing received or admitted and no
/// request completed in between.
fn quiet(rig: &Rig, token: WorkspaceToken) {
    let mut last = None;
    within("connection quiescent", || {
        let now = work(rig, token);
        let settled = now.active == 0 && last == Some(now.completed);
        last = Some(now.completed);
        thread::sleep(Duration::from_millis(10));
        settled
    });
}
/// One maintained point observation of the namespace's close and cleanup.
fn cleanup(client: &OwnerClient, route: Route, incarnation: [u8; 32]) -> CleanupState {
    // The Workspace file can already be gone. Observe through the daemon
    // roster, which authenticates the original namespace and incarnation.

    client
        .observe_cleanup(None, route.namespace(), incarnation)
        .unwrap()
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
/// One named counter out of a Debug-rendered receipt.
fn counter(receipt: &str, name: &str) -> u64 {
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

/// Reclaim of the test container's own memory cgroup, and nothing wider.
struct Reclaim(PathBuf);
impl Reclaim {
    /// Declared precondition, not a product effect: this container's cgroup
    /// interface must be writable so reclaim can be requested for it. The
    /// file is the `memory.reclaim` of the cgroup this process is a member
    /// of, read from its own membership record.
    fn own() -> Self {
        let membership = fs::read_to_string("/proc/self/cgroup").unwrap();
        let relative = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .unwrap_or_else(|| panic!("precondition: a cgroup v2 membership: {membership}"));
        let file = Path::new("/sys/fs/cgroup")
            .join(relative.trim_start_matches('/'))
            .join("memory.reclaim");
        let remount = Process::new("mount")
            .args(["-o", "remount,rw", "/sys/fs/cgroup"])
            .output()
            .unwrap();
        assert!(
            remount.status.success() && file.exists(),
            "precondition: container-scoped {} is required: {remount:?}",
            file.display()
        );
        println!(
            "FP-31 reclaim scope: membership={:?} file={}",
            membership.trim(),
            file.display()
        );
        Self(file)
    }
    /// One reclaim request to the kernel, for more than the cgroup can give
    /// back so that its dentry and inode lists are walked to the end. The
    /// kernel's own answer to the request is not a product result.
    fn request(&self) {
        let _ = OpenOptions::new()
            .write(true)
            .open(&self.0)
            .and_then(|mut file| file.write_all(b"1G"));
    }
}
/// Reclaim requests, one per observation, until this mount is idle and the
/// engine's daemon-wide owner rows are `rows`; returns the requests made.
/// When the bound expires the counters as they are go into the failure.
fn returned(
    reclaim: &Reclaim,
    rig: &Rig,
    token: WorkspaceToken,
    helper: WorkspaceToken,
    rows: u64,
    what: &str,
) -> u32 {
    let deadline = Instant::now() + RECLAIM;
    let mut requests = 0;
    loop {
        requests += 1;
        reclaim.request();
        let (now, counts) = (work(rig, token), owners(rig, helper));
        if now.active == 0 && counts.entries() == rows {
            return requests;
        }
        assert!(
            Instant::now() < deadline,
            "bounded observation expired: {what}: reclaim_requests={requests} memory_entries={} wanted={rows} {now:?} {counts:?}",
            counts.entries()
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn path_only(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)
        .unwrap()
}

const DIRECTORIES: usize = 20;
const FILES: usize = 100;
/// `tree`, its directories and their files; the alias is a second name.
const INODES: u64 = (1 + DIRECTORIES + DIRECTORIES * FILES) as u64;
const NAMES: u64 = INODES + 1;
/// Files of the last directory kept referenced through the first reclaim.
const PINNED: usize = 50;
/// What the pins keep: themselves, their directory and `tree`.
const KEPT: u64 = PINNED as u64 + 2;
/// Directories whose names are looked up again before the detach.
const AGAIN: usize = 10;
/// Lookup rows outstanding at the detach: `tree`, those directories and
/// their files.
const OUTSTANDING: u64 = (1 + AGAIN + AGAIN * FILES) as u64;

fn directory(d: usize) -> String {
    format!("tree/d-{d:02}")
}
fn file(d: usize, f: usize) -> String {
    format!("tree/d-{d:02}/f-{f:03}")
}
/// One `lstat` of `tree`, of each of the first `directories` directories and
/// of each of their files, and of the alias: names looked up and the
/// distinct inodes they bound.
fn walk(mount: &Path, directories: usize) -> (u64, BTreeSet<u64>) {
    let mut inodes = BTreeSet::new();
    let mut names = 0;
    let mut look = |relative: &str| {
        names += 1;
        inodes.insert(fs::symlink_metadata(mount.join(relative)).unwrap().ino());
    };
    look("tree");
    for d in 0..directories {
        look(&directory(d));
        for f in 0..FILES {
            look(&file(d, f));
        }
    }
    look("tree/d-00/alias");
    (names, inodes)
}
/// FP-31, mounted halves at about two thousand lookups. Reclaim with fifty
/// files kept referenced returns exactly the other inodes, one FORGET unit
/// each (the hard-linked inode's two references in one unit), in batched
/// frames; releasing the references returns the rest. A detach with 1,011
/// lookups outstanding then disposes them with its checked-closed file,
/// without terminal SQL jobs, until physical Gone and exact baseline counts.
#[test]
fn fp31_batched_forget_is_exact_and_detach_disposes_outstanding_lookup_memory() {
    let reclaim = Reclaim::own();
    let rig = Rig::built("fp31-wide", |source| {
        for d in 0..DIRECTORIES {
            fs::create_dir_all(source.join(directory(d))).unwrap();
            for f in 0..FILES {
                fs::write(source.join(file(d, f)), format!("{d:02}/{f:03}\n")).unwrap();
            }
        }
        fs::hard_link(source.join(file(0, 0)), source.join("tree/d-00/alias")).unwrap();
        stamp_tree(source);
    });
    let client = rig.harness.owner.client();
    let helper = rig.harness.bind(1);
    let empty = owners(&rig, helper);
    let ready = rig.mount(2);
    let guard = Mounted(ready.directory.clone());
    let token = ready.token;
    let route = holds::route(&rig.harness.service, token);
    let mount = root(&ready).to_owned();
    quiet(&rig, token);
    let baseline = owners(&rig, token);
    assert_eq!(work(&rig, token).forget_units, 0);

    // References with kernel lookup custody only, then every name once.
    let pins: Vec<File> = (0..PINNED)
        .map(|f| path_only(&mount.join(file(DIRECTORIES - 1, f))))
        .collect();
    let (names, inodes) = walk(&mount, DIRECTORIES);
    assert_eq!((names, inodes.len() as u64), (NAMES, INODES));
    quiet(&rig, token);
    let looked = owners(&rig, token);
    assert_eq!(
        looked.entries() - baseline.entries(),
        custody(INODES, 1 + DIRECTORIES as u64),
        "one indexed lookup owner per live kernel inode: {baseline:?} {looked:?}"
    );

    // Partial return: everything but what the pins keep.
    let first_requests = returned(
        &reclaim,
        &rig,
        token,
        token,
        baseline.entries() + custody(KEPT, 2),
        "every unreferenced inode returned by FORGET",
    );
    quiet(&rig, token);
    reclaim.request();
    quiet(&rig, token);
    let partial = work(&rig, token);
    let kept = owners(&rig, token);
    println!(
        "FP-31 wide: names={names} inodes={INODES} pinned_files={PINNED} reclaim_requests={first_requests}+1 forget_units={} memory_entries(baseline={} looked={} after_reclaim={}) engine_decrement={}",
        partial.forget_units,
        baseline.entries(),
        looked.entries(),
        kept.entries(),
        looked.entries() - kept.entries()
    );
    assert_eq!(
        partial.forget_units,
        INODES - KEPT,
        "one unit per evicted inode"
    );
    // Every directory but `tree` and the pinned files' own was returned.
    assert_eq!(
        looked.entries() - kept.entries(),
        custody(partial.forget_units, DIRECTORIES as u64 - 1),
        "the engine's decrement is the units received"
    );
    assert_eq!(kept.entries() - baseline.entries(), custody(KEPT, 2));
    assert_eq!((partial.retained, partial.terminal), (0, 0), "{partial:?}");

    // The rest, once nothing references it.
    drop(pins);
    let second_requests = returned(
        &reclaim,
        &rig,
        token,
        token,
        baseline.entries(),
        "every lookup owner returned by FORGET",
    );
    quiet(&rig, token);
    let all = work(&rig, token);
    assert_eq!(all.forget_units, INODES, "one unit per inode in all");
    assert_eq!((all.retained, all.terminal), (0, 0));
    assert_eq!(rig.harness.phase(token), MountPhase::Ready);

    // Lookups still outstanding at the detach.
    let (again_names, again_inodes) = walk(&mount, AGAIN);
    assert_eq!(again_inodes.len() as u64, OUTSTANDING);
    assert!(again_inodes.is_subset(&inodes), "the same inodes again");
    quiet(&rig, token);
    let outstanding = owners(&rig, token);
    assert_eq!(
        outstanding.entries() - baseline.entries(),
        custody(OUTSTANDING, 1 + AGAIN as u64),
        "reacquired after FORGET"
    );
    let start = client.diagnostics().unwrap();
    let receipt = rig.unmount(&ready);
    let mut states = Vec::new();
    before("cleanup state Gone", LONG, || {
        let state = cleanup(&client, route, [2; 32]);
        if states.last() != Some(&state) {
            states.push(state);
        }
        state == CleanupState::Gone
    });
    let mut last = owners(&rig, helper);
    before(
        "sibling Workspace counts remain at their baseline",
        LONG,
        || {
            last = owners(&rig, helper);
            last == empty
        },
    );
    let end = client.diagnostics().unwrap();

    // The receipt: FORGET frames, batched frames and units of the connection.
    let counts = opcodes(&receipt);
    let (frames, batched) = (
        counts[Opcode::Forget as usize],
        counts[Opcode::BatchForget as usize],
    );
    let units = counter(&receipt, "forget_units");
    println!(
        "FP-31 wide: second_reclaim_requests={second_requests} forget_units_total={units} forget_frames={frames} frames_with_several_units={batched} lookup_frames={} outstanding_at_detach(names={again_names} rows={OUTSTANDING})",
        counts[Opcode::Lookup as usize]
    );
    assert_eq!(units, INODES, "no unit arrived after the detach: {receipt}");
    assert!(
        batched >= 1 && frames < units,
        "batched delivery: {frames} frames, {batched} with several units, {units} units"
    );

    // Drained references and cookie allocations go with checked file disposal.
    // No per-row retirement job or SQL work is manufactured for them.
    assert_eq!(end.maintenance_jobs, start.maintenance_jobs);
    assert_eq!(end.maintenance_rows, start.maintenance_rows);
    assert_eq!(end.sql_maintenance, start.sql_maintenance);
    assert_eq!(last.memory, empty.memory);
    println!("FP-31 disposal: outstanding_entries={OUTSTANDING} row_cleanup_jobs=absent cleanup_states={states:?} counts_equal_baseline=true");
    assert_eq!(end.closed_namespaces, start.closed_namespaces + 1);
    assert!(client.maintenance_failure().unwrap().is_none());
    assert_eq!(last, empty);
    drop(guard);
    let closed = rig.harness.try_unmount(helper).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(helper));
    drop(closed);
    rig.finish();
}

use layerfs_workspace::Route;
