//! The frozen Commit transfer: frame-wise replay, bounded page reads, and a
//! mounted mutation admitted while it is in flight.
//!
//! A Commit pins its captured root, and the captured sequence is immutable, so
//! the lowering walk and the descriptor/replacement transfer only read it under
//! a bounded read lease and hold no metadata writer gate across the walk. This
//! drives the ordinary public route - attach, one local file write,
//! `Workspace::commit` - against a delivery that pulls the transfer body frame
//! by frame and, between two frames, publishes an ordinary mounted mutation
//! through the same Workspace.
//!
//! It checks three things the phase-2 Commit path must keep: the body is a
//! replay of one descriptor per extent plus every replacement byte, in frames;
//! the walk that produces it reads a bounded number of metadata pages however
//! many frames the transport asks for (the cursor keeps its path across pulls);
//! and an ordinary mounted mutation is accepted while the transfer is in
//! flight. The writer-gate property itself - no Commit phase acquires the gate
//! across the walk or the pull - is held deterministically by
//! `commit_overlap.rs`, which stops a page read of the transfer in the kernel
//! with the recorded E/F page-read barrier and reads the gate state there; this
//! file claims the frame replay and the bounded walk, not the gate.
#![cfg(target_os = "linux")]
use layerfs_bridge::contract::*;
use layerfs_workspace::*;
use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
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
/// One small acquisition per write, so the frozen sequence holds many extents:
/// a transfer frame then crosses several of them, which is the shape that pays
/// for a cursor re-seeked per pull.
const PIECES: usize = 384;
const PIECE_BYTES: usize = 4096;
const PAYLOAD_BYTES: usize = PIECES * PIECE_BYTES;
const PROBE_MS: u64 = 500;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(20)
}
fn private(path: &PathBuf) {
    let mut builder = fs::DirBuilder::new();
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
    builder.create(path).unwrap();
}
fn temporary() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "commit-progress-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    private(&path);
    path
}
/// What the delivery observed while it was pulling the frozen transfer.
#[derive(Default)]
struct Observed {
    workspace: Mutex<Option<Workspace>>,
    handle: Mutex<Option<HandleId>>,
    payload: Mutex<Option<OwnedPayload>>,
    frames: AtomicU64,
    bytes: AtomicU64,
    probes: Mutex<Vec<Result<(), String>>>,
    offset: AtomicU64,
}
impl Observed {
    /// One ordinary mounted mutation, taken between two frames of the transfer.
    /// It publishes through the same Workspace the Commit is reading, so it can
    /// only succeed while that Commit holds no metadata writer gate.
    fn probe(&self) {
        let (Some(workspace), Some(handle), Some(payload)) = (
            self.workspace.lock().unwrap().clone(),
            *self.handle.lock().unwrap(),
            self.payload.lock().unwrap().clone(),
        ) else {
            self.probes
                .lock()
                .unwrap()
                .push(Err("probe was not armed".into()));
            return;
        };
        let offset = self.offset.fetch_add(0, Ordering::AcqRel);
        let result = workspace
            .write_file(
                handle,
                offset,
                &payload,
                Instant::now() + Duration::from_millis(PROBE_MS),
            )
            .map(|_| ())
            .map_err(|error| format!("{error:?}"));
        self.probes.lock().unwrap().push(result);
    }
}

fn delivery(observed: Arc<Observed>) -> OperationDelivery {
    Arc::new(move |request, input, _output, deadline| {
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
            // No name of the attached base exists in the Store the fixture
            // models: every namespace lookup below the root reports NotFound.
            Operation::Inspect {
                query: Inspect::Attributes { path },
                ..
            } if !path.is_empty() => Err(Code::NotFound.into()),
            Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count }) => {
                Ok(Response::History(Box::new(HistoryResult::Reservation {
                    scope: *scope,
                    start: 1000,
                    count: *count,
                })))
            }
            Operation::SaveFile { .. } => {
                let cancel = AtomicBool::new(false);
                let mut buffer = [0u8; 4096];
                loop {
                    let count = input
                        .read(&mut buffer, deadline, &cancel)
                        .map_err(|_| Failure::from(Code::Io))?;
                    if count == 0 {
                        break;
                    }
                    let frame = observed.frames.fetch_add(1, Ordering::AcqRel);
                    observed.bytes.fetch_add(count as u64, Ordering::AcqRel);
                    if frame == 0 {
                        observed.probe();
                    }
                }
                // The probe is the assertion; the transfer itself stops here so
                // the check never needs a canonical Store.
                Err(Code::Unsupported.into())
            }
            _ => Err(Code::Unsupported.into()),
        }
    })
}

