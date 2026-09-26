//! The writer-gate property, held open: a page read inside a Commit phase is
//! stopped in the kernel and an ordinary mounted mutation runs while it is held.
//!
//! `commit_progress` shows that an ordinary mounted mutation is admitted
//! *between* two transfer frames. That is a necessary check but a weak one: the
//! two syscalls may fall far apart, so it does not fail if the metadata writer
//! gate comes back across a frozen walk. This file builds the check that does.
//! It reuses the recorded E/F barrier - a seccomp `pread64` user-notification
//! listener installed on the Commit thread - so a page read of the frozen
//! transfer stops until this test releases it. While it is held, the Workspace
//! reports the exact work charge a held writer gate adds, an ordinary mounted
//! mutation runs on the same Workspace, and the read is then released so the
//! Commit finishes and the phase counts are reported.
//!
//! The barrier is checked against a control: a page read taken while the gate
//! *is* held must refuse the same mutation. Without that control a barrier that
//! never intercepted anything would report every phase as clean.
#![cfg(target_os = "linux")]
use layerfs_bridge::contract::*;
use layerfs_workspace::*;
use std::{
    fs, io,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

const BRANCH: [u8; 17] = [0x11; 17];
const STACK: [u8; 17] = [0x31; 17];
const LAYER: [u8; 33] = [7; 33];
const BASE: Root = [8; 32];
const SCOPE: Root = [9; 32];
const PROFILE: Root = [10; 32];
const ROOT_SERIAL: u64 = 7;
const METADATA: Root = [13; 32];
/// One small acquisition per write, so the frozen sequence spans several
/// extents and one write phase walks more than one leaf. The shape is small on
/// purpose: the barrier turns every metadata page read on the Commit thread
/// into a kernel round trip, so one extra extent costs more here than it does
/// on the mounted route. The property under test - which walk owns a page read,
/// and what the writer gate is doing across it - does not scale with the count.
const PIECES: usize = 6;
const PIECE_BYTES: usize = 4096;
const PAYLOAD_BYTES: u64 = (PIECES * PIECE_BYTES) as u64;
/// The mutation this test runs while a Commit thread is stopped. `write_file`
/// waits for the gate up to its own deadline, so a one-second budget turns a
/// held gate into a deterministic `Deadline` refusal instead of a hang.
const PROBE_MS: u64 = 1_000;
/// The Commit keeps a wide budget so the probe, not the Commit, is what expires.
const COMMIT_MS: u64 = 20_000;
const FRAME_MS: u64 = 5_000;

fn deadline_after(millis: u64) -> Instant {
    Instant::now() + Duration::from_millis(millis)
}

fn temporary(tag: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "commit-overlap-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut builder = fs::DirBuilder::new();
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
    builder.create(&path).unwrap();
    path
}

/// The bytes one zero-copy acquisition reads from.
struct Bytes<'a>(&'a [u8]);
impl io::Read for Bytes<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let take = out.len().min(self.0.len());
        out[..take].copy_from_slice(&self.0[..take]);
        self.0 = &self.0[take..];
        Ok(take)
    }
}
impl Source for Bytes<'_> {
    fn read(
        &mut self,
        out: &mut [u8],
        _deadline: Instant,
        _cancel: &AtomicBool,
    ) -> io::Result<usize> {
        io::Read::read(self, out)
    }
}

// ---------------------------------------------------------------------------
// The syscall barrier: a seccomp `pread64` user-notification listener.
// ---------------------------------------------------------------------------

/// Installs the listener on the *calling* thread and returns its descriptor, so
/// a caller can stop each of that thread's page reads until it answers.
fn page_read_listener() -> std::os::fd::OwnedFd {
    use nix::libc;
    use std::os::fd::FromRawFd;
    let arch = match std::env::consts::ARCH {
        "aarch64" => 0xc00000b7,
        "x86_64" => 0xc000003e,
        other => panic!("unsupported syscall architecture {other}"),
    };
    let stmt = |code, k| libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    };
    let code = [
        stmt(0x20, 4),
        libc::sock_filter {
            code: 0x15,
            jt: 0,
            jf: 3,
            k: arch,
        },
        stmt(0x20, 0),
        libc::sock_filter {
            code: 0x15,
            jt: 0,
            jf: 1,
            k: libc::SYS_pread64 as u32,
        },
        stmt(0x06, libc::SECCOMP_RET_USER_NOTIF),
        stmt(0x06, libc::SECCOMP_RET_ALLOW),
    ];
    let program = libc::sock_fprog {
        len: code.len() as u16,
        filter: code.as_ptr() as *mut _,
    };
    unsafe {
        assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
        let fd = libc::syscall(
            libc::SYS_seccomp,
            libc::SECCOMP_SET_MODE_FILTER,
            libc::SECCOMP_FILTER_FLAG_NEW_LISTENER,
            &program,
        );
        assert!(fd >= 0, "{}", std::io::Error::last_os_error());
        std::os::fd::OwnedFd::from_raw_fd(fd as i32)
    }
}

