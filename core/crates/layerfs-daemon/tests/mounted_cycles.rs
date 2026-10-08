//! Real kernel mounts: sustained ownership and cleanup (R6 proof track P2).
//!
//! - R6-7, the plateau half of FP-26: twelve mount, write, Commit, unmount
//!   cycles beside one anchor Workspace that stays mounted. The idle variant
//!   waits for owner maintenance after each unmount by reading the owner's
//!   maintained counters only; the live variant keeps an externally launched
//!   writer running on a sibling Workspace through every cycle and pauses it
//!   only to read the counts. The refusal half of FP-26 (`mount:debt`, a
//!   stopped maintenance turn refusing Mount) is not implemented and is not
//!   staged here.
//! - R6-8: a process holding a descriptor, a working directory and a shared
//!   mapping in the mount blocks writing to a full pipe. Filesystem side
//!   only; stream delivery belongs to the runtime rows.
//! - FP-24: a Branch, a fork of it and a Workspace on a committed descendant,
//!   mounted at once by one daemon.
//!
//! Every command is launched here by plain exec under the command identity;
//! nothing registers it with the daemon. Counts only: nothing is timed.
#![cfg(target_os = "linux")]
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
use layerfs_bridge::control::{
    Activity, ControlCode, NativePhase, ReadyMount, Reply, Request, WorkspaceStatus, WorkspaceToken,
};
use layerfs_content::ObjectId;
use layerfs_daemon::{control::Failure, Command, OwnerWork, Response};
use layerfs_history::{
    BranchId, BranchSnapshot, ForkRequest, ForkSource, HistoryName, WorkspaceId,
};
use layerfs_overlay::{Resources, StoredCounts};
use model::{assert_same, Flat, FILE};
use mounted::{mount_entry, COMMAND};
use nix::libc;
use rig::{bash_in, native, passed, pattern, root, stamp_tree, Rig};
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{MetadataExt, PermissionsExt},
            process::{CommandExt, ExitStatusExt},
        },
    },
    path::Path,
    process::{Child, ChildStdout, Command as Process, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(8);

/// Never leaves a kernel mount behind: if the test unwinds, or the product
/// did not unmount, one lazy umount(8) launched and reaped by the test
/// detaches what is still there.
struct Mounted(String);
impl Drop for Mounted {
    fn drop(&mut self) {
        if mount_entry(&self.0).is_some() {
            let done = Process::new("umount").arg("-l").arg(&self.0).output();
            println!(
                "P2_GUARD lazy umount of {} by the test: {:?}",
                self.0,
                done.map(|output| output.status)
            );
        }
    }
}

/// A small deterministic base, so that a cycle is cheap.
fn small_tree(source: &Path) {
    fs::create_dir_all(source.join("work")).unwrap();
    for (path, bytes) in [
        ("README.md", b"# base\n".to_vec()),
        ("work/keep.txt", b"keep\n".to_vec()),
        ("data.bin", pattern(3, 20_000)),
    ] {
        let path = source.join(path);
        fs::write(&path, bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    }
    stamp_tree(source);
}
fn at<'a>(flat: &'a Flat, path: &str) -> &'a [u8] {
    let seen = flat
        .get(path.as_bytes())
        .unwrap_or_else(|| panic!("missing {path}"));
    assert_eq!(seen.kind, FILE, "{path}");
    &seen.payload
}
fn absent(flat: &Flat, paths: &[&str], what: &str) {
    for path in paths {
        assert!(!flat.contains_key(path.as_bytes()), "{what}: {path}");
    }
}
/// A bounded observation loop that reports instead of unwinding.
fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(2));
    }
}
/// Two consecutive observations of a connection with nothing received or
/// admitted and no request completed in between.
fn quiet(rig: &Rig, token: WorkspaceToken) -> bool {
    let mut last = None;
    eventually(|| {
        let work = rig.harness.status(token).native.unwrap().work.unwrap();
        let now = (work.received, work.admitted, work.completed);
        let settled = work.received == 0 && work.admitted == 0 && last == Some(now);
        last = Some(now);
        settled
    })
}
/// One engine observation through a live route: the daemon aggregate, or
/// that route's own namespace.
fn resources(rig: &Rig, through: WorkspaceToken, global: bool) -> Resources {
    let operation = rig.harness.service.operation(through).unwrap();
    let done = operation
        .overlay()
        .try_submit(
            Some(operation.workspace().route()),
            Command::Resources { global },
        )
        .unwrap()
        .wait()
        .unwrap();
    match done.result() {
        Ok(Response::Resources(resources)) => **resources,
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------- R6-7

const CYCLES: usize = 12;
/// Consecutive unchanged observations, two milliseconds apart, that end a
/// maintenance wait.
const STABLE: usize = 20;

/// The owner has acknowledged `closed` terminal reclamations and made no
/// maintenance step over `STABLE` consecutive observations. Reads the owner's
/// maintained counters only: no job is submitted, so nothing here drives the
/// maintenance it waits for. `Err` describes the last observation.
fn settled(rig: &Rig, closed: u64) -> Result<OwnerWork, String> {
    let client = rig.harness.owner.client();
    let deadline = Instant::now() + WAIT;
    let (mut last, mut stable) = (None, 0);
    loop {
        let work = client.diagnostics().unwrap();
        if work.closed_namespaces == closed && last == Some(work.maintenance_jobs) {
            stable += 1;
        } else {
            stable = 0;
        }
        last = Some(work.maintenance_jobs);
        if stable == STABLE {
            return Ok(work);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "maintenance did not settle at closed_namespaces={closed}: closed_namespaces={} maintenance_jobs={} maintenance_rows={}",
                work.closed_namespaces, work.maintenance_jobs, work.maintenance_rows
            ));
        }
        thread::sleep(Duration::from_millis(2));
    }
}
/// The global counts once idle: the owner settled, one `Resources` job, and
/// the owner settled again with no maintenance step in between, so the job
/// read a state in which no automatic work was left.
fn idle(rig: &Rig, through: WorkspaceToken, closed: u64) -> Result<(OwnerWork, Resources), String> {
    for _ in 0..4 {
        let before = settled(rig, closed)?;
        let observed = resources(rig, through, true);
        let after = settled(rig, closed)?;
        if after.maintenance_jobs == before.maintenance_jobs {
            return Ok((after, observed));
        }
    }
    Err("maintenance kept making steps across four idle observations".into())
}

