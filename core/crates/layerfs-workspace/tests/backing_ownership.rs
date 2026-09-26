//! Counted private ownership work through the ordinary native Workspace API.
//!
//! One accepted payload mutation must not walk every earlier acquisition, and a
//! quota refusal or a live reader pin must leave its owners exactly where they
//! were. The counts come from the production backing status; this is a
//! correctness and work-count diagnostic, not a performance or cache claim.
#![cfg(target_os = "linux")]
use layerfs_bridge::contract::Source;
use layerfs_workspace::*;
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

const MIB: u64 = 1024 * 1024;
const AWAIT: u64 = 20;
const PAYLOAD: u64 = 1;
const SEGMENT: u64 = 8192;
/// The acquisition counts this diagnostic walks through. Each row reports the
/// average routine ownership work one accepted write paid since the row before.
const CHECKPOINTS: [usize; 6] = [1, 8, 64, 256, 512, 1024];

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(AWAIT)
}
fn config(path: &Path, quota: u64) -> WorkspaceConfig {
    WorkspaceConfig {
        root: path.to_owned(),
        max_count: 2,
        memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
        disk_budget_bytes: Some(quota),
    }
}
fn delivery() -> OperationDelivery {
    Arc::new(|request, _, _, _| match &request.operation {
        layerfs_bridge::contract::Operation::Inspect {
            query: layerfs_bridge::contract::Inspect::Attributes { path },
            ..
        } if path.is_empty() => Ok(layerfs_bridge::contract::Response::Attributes {
            serial: 7,
            kind: 2,
            references: 0,
            content: [7; 32],
            metadata: [1; 32],
            mode: 0o755,
            mtime: -1,
            nanoseconds: 123,
            size: 0,
        }),
        _ => Err(layerfs_bridge::contract::Code::Unsupported.into()),
    })
}
fn private(path: &Path) {
    let mut builder = fs::DirBuilder::new();
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
    builder.create(path).unwrap();
}
fn temporary(parent: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = parent.canonicalize().unwrap().join(format!(
        "ownership-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    private(&path);
    path
}
fn pass(id: &str) {
    println!("OWNERSHIP_CHECK {id} PASS");
}

struct Fixture {
    path: PathBuf,
    host: WorkspaceHost,
    workspace: Workspace,
}
impl Fixture {
    fn new(quota: u64) -> Self {
        let path = temporary(&std::env::temp_dir());
        let metadata = fs::metadata(&path).unwrap();
        use std::os::unix::fs::MetadataExt;
        let host = WorkspaceHost::new(config(&path, quota), delivery()).unwrap();
        let workspace = host
            .attach(
                AttachOptions {
                    id: "ownership".into(),
                    incarnation: [23; 32],
                    store: 1,
                    base: Base::Root([1; 32]),
                    access: WorkspaceAccess::ReadOnly,
                    owner_uid: metadata.uid(),
                    owner_gid: metadata.gid(),
                },
                deadline(),
            )
            .unwrap();
        Self {
            path,
            host,
            workspace,
        }
    }
    fn backing(&self) -> PathBuf {
        self.path.join("private-backing/ownership")
    }
    fn own(&self, bytes: &[u8]) -> OwnedPayload {
        self.workspace
            .own_payload(bytes.len() as u64, &mut &bytes[..], deadline())
            .unwrap()
    }
    fn status(&self) -> BackingStatus {
        self.workspace.backing_status().unwrap()
    }
    fn clean(self) {
        self.workspace.reclaim_payloads(deadline()).unwrap();
        let status = self.status();
        assert_eq!(
            (
                status.payloads,
                status.allocated_bytes,
                status.reserved_bytes
            ),
            (0, 0, 0)
        );
        self.workspace.close_clean_until(deadline()).unwrap();
        assert!(!self.backing().exists());
        drop(self.workspace);
        drop(self.host);
        fs::remove_dir_all(self.path).unwrap();
    }
}
/// Reads a payload through its public reader and checks every byte.
fn verify(payload: &OwnedPayload, start: u64, length: u64) {
    let cancel = AtomicBool::new(false);
    let mut buffer = [0; 4093];
    let mut reader = payload.reader(start..start + length).unwrap();
    let mut position = 0;
    while position < length {
        let n = reader.read(&mut buffer, deadline(), &cancel).unwrap();
        assert!(n > 0 && n as u64 <= length - position);
        position += n as u64;
    }
    assert_eq!(reader.read(&mut buffer, deadline(), &cancel).unwrap(), 0);
}
struct Unread;
impl Source for Unread {
    fn read(&mut self, _: &mut [u8], _: Instant, _: &AtomicBool) -> io::Result<usize> {
        panic!("refused input was consumed")
    }
}

#[test]
fn routine_reclamation_does_not_rescan_earlier_acquisitions() {
    let f = Fixture::new(64 * MIB);
    let mut held = Vec::new();
    let mut previous = (0usize, 0u64, 0u64);
    for index in 0..CHECKPOINTS[CHECKPOINTS.len() - 1] {
        let byte = [(index % 251) as u8];
        held.push(f.own(&byte));
        if !CHECKPOINTS.contains(&(index + 1)) {
            continue;
        }
        let status = f.status();
        let writes = (index + 1 - previous.0) as f64;
        println!(
            "OWNERSHIP_TRACE accepted={} payloads={} writes={} routine_scans={} routine_per_write={:.3} lookup_scans={} allocated_bytes={}",
            index + 1,
            status.payloads,
            writes as u64,
            status.routine_scans - previous.1,
            (status.routine_scans - previous.1) as f64 / writes,
            status.lookup_scans - previous.2,
            status.allocated_bytes
        );
        previous = (index + 1, status.routine_scans, status.lookup_scans);
    }
    let status = f.status();
    assert!(status.accounting_complete && !status.admission_stopped);
    assert_eq!(status.payloads, CHECKPOINTS[CHECKPOINTS.len() - 1]);
    assert_eq!(status.allocated_bytes, status.payloads as u64 * SEGMENT);
    assert_eq!(status.failed_payloads, 0);
    assert_eq!(status.retained_payloads, status.payloads);
    for payload in &held {
        assert_eq!(payload.len(), PAYLOAD);
        verify(payload, 0, PAYLOAD);
        assert!(payload.reader(PAYLOAD..PAYLOAD + 1).is_err());
    }
    println!(
        "OWNERSHIP_RESOURCE retained={} routine_scans={} lookup_scans={}",
        status.payloads, status.routine_scans, status.lookup_scans
    );
    drop(held);
    let cleanup = f.workspace.reclaim_payloads(deadline()).unwrap();
    let status = f.status();
    assert_eq!(
        cleanup.payloads_released,
        CHECKPOINTS[CHECKPOINTS.len() - 1]
    );
    assert_eq!(status.payloads, 0);
    // The same counter observes the deliberate scoped walk at teardown, so a
    // zero routine count above is an absence of walks rather than a dead field.
    assert!(status.lookup_scans > 0);
    f.clean();
    pass("routine-reclamation-does-not-rescan-earlier-acquisitions");
}

#[test]
fn refused_admission_preserves_its_existing_owners() {
    let f = Fixture::new(SEGMENT);
    let held = f.own(b"a");
    let status = f.status();
    assert_eq!(
        (
            status.payloads,
            status.allocated_bytes,
            status.reserved_bytes
        ),
        (1, SEGMENT, 0)
    );
    let refused = match f.workspace.own_payload(PAYLOAD, &mut Unread, deadline()) {
        Ok(_) => panic!("a full private quota must refuse the next admission"),
        Err(error) => error,
    };
    match refused {
        WorkspaceError::Backing(failure) => {
            assert_eq!(failure.phase, BackingPhase::Acquire);
            assert_eq!(failure.kind, io::ErrorKind::StorageFull);
            assert_eq!(failure.payload, 0);
        }
        other => panic!("quota refusal must be an accounted backing failure: {other:?}"),
    }
    let status = f.status();
    assert_eq!(
        (
            status.payloads,
            status.allocated_bytes,
            status.reserved_bytes
        ),
        (1, SEGMENT, 0),
        "a refused admission may not charge or drop the existing owner"
    );
    assert_eq!(status.failed_payloads, 0);
    verify(&held, 0, PAYLOAD);
    assert!(fs::read_dir(f.backing()).unwrap().count() >= 1);
    drop(held);
    let cleanup = f.workspace.reclaim_payloads(deadline()).unwrap();
    assert_eq!(
        (cleanup.payloads_released, cleanup.bytes_released),
        (1, SEGMENT)
    );
    assert!(fs::read_dir(f.backing()).unwrap().next().is_none());
    let admitted = f.own(b"b");
    verify(&admitted, 0, PAYLOAD);
    drop(admitted);
    f.clean();
    pass("refused-admission-preserves-its-existing-owners");
}

#[test]
fn reader_and_owner_pins_survive_routine_maintenance() {
    let f = Fixture::new(64 * MIB);
    let pinned = f.own(b"p");
    let reader = pinned.reader(0..PAYLOAD).unwrap();
    let mut churn = Vec::new();
    for index in 0..256 {
        churn.push(f.own(&[(index % 251) as u8]));
    }
    drop(churn);
    let status = f.status();
    assert!(
        status.payloads >= 1,
        "a pinned or just-released owner cannot vanish from the account"
    );
    assert!(status.retained_payloads >= 1);
    verify(&pinned, 0, PAYLOAD);
    drop(reader);
    drop(pinned);
    println!(
        "OWNERSHIP_RESOURCE pinned_survived=1 payloads={} routine_scans={} lookup_scans={}",
        status.payloads, status.routine_scans, status.lookup_scans
    );
    f.workspace.reclaim_payloads(deadline()).unwrap();
    assert_eq!(f.status().payloads, 0);
    f.clean();
    pass("reader-and-owner-pins-survive-routine-maintenance");
}