/// One page read the Commit thread has stopped in, with the descriptor of the
/// listener that reported it so exactly this read can be released.
struct Held {
    listener: std::os::fd::OwnedFd,
    id: u64,
    path: String,
    offset: i64,
    length: i64,
}

impl Held {
    /// The page's own name, as the recorded E/F fixtures report it.
    fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }
}

/// The barrier over one Commit thread's page reads.
struct Barrier {
    listener: std::os::fd::OwnedFd,
    test_tid: u32,
    answered: AtomicU64,
}

impl Barrier {
    fn new(listener: std::os::fd::OwnedFd, test_tid: u32) -> Self {
        Self {
            listener,
            test_tid,
            answered: AtomicU64::new(0),
        }
    }
    /// The next page read the Commit thread starts, or `None` when it reaches
    /// the wait without starting one.
    fn hold_next(&self, wait: Duration) -> Option<Held> {
        use nix::libc;
        use std::os::fd::AsRawFd;
        let mut poll = libc::pollfd {
            fd: self.listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, wait.as_millis() as i32) };
        assert!(ready >= 0, "{}", std::io::Error::last_os_error());
        if ready == 0 || poll.revents & libc::POLLIN == 0 {
            return None;
        }
        let mut event: nix::libc::seccomp_notif = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::ioctl(
                    self.listener.as_raw_fd(),
                    libc::SECCOMP_IOCTL_NOTIF_RECV,
                    &mut event,
                )
            },
            0
        );
        assert_eq!(
            event.pid, self.test_tid,
            "the barrier stopped another thread"
        );
        assert_eq!(event.data.nr as i64, libc::SYS_pread64);
        let fd = event.data.args[0] as i32;
        // The first argument is the page's own descriptor. Read its name while
        // the syscall is stopped: this is the page the walk is inside.
        let path = std::fs::read_link(format!("/proc/self/task/{}/fd/{fd}", self.test_tid))
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|error| format!("<unreadable: {error}>"));
        Some(Held {
            listener: self.listener.try_clone().unwrap(),
            id: event.id,
            path,
            offset: event.data.args[1] as i64,
            length: event.data.args[2] as i64,
        })
    }
    /// Finds the next page read this Commit starts, answering every read for
    /// which `wanted` is false so the Commit keeps making progress.
    fn hold_where(&self, wait: Duration, mut wanted: impl FnMut(&Held) -> bool) -> Option<Held> {
        let until = Instant::now() + wait;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            let held = self.hold_next(left)?;
            if wanted(&held) {
                return Some(held);
            }
            self.answer(&held);
        }
    }
    /// Answers a stopped read with `CONTINUE`, so the kernel performs it, and
    /// counts it. Exactly one answer reaches the kernel per stopped read.
    fn answer(&self, held: &Held) {
        use nix::libc;
        use std::os::fd::AsRawFd;
        let mut response = libc::seccomp_notif_resp {
            id: held.id,
            val: 0,
            error: 0,
            flags: libc::SECCOMP_USER_NOTIF_FLAG_CONTINUE as u32,
        };
        assert_eq!(
            unsafe {
                libc::ioctl(
                    held.listener.as_raw_fd(),
                    libc::SECCOMP_IOCTL_NOTIF_SEND,
                    &mut response,
                )
            },
            0
        );
        self.answered.fetch_add(1, Ordering::AcqRel);
    }
}

// ---------------------------------------------------------------------------
// The fixture: one attached Workspace and a delivery that models the route.
// ---------------------------------------------------------------------------