/// Writes without end and without growing: one fixed-size name is rewritten
/// in place, so the writer's own footprint is the same at every pause. A line
/// on standard input pauses it (acknowledged with its iteration count), the
/// next line resumes it, end of input ends it with status 0. Builtins only.
const WRITER: &str = r#"
set -u
umask 022
n=0
printf 'live %08d\n' "$n" > live.bin || exit 8
echo "started"
while :; do
  if read -r -t 0; then
    read -r word || exit 0
    echo "paused $n"
    read -r word || exit 0
  fi
  printf 'live %08d\n' "$n" 1<>live.bin || exit 9
  n=$((n+1))
done
"#;
/// The sibling's external writer. Killed and reaped on drop.
struct Writer {
    child: Option<Child>,
    lines: mpsc::Receiver<String>,
    reader: Option<thread::JoinHandle<()>>,
}
impl Writer {
    fn spawn(directory: &Path) -> Self {
        let mut child = Process::new("bash")
            .args(["-c", WRITER])
            .uid(COMMAND)
            .gid(COMMAND)
            .env("LC_ALL", "C")
            .current_dir(directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let (sender, lines) = mpsc::channel();
        // Ends at end of file, which the child's exit or the guard's kill
        // always produces: the loop spawns no process that could keep the pipe.
        let reader = thread::spawn(move || {
            let mut line = String::new();
            loop {
                line.clear();
                match output.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if sender.send(line.trim_end().to_owned()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let writer = Self {
            child: Some(child),
            lines,
            reader: Some(reader),
        };
        assert_eq!(writer.line(), "started", "the sibling writer");
        writer
    }
    fn line(&self) -> String {
        self.lines
            .recv_timeout(WAIT)
            .expect("the sibling writer did not answer")
    }
    fn say(&mut self, word: &str) {
        let input = self.child.as_mut().unwrap().stdin.as_mut().unwrap();
        input.write_all(word.as_bytes()).unwrap();
        input.flush().unwrap();
    }
    /// Its iteration count; nothing more is written until `resume`.
    fn pause(&mut self) -> u64 {
        self.say("pause\n");
        let answer = self.line();
        answer
            .strip_prefix("paused ")
            .and_then(|count| count.parse().ok())
            .unwrap_or_else(|| panic!("the sibling writer answered {answer:?}"))
    }
    fn resume(&mut self) {
        self.say("resume\n");
    }
    /// Closes its input and waits, bounded, for it to exit by itself.
    fn stop(mut self) -> Option<ExitStatus> {
        let mut child = self.child.take().unwrap();
        drop(child.stdin.take());
        let mut status = None;
        let exited = eventually(|| {
            status = child.try_wait().unwrap();
            status.is_some()
        });
        if !exited {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        status
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
/// The sibling Workspace of the live variant. The writer is the first field:
/// it is killed before the mount guard looks at the mount.
struct Sibling {
    writer: Option<Writer>,
    ready: ReadyMount,
    _guard: Mounted,
}

/// One cycle's observation once idle.
struct Row {
    cycle: usize,
    /// Monotone by design: recorded, not asserted.
    namespace: i64,
    commit: String,
    admitted: u64,
    maintenance_jobs: u64,
    maintenance_rows: u64,
    free_pages: u64,
    /// `closed_namespaces` when Unmount returned, before any wait.
    closed_at_unmount: u64,
    closed: u64,
    pages: u64,
    counts: StoredCounts,
    anchor: StoredCounts,
    sibling: Option<StoredCounts>,
    writer: Option<u64>,
}
fn cycle_script(number: usize) -> String {
    format!(
        "set -euo pipefail; umask 022; mkdir -p made; printf 'cycle {number}\\n' > made/cycle.txt; head -c 20000 /dev/zero | tr '\\0' 'c' > made/data.bin; printf 'cycle {number}\\n' >> README.md"
    )
}
fn cycles(label: &str, live: bool) {
    let rig = Rig::built(label, small_tree);
    let client = rig.harness.owner.client();
    let anchor = rig.mount(1);
    let anchor_guard = Mounted(anchor.directory.clone());
    let mut sibling = live.then(|| {
        let ready = rig.mount(2);
        let guard = Mounted(ready.directory.clone());
        let writer = Writer::spawn(root(&ready));
        Sibling {
            writer: Some(writer),
            ready,
            _guard: guard,
        }
    });
    let mut rows: Vec<Row> = Vec::new();
    let mut deviations: Vec<String> = Vec::new();
    let mut iterations = 0;
    let mut head = None;
    for number in 1..=CYCLES {
        let ready = rig.mount(10 + number as u8);
        let guard = Mounted(ready.directory.clone());
        passed(
            &bash_in(COMMAND, root(&ready), &cycle_script(number)),
            "the cycle's writes",
        );
        let record = rig.committed(ready.token, &format!("R6-7 {label} cycle {number}"));
        rig.unmount(&ready);
        drop(guard);
        let closed_at_unmount = client.diagnostics().unwrap().closed_namespaces;
        // The live writer ran through the whole cycle. It is paused only so
        // that the counts are read with no request of its own in flight.
        let mut writer = None;
        if let Some(sibling) = sibling.as_mut() {
            let now = sibling.writer.as_mut().unwrap().pause();
            if now <= iterations {
                deviations.push(format!(
                    "cycle {number}: the sibling writer made no write during the cycle ({iterations} -> {now})"
                ));
            }
            iterations = now;
            writer = Some(now);
            if !quiet(&rig, sibling.ready.token) {
                deviations.push(format!(
                    "cycle {number}: the paused sibling's connection did not become quiet"
                ));
            }
        }
        let observed = idle(&rig, anchor.token, number as u64);
        let (work, global) = match observed {
            Ok(observed) => observed,
            Err(why) => {
                let work = client.diagnostics().unwrap();
                deviations.push(format!("cycle {number}: {why}"));
                (work, resources(&rig, anchor.token, true))
            }
        };
        let row = Row {
            cycle: number,
            namespace: ready.token.namespace,
            commit: format!("{:?}", record.id),
            admitted: work.admitted,
            maintenance_jobs: work.maintenance_jobs,
            maintenance_rows: work.maintenance_rows,
            free_pages: global.free_pages,
            closed_at_unmount,
            closed: work.closed_namespaces,
            pages: global.database_pages,
            counts: global.counts,
            anchor: resources(&rig, anchor.token, false).counts,
            sibling: sibling
                .as_ref()
                .map(|sibling| resources(&rig, sibling.ready.token, false).counts),
            writer,
        };
        println!(
            "R6_7 {label} cycle={:02} namespace={} closed_at_unmount={} closed={} maintenance_jobs={} maintenance_rows={} admitted={} pages={} free_pages={} writer_iterations={:?} commit={} counts={:?}",
            row.cycle,
            row.namespace,
            row.closed_at_unmount,
            row.closed,
            row.maintenance_jobs,
            row.maintenance_rows,
            row.admitted,
            row.pages,
            row.free_pages,
            row.writer,
            row.commit,
            row.counts
        );
        println!(
            "R6_7 {label} cycle={:02} anchor_only={:?} sibling_only={:?}",
            row.cycle, row.anchor, row.sibling
        );
        rows.push(row);
        head = Some(record);
        if let Some(sibling) = sibling.as_mut() {
            sibling.writer.as_mut().unwrap().resume();
        }
    }

    // The writer ends by itself; the sibling is unmounted like any Workspace.
    let mut unmounted = CYCLES as u64;
    if let Some(mut sibling) = sibling.take() {
        let writer = sibling.writer.take().unwrap();
        let status = writer.stop();
        println!(
            "R6_7 {label} sibling writer exit={status:?} iterations_at_last_pause={iterations}"
        );
        if !status.is_some_and(|status| status.success()) {
            deviations.push(format!("the sibling writer did not exit 0: {status:?}"));
        }
        rig.unmount(&sibling.ready);
        unmounted += 1;
    }
    // What the twelve Commits published, from a fresh bind that is closed
    // again: one more unmounted Workspace.
    let head = head.unwrap();
    let published = rig.published(head.root);
    unmounted += 1;
    assert_eq!(
        at(&published, "made/cycle.txt"),
        format!("cycle {CYCLES}\n").as_bytes()
    );
    let mut readme = b"# base\n".to_vec();
    for number in 1..=CYCLES {
        readme.extend_from_slice(format!("cycle {number}\n").as_bytes());
    }
    assert_eq!(at(&published, "README.md"), readme.as_slice());
    let history = rig.history();
    let last = idle(&rig, anchor.token, unmounted);
    match &last {
        Ok((work, global)) => println!(
            "R6_7 {label} final unmounted={unmounted} closed={} maintenance_jobs={} maintenance_rows={} pages={} free_pages={} history_records={} counts={:?}",
            work.closed_namespaces,
            work.maintenance_jobs,
            work.maintenance_rows,
            global.database_pages,
            global.free_pages,
            history.records.len(),
            global.counts
        ),
        Err(why) => deviations.push(format!("after the last Workspace: {why}")),
    }
    let failure = client.maintenance_failure().unwrap();
    println!("R6_7 {label} maintenance_failure={failure:?}");
    if failure.is_some() {
        deviations.push(format!("maintenance stopped: {failure:?}"));
    }
    rig.unmount(&anchor);
    drop(anchor_guard);
    rig.finish();

    // The plateau, judged after everything is torn down.
    let first = &rows[0];
    for row in &rows {
        if row.closed != row.cycle as u64 {
            deviations.push(format!(
                "cycle {}: closed_namespaces={} with {} Workspaces unmounted",
                row.cycle, row.closed, row.cycle
            ));
        }
        if row.cycle >= 2 && row.counts != first.counts {
            deviations.push(format!(
                "cycle {}: global counts differ from cycle 1: {:?} against {:?}",
                row.cycle, row.counts, first.counts
            ));
        }
        if row.cycle > CYCLES - 6 && row.pages > rows[CYCLES - 7].pages {
            deviations.push(format!(
                "cycle {}: database_pages={} above cycle {}'s {}",
                row.cycle,
                row.pages,
                CYCLES - 6,
                rows[CYCLES - 7].pages
            ));
        }
    }
    println!(
        "R6_7 {label} cycles={CYCLES} unmounted={unmounted} live_writer={live} deviations={}",
        deviations.len()
    );
    assert!(
        deviations.is_empty(),
        "R6-7 {label}: {} observations did not plateau: {deviations:#?}",
        deviations.len()
    );
}

#[test]
fn counts_plateau_over_twelve_cycles_with_maintenance_awaited_after_each_unmount() {
    cycles("r6-7-idle", false);
}
#[test]
fn counts_plateau_over_twelve_cycles_beside_a_running_sibling_writer() {
    cycles("r6-7-live", true);
}

// ---------------------------------------------------------------- R6-8

const PIPED: usize = 1024 * 1024;
/// Writes through a descriptor, a shared mapping (synchronized) and a new
/// file, keeps the descriptor, the mapping and its working directory, says
/// `ready` on standard error and then writes `PIPED` bytes to standard
/// output, which nothing reads. Once that write returns it uses the mapping,
/// the descriptor and the directory again and exits 0.
const HOLDER: &str = r#"
import mmap, os, sys
os.umask(0o022)
with open('earlier.txt', 'w') as f:
    f.write('written before the pipe filled\n')
fd = os.open('held.bin', os.O_RDWR | os.O_CREAT, 0o644)
os.write(fd, b'h' * 8192)
m = mmap.mmap(fd, 8192, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE)
m[100:106] = b'MAPPED'
m.flush()
sys.stderr.write('ready\n')
sys.stderr.flush()
data = b'p' * 1048576
done = 0
while done < len(data):
    done += os.write(1, data[done:])
if m[100:106] != b'MAPPED':
    sys.exit(7)
os.pwrite(fd, b'AFTER', 200)
with open('after.txt', 'w') as f:
    f.write('written after the pipe was read\n')
m.close()
os.close(fd)
sys.exit(0)
"#;
/// The blocked process. Killed and reaped on drop, so none is left.
struct Holder(Option<Child>);
impl Drop for Holder {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
/// What the kernel says of a process and of the pipe it writes to.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Blocked {
    /// `/proc/<pid>/stat` state: `S` is an interruptible sleep.
    state: char,
    /// `SigPnd` and `ShdPnd`: signals pending for the thread and the process.
    pending: (String, String),
    /// Voluntary and involuntary context switches so far.
    switches: (u64, u64),
    /// Bytes the pipe holds unread.
    buffered: usize,
}
fn buffered(pipe: &ChildStdout) -> usize {
    let mut count: libc::c_int = 0;
    let result = unsafe { libc::ioctl(pipe.as_raw_fd(), libc::FIONREAD, &mut count) };
    assert_eq!(result, 0, "{}", io::Error::last_os_error());
    count as usize
}
fn capacity(pipe: &ChildStdout) -> usize {
    let size = unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_GETPIPE_SZ) };
    assert!(size > 0, "{}", io::Error::last_os_error());
    size as usize
}
fn blocked(pid: u32, pipe: &ChildStdout) -> Blocked {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let status = fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .unwrap_or_else(|| panic!("no {name} in /proc/{pid}/status"))
            .trim_start_matches(':')
            .trim()
            .to_owned()
    };
    Blocked {
        state: stat
            .rsplit_once(") ")
            .and_then(|(_, rest)| rest.chars().next())
            .expect("process state"),
        pending: (field("SigPnd"), field("ShdPnd")),
        switches: (
            field("voluntary_ctxt_switches").parse().unwrap(),
            field("nonvoluntary_ctxt_switches").parse().unwrap(),
        ),
        buffered: buffered(pipe),
    }
}

#[test]
fn a_caller_blocked_on_a_full_pipe_neither_blocks_commit_nor_is_touched_by_unmount() {
    let rig = Rig::built("r6-8", small_tree);
    let ready = rig.mount(1);
    let _mounted = Mounted(ready.directory.clone());
    let mount = root(&ready);
    let token = ready.token;

    let mut child = Process::new("python3")
        .args(["-c", HOLDER])
        .uid(COMMAND)
        .gid(COMMAND)
        .env("LC_ALL", "C")
        .current_dir(mount.join("work"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let pipe = child.stdout.take().unwrap();
    let mut errors = BufReader::new(child.stderr.take().unwrap());
    let mut holder = Holder(Some(child));
    // Standard error ends when the process does, also when the guard kills it.
    let (sender, said) = mpsc::channel();
    let listener = thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match errors.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    if sender.send(line.trim_end().to_owned()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    assert_eq!(
        said.recv_timeout(WAIT).expect("the holder did not start"),
        "ready"
    );
    // Blocked: the pipe is full and the writer sleeps with more to write.
    let full = capacity(&pipe);
    assert!(full < PIPED, "the pipe holds {full} bytes");
    assert!(
        eventually(|| {
            let now = blocked(pid, &pipe);
            now.state == 'S' && now.buffered == full
        }),
        "the holder did not block on the full pipe: {:?}",
        blocked(pid, &pipe)
    );
    let before_commit = blocked(pid, &pipe);
    assert_eq!(
        before_commit.pending,
        ("0000000000000000".to_owned(), "0000000000000000".to_owned())
    );
    println!("R6_8 holder pid={pid} pipe_capacity={full} blocked={before_commit:?}");

    // A Commit completes and includes what the process wrote earlier.
    let before = native(mount);
    assert_eq!(
        at(&before, "work/earlier.txt"),
        b"written before the pipe filled\n"
    );
    let mut held_bytes = vec![b'h'; 8192];
    held_bytes[100..106].copy_from_slice(b"MAPPED");
    assert_eq!(at(&before, "work/held.bin"), held_bytes.as_slice());
    absent(&before, &["work/after.txt"], "before the pipe is read");
    let record = rig.committed(token, "R6-8 Commit beside a blocked holder");
    let published = rig.published(record.root);
    assert_same(&before, &published, "R6-8: the published root");
    assert_eq!(
        at(&published, "work/earlier.txt"),
        b"written before the pipe filled\n"
    );
    assert_eq!(at(&published, "work/held.bin"), held_bytes.as_slice());
    let after_commit = blocked(pid, &pipe);
    assert_eq!(
        after_commit, before_commit,
        "the Commit left the holder as it was"
    );

    // Normal Unmount: the kernel's reversible answer, and nothing else.
    let receipt = ready.receipt;
    match rig.harness.try_unmount(token) {
        Err(Failure::Native(failure)) => {
            assert_eq!(
                (failure.code, failure.phase),
                (ControlCode::Busy, "unmount:kernel"),
                "{}",
                failure.detail
            );
        }
        other => panic!("Unmount beside the blocked holder: {other:?}"),
    }
    let after_unmount = blocked(pid, &pipe);
    let running = holder.0.as_mut().unwrap().try_wait().unwrap().is_none();
    println!("R6_8 after Busy: running={running} blocked={after_unmount:?}");
    assert!(running, "the holder is still running");
    assert_eq!(
        after_unmount, after_commit,
        "Unmount left the holder alive, unsignalled and where it was"
    );
    let status = rig.harness.status(token);
    assert_eq!(status.activity, Activity::Idle);
    let native_status = status.native.unwrap();
    assert_eq!(native_status.phase, NativePhase::Ready);
    assert!(!native_status.detached);
    assert_eq!(
        native_status.ready.unwrap().receipt,
        receipt,
        "the same connection"
    );
    assert!(mount_entry(&ready.directory).is_some());
    assert_eq!(
        fs::read(mount.join("work/earlier.txt")).unwrap(),
        b"written before the pipe filled\n",
        "the mount still serves"
    );
    assert_same(&before, &native(mount), "R6-8: the view after Busy");

    // The pipe is read: the process finishes by itself.
    let (counted, total) = mpsc::channel();
    let drain = thread::spawn(move || {
        let mut pipe = pipe;
        let mut bytes = Vec::new();
        let result = pipe.read_to_end(&mut bytes);
        let _ = counted.send((result.map_err(|error| error.kind()), bytes));
    });
    let (result, bytes) = total
        .recv_timeout(WAIT)
        .expect("the holder's output did not end");
    assert_eq!(result, Ok(PIPED));
    assert!(bytes.iter().all(|byte| *byte == b'p'));
    let mut child = holder.0.take().unwrap();
    let mut exit = None;
    let exited = eventually(|| {
        exit = child.try_wait().unwrap();
        exit.is_some()
    });
    if !exited {
        let _ = child.kill();
        let _ = child.wait();
    }
    drain.join().unwrap();
    listener.join().unwrap();
    let rest: Vec<String> = said.try_iter().collect();
    let exit = exit.expect("the holder did not exit after its output was read");
    println!(
        "R6_8 holder exit={exit:?} signal={:?} piped_bytes={} stderr_after_ready={rest:?}",
        exit.signal(),
        bytes.len()
    );
    assert_eq!((exit.code(), exit.signal()), (Some(0), None), "{rest:?}");
    assert!(rest.is_empty(), "{rest:?}");
    // Its mapping, descriptor and working directory worked after the Busy.
    let after = native(mount);
    assert_eq!(
        at(&after, "work/after.txt"),
        b"written after the pipe was read\n"
    );
    held_bytes[200..205].copy_from_slice(b"AFTER");
    assert_eq!(at(&after, "work/held.bin"), held_bytes.as_slice());

    // With no reference left, Unmount succeeds.
    let drained = rig.unmount(&ready);
    println!("R6_8 unmounted after the holder exited: {drained}");
    assert!(matches!(
        rig.harness.service.execute_control(&Request::Status(token)),
        Err(Failure::Rejected(ControlCode::Missing, _))
    ));
    rig.finish();
}

// ---------------------------------------------------------------- FP-24

/// Binds a new Workspace of any Branch.
fn bind_on(rig: &Rig, tag: u8, branch: BranchId) -> (WorkspaceToken, BranchSnapshot) {
    match rig
        .harness
        .service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
            branch,
        })
        .unwrap()
        .reply
    {
        Reply::Bound { token, binding } => (token, binding),
        other => panic!("{other:?}"),
    }
}
/// A Branch's current root, which must be `root`, walked completely through
/// a fresh bind of a new Workspace that is closed again. Reads the Store
/// only: no mount and no Workspace of the test is consulted.
fn published_on(rig: &Rig, tag: u8, branch: BranchId, root: ObjectId) -> Flat {
    let (token, binding) = bind_on(rig, tag, branch);
    assert_eq!(
        binding.effective_root, root,
        "a fresh bind selects the published root"
    );
    let operation = rig.harness.service.operation(token).unwrap();
    let flat = model::canonical(operation.client(), root);
    assert!(operation.ports().failure().unwrap().is_none());
    drop(operation);
    let closed = rig.harness.try_unmount(token).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(token));
    flat
}
/// A Branch as History holds it now, read from the Store.
fn branch_now(rig: &Rig, branch: BranchId) -> BranchSnapshot {
    rig.store
        .history()
        .branch_snapshot(branch)
        .unwrap()
        .expect("the Branch")
}
/// What a refused request must leave alone: the registry's epoch, activity,
/// binding and retained publication, the engine row and the native phase.
fn standing(status: &WorkspaceStatus) -> String {
    let native = status.native.as_ref().unwrap();
    format!(
        "epoch={} saturated={} activity={:?} binding={:?} published={:?} local={:?} local_failure={:?} phase={:?} detached={}",
        status.epoch,
        status.epoch_saturated,
        status.activity,
        status.binding,
        status.published,
        status.local,
        status.local_failure,
        native.phase,
        native.detached
    )
}
fn write_same(ready: &ReadyMount, text: &str) {
    passed(
        &bash_in(
            COMMAND,
            root(ready),
            &format!(
                "set -euo pipefail; umask 022; printf '{text}\\n' > same.txt; printf '{text}\\n' >> README.md"
            ),
        ),
        text,
    );
}

#[test]
fn simultaneous_mounts_over_related_roots_are_isolated_and_a_stale_token_never_redirects() {
    let rig = Rig::built("fp-24", small_tree);
    let main = rig.harness.branch;

    // A: a Workspace of the Branch, bound at its base before any Commit.
    let a = rig.mount(1);
    let guard_a = Mounted(a.directory.clone());
    let base = native(root(&a));
    let bound_a = rig.harness.status(a.token).binding;
    assert_eq!(
        (bound_a.branch.id, bound_a.branch.head_commit),
        (main, None)
    );

    // K publishes two descendants of that root on the Branch. The fork is
    // taken at the first; K is then unmounted, so its token is stale.
    let k = rig.mount(3);
    let guard_k = Mounted(k.directory.clone());
    passed(
        &bash_in(
            COMMAND,
            root(&k),
            "set -euo pipefail; umask 022; printf 'first descendant\\n' > descend-1.txt",
        ),
        "the first descendant",
    );
    let tree_1 = native(root(&k));
    let c1 = rig.committed(k.token, "FP-24 descendant 1");
    let fork = BranchId::from_authority([75; 16]);
    let forked = match rig
        .harness
        .service
        .execute_control(&Request::Fork(ForkRequest {
            stack: bound_a.branch.stack,
            branch: fork,
            name: HistoryName::new("fork").unwrap(),
            source: ForkSource::Commit {
                branch: main,
                commit: c1.id,
            },
        }))
        .unwrap()
        .reply
    {
        Reply::Forked(snapshot) => snapshot,
        other => panic!("{other:?}"),
    };
    assert_eq!((forked.branch.id, forked.effective_root), (fork, c1.root));
    passed(
        &bash_in(
            COMMAND,
            root(&k),
            "set -euo pipefail; umask 022; printf 'second descendant\\n' > descend-2.txt",
        ),
        "the second descendant",
    );
    let tree_2 = native(root(&k));
    let c2 = rig.committed(k.token, "FP-24 descendant 2");
    assert_eq!(c2.parent, Some(c1.id));
    rig.unmount(&k);
    drop(guard_k);

    // F on the fork, D on the Branch at its committed descendant; A is still
    // mounted at the base. Three kernel mounts of one daemon.
    let (token_f, bound_f) = bind_on(&rig, 2, fork);
    let f = rig.harness.attach(token_f);
    let guard_f = Mounted(f.directory.clone());
    let d = rig.mount(4);
    let guard_d = Mounted(d.directory.clone());
    let bound_d = rig.harness.status(d.token).binding;
    assert_eq!(
        rig.harness.status(a.token).binding,
        bound_a,
        "A is not refreshed"
    );
    assert_eq!((bound_f.branch.id, bound_f.effective_root), (fork, c1.root));
    assert_eq!(
        (
            bound_d.branch.id,
            bound_d.branch.head_commit,
            bound_d.effective_root
        ),
        (main, Some(c2.id), c2.root)
    );
    let roots = [bound_a.effective_root, c1.root, c2.root];
    assert!(roots[0] != roots[1] && roots[1] != roots[2] && roots[0] != roots[2]);
    let mounts = [&a, &f, &d];
    let devices: Vec<u64> = mounts
        .iter()
        .map(|ready| fs::metadata(root(ready)).unwrap().dev())
        .collect();
    assert!(devices[0] != devices[1] && devices[1] != devices[2] && devices[0] != devices[2]);
    for ready in mounts {
        assert!(mount_entry(&ready.directory).is_some());
        assert_eq!(rig.harness.status(ready.token).token, ready.token);
    }
    let namespaces = [a.token.namespace, f.token.namespace, d.token.namespace];
    assert!(!namespaces.contains(&k.token.namespace));
    println!(
        "FP_24 mounts: A namespace={} root={:?} | fork namespace={} root={:?} | descendant namespace={} root={:?} | unmounted K namespace={} | devices={devices:?}",
        namespaces[0], roots[0], namespaces[1], roots[1], namespaces[2], roots[2], k.token.namespace
    );
    // Each is routed to its own root.
    assert_same(&base, &native(root(&a)), "A at the base");
    assert_same(
        &tree_1,
        &native(root(&f)),
        "the fork at the first descendant",
    );
    assert_same(&tree_2, &native(root(&d)), "D at the second descendant");

    // The same name, written differently in each.
    write_same(&a, "written in A");
    write_same(&f, "written in the fork");
    write_same(&d, "written on the descendant");
    let (tree_a, tree_f, tree_d) = (native(root(&a)), native(root(&f)), native(root(&d)));
    let isolated = |when: &str| {
        assert_same(&tree_a, &native(root(&a)), &format!("{when}: mount A"));
        assert_same(
            &tree_f,
            &native(root(&f)),
            &format!("{when}: the fork's mount"),
        );
        assert_same(&tree_d, &native(root(&d)), &format!("{when}: mount D"));
    };
    let own = |tree: &Flat, text: &str, what: &str| {
        assert_eq!(
            at(tree, "same.txt"),
            format!("{text}\n").as_bytes(),
            "{what}"
        );
        assert_eq!(
            at(tree, "README.md"),
            format!("# base\n{text}\n").as_bytes(),
            "{what}"
        );
    };
    own(&tree_a, "written in A", "mount A");
    own(&tree_f, "written in the fork", "the fork's mount");
    own(&tree_d, "written on the descendant", "mount D");
    absent(&tree_a, &["descend-1.txt", "descend-2.txt"], "mount A");
    assert_eq!(at(&tree_f, "descend-1.txt"), b"first descendant\n");
    absent(&tree_f, &["descend-2.txt"], "the fork's mount");
    assert_eq!(at(&tree_d, "descend-1.txt"), b"first descendant\n");
    assert_eq!(at(&tree_d, "descend-2.txt"), b"second descendant\n");
    isolated("after the three writes");

    // Each Commit publishes its own bytes where its Workspace is bound, and
    // a fresh bind of that Branch reads exactly them.
    let record_f = rig.committed(f.token, "FP-24 fork");
    let stored_f = published_on(&rig, 100, fork, record_f.root);
    assert_same(&tree_f, &stored_f, "the fork's published root");
    own(&stored_f, "written in the fork", "the fork's fresh bind");
    assert_eq!(
        branch_now(&rig, main).effective_root,
        c2.root,
        "a Commit on the fork does not move the Branch"
    );
    let record_d = rig.committed(d.token, "FP-24 descendant Workspace");
    assert_eq!(record_d.parent, Some(c2.id));
    let stored_d = rig.published(record_d.root);
    assert_same(&tree_d, &stored_d, "D's published root");
    own(
        &stored_d,
        "written on the descendant",
        "the Branch's fresh bind after D",
    );
    let record_a = rig.committed(a.token, "FP-24 Branch Workspace at the base");
    assert_eq!(record_a.parent, None, "the captured parent");
    let stored_a = rig.published(record_a.root);
    assert_same(&tree_a, &stored_a, "A's published root");
    own(&stored_a, "written in A", "the Branch's fresh bind after A");
    absent(&stored_a, &["descend-1.txt", "descend-2.txt"], "A's root");
    assert_eq!(
        branch_now(&rig, fork).effective_root,
        record_f.root,
        "Commits on the Branch do not move the fork"
    );
    assert_same(
        &tree_f,
        &published_on(&rig, 101, fork, record_f.root),
        "the fork's root after the Branch's Commits",
    );
    isolated("after the three Commits");
    println!(
        "FP_24 commits: fork={:?} parent={:?} | descendant={:?} parent={:?} | A={:?} parent={:?}",
        record_f.id, record_f.parent, record_d.id, record_d.parent, record_a.id, record_a.parent
    );

    // Tokens. A refused request changes nothing of any mounted Workspace.
    let standing_all = |rig: &Rig| -> Vec<String> {
        [&a, &f, &d]
            .iter()
            .map(|ready| standing(&rig.harness.status(ready.token)))
            .collect()
    };
    let before = standing_all(&rig);
    let requests = |token: WorkspaceToken| {
        [
            Request::Status(token),
            Request::Commit(token),
            Request::Attach(token),
            Request::Unmount(token),
        ]
    };
    let refused = |token: WorkspaceToken, code: ControlCode, what: &str| {
        for request in requests(token) {
            match rig.harness.service.execute_control(&request) {
                Err(Failure::Rejected(actual, _)) if actual == code => {}
                other => panic!("{what}: {request:?} answered {other:?}"),
            }
        }
    };
    // The token of an unmounted Workspace.
    refused(
        k.token,
        ControlCode::Missing,
        "the unmounted Workspace's token",
    );
    assert!(matches!(
        rig.harness
            .service
            .execute_control(&Request::Locate(k.token.workspace)),
        Err(Failure::Rejected(ControlCode::Missing, _))
    ));
    // Its identity with a live namespace is not redirected to that namespace.
    for ready in [&a, &f, &d] {
        refused(
            WorkspaceToken {
                workspace: k.token.workspace,
                namespace: ready.token.namespace,
            },
            ControlCode::Missing,
            "an unmounted identity with a live namespace",
        );
    }
    // A wrong namespace: another live Workspace's, the unmounted one's, and
    // one this daemon never issued.
    let mut wrong = 0;
    for ready in [&a, &f, &d] {
        let others = [&a, &f, &d]
            .into_iter()
            .map(|other| other.token.namespace)
            .filter(|namespace| *namespace != ready.token.namespace)
            .chain([k.token.namespace, ready.token.namespace + 1000]);
        for namespace in others {
            refused(
                WorkspaceToken {
                    workspace: ready.token.workspace,
                    namespace,
                },
                ControlCode::Invalid,
                "a wrong namespace",
            );
            wrong += 1;
        }
    }
    let after = standing_all(&rig);
    assert_eq!(
        before, after,
        "no epoch or state moved under a refused token"
    );
    isolated("after the refused tokens");
    for ready in [&a, &f, &d] {
        assert!(mount_entry(&ready.directory).is_some());
    }
    println!(
        "FP_24 tokens: unmounted=Missing x4 requests, unmounted identity with a live namespace=Missing x3 tokens, wrong namespace=Invalid x{wrong} tokens of 4 requests each, standing unchanged: {after:?}"
    );

    rig.unmount(&a);
    rig.unmount(&f);
    rig.unmount(&d);
    drop((guard_a, guard_f, guard_d));
    rig.finish();
}