#[test]
fn a_frozen_transfer_admits_a_mounted_mutation_between_its_frames() {
    use std::io;
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

    let path = temporary();
    let metadata = fs::metadata(&path).unwrap();
    use std::os::unix::fs::MetadataExt;
    let observed = Arc::new(Observed::default());
    let host = WorkspaceHost::new(
        WorkspaceConfig {
            root: path.clone(),
            max_count: 1,
            memory_budget_bytes: 64 * 1024 * 1024,
            disk_budget_bytes: Some(256 * 1024 * 1024),
        },
        delivery(observed.clone()),
    )
    .unwrap();
    let workspace = host
        .attach(
            AttachOptions {
                id: "progress".into(),
                incarnation: [23; 32],
                store: 1,
                base: Base::Branch(BRANCH),
                access: WorkspaceAccess::LocalEdit,
                owner_uid: metadata.uid(),
                owner_gid: metadata.gid(),
            },
            deadline(),
        )
        .unwrap();
    let file = workspace
        .mknod(ROOT_SERIAL, b"data.bin", 0o644, 0, deadline())
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
            deadline(),
        )
        .unwrap();
    let bytes = vec![0x5a_u8; PIECE_BYTES];
    let mut probe_payload = None;
    for index in 0..PIECES {
        let payload = workspace
            .own_payload(bytes.len() as u64, &mut Bytes(&bytes), deadline())
            .unwrap_or_else(|error| panic!("acquire {index}: {error:?}"));
        workspace
            .write_file(handle, (index * PIECE_BYTES) as u64, &payload, deadline())
            .unwrap_or_else(|error| {
                panic!(
                    "write {index}: {error:?} status={:?}",
                    workspace.backing_status().unwrap()
                )
            });
        probe_payload = Some(payload);
    }
    let payload = probe_payload.unwrap();
    {
        let mut armed = observed.workspace.lock().unwrap();
        *armed = Some(workspace.clone());
        drop(armed);
        *observed.handle.lock().unwrap() = Some(handle);
        *observed.payload.lock().unwrap() = Some(payload);
        observed
            .offset
            .store(PAYLOAD_BYTES as u64, Ordering::Release);
    }
    // The Commit reaches the frozen transfer and stops there: the delivery
    // refuses the save after it has probed.
    let before = workspace.backing_status().unwrap().metadata_reads;
    let result = workspace.commit(deadline());
    let pages = workspace.backing_status().unwrap().metadata_reads - before;
    // The delivery refuses the save once it has probed, so the Commit fails at
    // its file-save phase with that refusal: the probe really ran inside the
    // frozen transfer and not somewhere after it.
    let Err(WorkspaceError::Commit(failure)) = &result else {
        panic!("the delivery stops the Commit after the probe, got {result:?}");
    };
    assert_eq!(failure.phase, CommitPhase::Preparing);
    let WorkspaceError::Stage(stage) = &failure.cause else {
        panic!("the refusal comes from the file-save stage, got {failure:?}");
    };
    assert_eq!(stage.phase, StagePhase::FileSave);
    let probes = observed.probes.lock().unwrap();
    assert_eq!(probes.len(), 1, "exactly one probe ran between two frames");
    assert_eq!(probes[0], Ok(()), "a mounted mutation must be admitted");
    let frames = observed.frames.load(Ordering::Acquire);
    let transferred = observed.bytes.load(Ordering::Acquire);
    println!(
        "COMMIT_PROGRESS frames={frames} bytes={transferred} payload={PAYLOAD_BYTES} metadata_reads={pages}"
    );
    assert!(frames > 1, "the probe must run between two transfer frames");
    // Three ordered passes over the frozen sequence - lowering, descriptors and
    // replacement - each read its few leaf pages once, plus the generation
    // bookkeeping's bounded handful: tens of reads for hundreds of frames. A
    // cursor re-seeked per pull reads a path for every frame instead (about two
    // pages per frame on this shape), which is why the bound sits below that.
    assert!(
        pages <= 100,
        "the frozen walk read {pages} metadata pages for {frames} transfer frames"
    );
    assert!(
        transferred >= PAYLOAD_BYTES as u64 + 24 * PIECES as u64,
        "the transfer must carry one descriptor per extent plus every replacement byte"
    );
    drop(probes);
    drop(workspace);
    drop(host);
    fs::remove_dir_all(&path).unwrap();
}