/// What the delivery observed, and the fence it uses to reach the test.
#[derive(Default)]
struct Observed {
    frames: AtomicU64,
    bytes: AtomicU64,
    /// Set when the test wants the delivery to park its first transfer pull, so
    /// the barrier can be armed before the frame is served.
    park: AtomicBool,
    /// The single-use fence channel, consumed by the closed-loop wait.
    fence: Mutex<Option<mpsc::Sender<()>>>,
    /// Every mutation the test ran while a read was held.
    probes: Mutex<Vec<(Result<(), String>, u128)>>,
}

struct Fixture {
    host: WorkspaceHost,
    workspace: Workspace,
    handle: HandleId,
    payload: OwnedPayload,
    observed: Arc<Observed>,
    path: PathBuf,
}

impl Fixture {
    fn new(tag: &str, outcome: Outcome) -> Self {
        let path = temporary(tag);
        let metadata = fs::metadata(&path).unwrap();
        use std::os::unix::fs::MetadataExt;
        let observed = Arc::new(Observed::default());
        let host = WorkspaceHost::new(
            WorkspaceConfig {
                root: path.clone(),
                max_count: 1,
                memory_budget_bytes: 128 * 1024 * 1024,
                disk_budget_bytes: Some(512 * 1024 * 1024),
            },
            delivery(observed.clone(), outcome),
        )
        .unwrap();
        let workspace = host
            .attach(
                AttachOptions {
                    id: "overlap".into(),
                    incarnation: [23; 32],
                    store: 1,
                    base: Base::Branch(BRANCH),
                    access: WorkspaceAccess::LocalEdit,
                    owner_uid: metadata.uid(),
                    owner_gid: metadata.gid(),
                },
                deadline_after(20_000),
            )
            .unwrap();
        let file = workspace
            .mknod(ROOT_SERIAL, b"data.bin", 0o644, 0, deadline_after(20_000))
            .unwrap();
        let handle = workspace
            .open_file(
                file.serial,
                FileOpenOptions {
                    access: FileAccess::ReadWrite,
                    append: false,
                    truncate: false,
                },
                ReferenceScope::Local,
                deadline_after(20_000),
            )
            .unwrap();
        let bytes = vec![0x5a_u8; PIECE_BYTES];
        let mut payload = None;
        for index in 0..PIECES {
            let piece = workspace
                .own_payload(
                    bytes.len() as u64,
                    &mut Bytes(&bytes),
                    deadline_after(20_000),
                )
                .unwrap_or_else(|error| panic!("acquire {index}: {error:?}"));
            workspace
                .write_file(
                    handle,
                    (index * PIECE_BYTES) as u64,
                    &piece,
                    deadline_after(20_000),
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "write {index}: {error:?} status={:?}",
                        workspace.backing_status().unwrap()
                    )
                });
            payload = Some(piece);
        }
        Self {
            host,
            workspace,
            handle,
            payload: payload.unwrap(),
            observed,
            path,
        }
    }
    fn status(&self) -> WorkspaceStatus {
        self.workspace.status().unwrap()
    }
    /// The exact charge a held writer gate adds, so a reader can tell a held
    /// gate from a free one without taking the gate.
    fn gate_charge(&self) -> usize {
        self.workspace.metadata_status().unwrap().working_bytes
    }
    fn metadata_reads(&self) -> u64 {
        self.workspace.backing_status().unwrap().metadata_reads
    }
    /// The Commit's own phase while its thread is stopped. `WorkspaceStatus`
    /// takes the state lock, never the writer gate.
    fn commit_phase(&self) -> Option<CommitPhase> {
        self.status().submission?.commit.map(|commit| commit.phase)
    }
    /// One ordinary mounted mutation: a write through the same handle the frozen
    /// Commit is reading. It waits for the writer gate only up to its own
    /// one-second deadline, so a held gate is a refusal rather than a hang.
    fn probe(&self) -> (Result<(), String>, u128) {
        let start = Instant::now();
        let result = self
            .workspace
            .write_file(
                self.handle,
                PAYLOAD_BYTES,
                &self.payload,
                deadline_after(PROBE_MS),
            )
            .map(|_| ())
            .map_err(|error| format!("{error:?}"));
        let elapsed = start.elapsed().as_micros();
        self.observed
            .probes
            .lock()
            .unwrap()
            .push((result.clone(), elapsed));
        (result, elapsed)
    }
    /// Parks the delivery's first transfer pull and returns the channel it
    /// reports that pull on, so the test can arm the barrier before the frame is
    /// served.
    fn park_first_pull(&self) -> mpsc::Receiver<()> {
        let (send, receive) = mpsc::channel();
        *self.observed.fence.lock().unwrap() = Some(send);
        self.observed.park.store(true, Ordering::Release);
        receive
    }
    fn frames(&self) -> u64 {
        self.observed.frames.load(Ordering::Acquire)
    }
    fn bytes(&self) -> u64 {
        self.observed.bytes.load(Ordering::Acquire)
    }
    fn close(self) {
        drop(self.workspace);
        drop(self.host);
        fs::remove_dir_all(&self.path).unwrap();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The transfer replays every frame and then refuses the save, so the Commit
    /// stops at its file-save stage and never needs a canonical Store.
    Unsupported,
}

fn delivery(observed: Arc<Observed>, outcome: Outcome) -> OperationDelivery {
    Arc::new(move |request, input, _output, _deadline| {
        match &request.operation {
            Operation::HistoryQuery(HistoryQuery::GetBranch { branch }) => Ok(Response::History(
                Box::new(HistoryResult::BranchSnapshot(BranchSnapshotWire {
                    branch: BranchWire {
                        branch: *branch,
                        stack: STACK,
                        name: b"main".to_vec(),
                        base_layer: LAYER,
                        head_commit: None,
                    },
                    head_root: None,
                    base_root: BASE,
                    effective_root: BASE,
                    root_serial: Some(ROOT_SERIAL),
                    scope: SCOPE,
                    profile: PROFILE,
                })),
            )),
            // Only the filesystem root this fixture models exists.
            Operation::Inspect {
                query: Inspect::Attributes { path },
                ..
            } if path.is_empty() => Ok(Response::Attributes {
                serial: ROOT_SERIAL,
                kind: 2,
                references: 0,
                content: [7; 32],
                metadata: [1; 32],
                mode: 0o755,
                mtime: -1,
                nanoseconds: 123,
                size: 0,
            }),
            Operation::Inspect {
                query: Inspect::Attributes { .. },
                ..
            } => Err(Code::NotFound.into()),
            Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count }) => {
                Ok(Response::History(Box::new(HistoryResult::Reservation {
                    scope: *scope,
                    start: 1000,
                    count: *count,
                })))
            }
            // The transfer body: one descriptor per extent plus every
            // replacement byte, in frames. The fence parks the chosen pull so
            // the test can arm the barrier before that frame is served.
            Operation::SaveFile { .. } => {
                let cancel = AtomicBool::new(false);
                let mut buffer = [0u8; 4096];
                loop {
                    let count = input
                        .read(&mut buffer, deadline_after(FRAME_MS), &cancel)
                        .map_err(|_| Failure::from(Code::Io))?;
                    if count == 0 {
                        break;
                    }
                    let frame = observed.frames.fetch_add(1, Ordering::AcqRel);
                    observed.bytes.fetch_add(count as u64, Ordering::AcqRel);
                    if frame == 0 && observed.park.swap(false, Ordering::AcqRel) {
                        if let Some(fence) = observed.fence.lock().unwrap().take() {
                            fence.send(()).unwrap();
                        }
                    }
                }
                let _ = outcome;
                Err(Code::Unsupported.into())
            }
            Operation::UpdatePortableMetadata {
                base, kind, mode, ..
            } => Ok(Response::MetadataSaved {
                base: *base,
                kind: *kind,
                mode: *mode,
                mtime_seconds: 1,
                mtime_nanoseconds: 0,
                metadata: METADATA,
                inserted: 0,
                reused: 1,
            }),
            Operation::ConstructPortableMetadata { kind, mode, .. } => {
                Ok(Response::MetadataConstructed {
                    kind: *kind,
                    mode: *mode,
                    mtime_seconds: 1,
                    mtime_nanoseconds: 0,
                    metadata: METADATA,
                    inserted: 1,
                    reused: 0,
                })
            }
            _ => Err(Code::Unsupported.into()),
        }
    })
}

type Committing = std::thread::JoinHandle<Result<CommitReport, WorkspaceError>>;

/// Runs one Commit on its own thread with the barrier installed on that thread,
/// and hands the test the barrier. The listener stops only the thread that
/// installed it, so the test thread keeps reading pages freely.
fn committing(fixture: &Fixture) -> (mpsc::Receiver<Arc<Barrier>>, Committing) {
    let (send, receive) = mpsc::channel();
    let workspace = fixture.workspace.clone();
    let thread = std::thread::spawn(move || {
        let listener = page_read_listener();
        let tid = unsafe { nix::libc::syscall(nix::libc::SYS_gettid) as u32 };
        send.send(Arc::new(Barrier::new(listener, tid))).unwrap();
        workspace.commit(deadline_after(COMMIT_MS))
    });
    (receive, thread)
}

/// Answers every page read the Commit starts until it finishes.
fn release_and_drain(barrier: &Barrier, thread: &Committing) {
    while !thread.is_finished() {
        if let Some(held) = barrier.hold_next(Duration::from_millis(5)) {
            barrier.answer(&held);
        }
    }
}

/// Waits for the delivery's parked transfer pull while answering the page reads
/// the Commit starts in the meantime. A Commit that reaches a page read before
/// its first frame would otherwise wait on this test for a notification it has
/// not collected yet.
fn wait_for_fence(barrier: &Barrier, fence: &mpsc::Receiver<()>) {
    let until = Instant::now() + Duration::from_millis(5_000);
    loop {
        if fence.try_recv().is_ok() {
            return;
        }
        let left = until.saturating_duration_since(Instant::now());
        assert!(
            !left.is_zero(),
            "the delivery never reached its parked pull"
        );
        let wait = left.min(Duration::from_millis(5));
        if let Some(held) = barrier.hold_next(wait) {
            barrier.answer(&held);
        }
    }
}

/// The writer gate's own charge: `WORKING` in `backing::metadata`, and the
/// difference a held writer gate adds to the persistent budget.
const WORKING: u64 = 640 * 1024;

/// What the test observed while it held one Commit page read.
struct Observation {
    /// Page reads this Commit started while the barrier looked for the one to
    /// hold. Every one of them was answered so the Commit kept progressing.
    scans: u64,
    /// Of those, the ones that reported a held writer gate.
    scans_with_gate: u64,
    /// The charge the Workspace reported with the read stopped. A held writer
    /// gate adds exactly `WORKING` to the pre-Commit baseline, so a charge below
    /// `baseline + WORKING` is a read taken with the gate free.
    charge: u64,
    baseline: u64,
    phase: Option<CommitPhase>,
    frames: u64,
    page: String,
    offset: i64,
    length: i64,
}

impl Observation {
    fn gate_held(&self) -> bool {
        self.charge >= self.baseline + WORKING
    }
    fn report(&self, label: &str, probe: &(Result<(), String>, u128)) {
        println!(
            "COMMIT_OVERLAP {label} page={} offset={} length={} phase={:?} \
             baseline_charge={} charge={} gate_held={} scans={} scans_with_gate={} \
             frames={} mutation={:?} mutation_us={}",
            self.page,
            self.offset,
            self.length,
            self.phase,
            self.baseline,
            self.charge,
            self.gate_held(),
            self.scans,
            self.scans_with_gate,
            self.frames,
            probe.0,
            probe.1,
        );
    }
}

/// Holds the next page read whose charge reports the wanted gate state. Every
/// other read the Commit starts on the way is answered, so the Commit keeps
/// making progress; the ones that reported a held gate are counted, which is
/// what lets the frozen-walk arm assert on more than the single held read.
fn hold_with_gate(
    fixture: &Fixture,
    barrier: &Barrier,
    baseline: u64,
    gate_held: bool,
    wait: Duration,
) -> Option<(Held, Observation)> {
    let mut scans = 0u64;
    let mut scans_with_gate = 0u64;
    let held = barrier.hold_where(wait, |_| {
        scans += 1;
        let charge = fixture.gate_charge() as u64;
        if charge >= baseline + WORKING {
            scans_with_gate += 1;
        }
        (charge >= baseline + WORKING) == gate_held
    })?;
    let observation = Observation {
        scans,
        scans_with_gate,
        charge: fixture.gate_charge() as u64,
        baseline,
        phase: fixture.commit_phase(),
        frames: fixture.frames(),
        page: held.name().to_string(),
        offset: held.offset,
        length: held.length,
    };
    Some((held, observation))
}

/// One frozen transfer walk, held: the page read is stopped in the kernel with
/// the metadata writer gate free across it, and an ordinary mounted mutation
/// runs while it is held.
#[test]
fn a_frozen_walk_page_read_is_held_with_the_writer_gate_free() {
    let f = Fixture::new("frozen", Outcome::Unsupported);
    let baseline = f.gate_charge() as u64;
    let (listener, committer) = committing(&f);
    let barrier = listener.recv_timeout(Duration::from_secs(5)).unwrap();
    let fence = f.park_first_pull();
    // The delivery parks its first transfer pull, so the transfer has really
    // begun and the read this holds belongs to a walk of the frozen sequence.
    wait_for_fence(&barrier, &fence);
    let (held, observed) =
        hold_with_gate(&f, &barrier, baseline, false, Duration::from_millis(900)).unwrap_or_else(
            || {
                panic!(
                    "no frozen-walk page read was held: frames={} charge={} baseline={baseline}",
                    f.frames(),
                    f.gate_charge()
                )
            },
        );
    // An ordinary mounted mutation, taken while the walk's page read is stopped
    // in the kernel.
    let probe = f.probe();
    observed.report("FROZEN", &probe);
    assert!(
        !observed.gate_held(),
        "the held page read of the frozen walk runs with the writer gate free"
    );
    assert_eq!(
        observed.scans_with_gate, 0,
        "{} page reads before the held one reported a held writer gate",
        observed.scans
    );
    assert!(observed.page.starts_with("m-page-"), "{}", observed.page);
    assert_eq!(observed.length, 4096, "one metadata page");
    assert_eq!(
        observed.phase,
        Some(CommitPhase::Preparing),
        "the read is inside the Commit's preparation phase"
    );
    assert!(
        !committer.is_finished(),
        "the Commit thread is still stopped on the held read"
    );
    assert!(
        observed.frames >= 1,
        "the transfer really started before the read was held"
    );
    barrier.answer(&held);
    release_and_drain(&barrier, &committer);
    let result = committer.join().unwrap();
    // The delivery refuses the save once the body is replayed, so the Commit
    // fails at its file-save stage: the held read really was inside it.
    let Err(WorkspaceError::Commit(failure)) = &result else {
        panic!("the delivery stops the Commit after the walk, got {result:?}");
    };
    assert_eq!(failure.phase, CommitPhase::Preparing);
    println!(
        "COMMIT_OVERLAP FROZEN_RESULT scans={} scans_with_gate={} metadata_reads={} frames={} \
         bytes={} replacement_bytes={}",
        observed.scans,
        observed.scans_with_gate,
        f.metadata_reads(),
        f.frames(),
        f.bytes(),
        PAYLOAD_BYTES
    );
    assert!(f.bytes() > 0, "the transfer carried bytes in frames");
    f.close();
}

/// The control: the same barrier and the same mutation at a page read taken
/// while the writer gate really is held. Without this arm the frozen-walk
/// assertion would rest on a barrier that might intercept nothing.
#[test]
fn a_page_read_under_the_writer_gate_reports_the_held_gate() {
    let f = Fixture::new("gate", Outcome::Unsupported);
    let baseline = f.gate_charge() as u64;
    let (listener, committer) = committing(&f);
    let barrier = listener.recv_timeout(Duration::from_secs(5)).unwrap();
    let (held, observed) = hold_with_gate(&f, &barrier, baseline, true, Duration::from_millis(900))
        .unwrap_or_else(|| {
            panic!(
                "no gate-held page read was held: charge={} threshold={}",
                f.gate_charge(),
                baseline + WORKING
            )
        });
    let probe = f.probe();
    observed.report("CONTROL", &probe);
    assert!(
        observed.gate_held(),
        "this read is taken while the writer gate is held: charge={} baseline={}",
        observed.charge,
        baseline
    );
    assert_eq!(
        observed.charge,
        baseline + WORKING,
        "a held writer gate adds exactly its own working charge"
    );
    assert!(
        !committer.is_finished(),
        "the Commit thread is still inside the gate-held read"
    );
    barrier.answer(&held);
    release_and_drain(&barrier, &committer);
    let result = committer.join().unwrap();
    assert!(matches!(result, Err(WorkspaceError::Commit(_))));
    // The gate-held walk is the Commit's own bookkeeping, before any transfer
    // frame: it is the contrast arm, not the frozen transfer.
    assert_eq!(observed.frames, 0, "no transfer frame was served yet");
    f.close();
}
