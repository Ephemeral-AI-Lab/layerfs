//! Per-Store write admission on the real service route (#216).
//!
//! The writers here are real operations held open inside their input read, not
//! sleeps or timing probes: each blocked writer has already passed admission and
//! taken a Store slot, which is what makes the bounded refusals below meaningful.
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_service::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    io::{self, Cursor, Read, Write},
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
/// Unique directory for one case.
fn directory(label: &str) -> Temp {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "layerfs-admission-{label}-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    Temp(path)
}
fn store_at(path: &std::path::Path, writes: u8) -> Store {
    let store = Timing::disabled("create", |s| {
        Store::create(
            path.join("store.sqlite"),
            Store::default_policy(),
            s.child("create"),
        )
    })
    .0
    .unwrap();
    let applied = Timing::disabled("configure", |s| {
        store.set_max_concurrent_writes(writes, s.child("configure"))
    })
    .0
    .unwrap();
    assert_eq!(applied, writes);
    store
}
fn peer() -> VerifiedPeer {
    VerifiedPeer::from_private(&[7; 32]).unwrap()
}
fn service(stores: Vec<(u32, Store)>) -> Arc<Service> {
    let operations = 31;
    Arc::new(
        Service::new(
            stores
                .into_iter()
                .map(|(id, store)| StoreAccess {
                    id,
                    store,
                    history: None,
                    grants: vec![Grant {
                        public_key: *peer().public_key(),
                        operations,
                        expires_unix: u64::MAX,
                    }],
                })
                .collect(),
            OperationRecorder::disabled(),
        )
        .unwrap(),
    )
}
fn request(store: u32, id: u64, operation: Operation) -> Request {
    Request {
        id,
        generation: 1,
        store,
        profile: 1,
        deadline_ms: 30000,
        response_bytes: MAX_FILE,
        operation,
    }
}
/// Input that reports its first read and then blocks until the case releases it.
///
/// A released writer delivers its whole payload and completes normally, so the
/// permit it held is proven by the successful save that follows the wait.
struct Gate {
    opened: Sender<()>,
    release: Receiver<()>,
    waiting: bool,
    payload: Cursor<Vec<u8>>,
}
impl Gate {
    fn new(opened: Sender<()>, release: Receiver<()>) -> Self {
        Self {
            opened,
            release,
            waiting: true,
            payload: Cursor::new(vec![7u8; 64]),
        }
    }
}
impl Read for Gate {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.waiting {
            self.waiting = false;
            let _ = self.opened.send(());
            // The writer holds its permit and its Store slot across this wait.
            let _ = self.release.recv();
        }
        self.payload.read(buffer)
    }
}
/// One blocked writer: the thread, and the channel that releases it.
struct Blocked {
    release: Sender<()>,
    worker: std::thread::JoinHandle<Result<Response, Failure>>,
}

/// Starts `count` writers that stop inside their input read.
fn block_writers(service: &Arc<Service>, store: u32, count: usize) -> (Receiver<()>, Vec<Blocked>) {
    let (opened, first_read) = mpsc::channel();
    let blocked = (0..count)
        .map(|index| {
            let service = Arc::clone(service);
            let peer = peer();
            let opened = opened.clone();
            let (release, held) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let request = request(
                    store,
                    index as u64 + 1,
                    Operation::ConstructFile { length: 64 },
                );
                service
                    .handle(
                        &peer,
                        &request,
                        &mut Gate::new(opened, held),
                        &mut io::sink(),
                    )
                    .0
            });
            Blocked { release, worker }
        })
        .collect();
    (first_read, blocked)
}
/// Waits until every blocked writer has reached its input read.
fn await_admitted(first_read: &Receiver<()>, count: usize) {
    for index in 0..count {
        first_read
            .recv_timeout(Duration::from_secs(30))
            .unwrap_or_else(|error| panic!("writer {index} did not reach its input: {error}"));
    }
}
fn write_once(service: &Service, store: u32, id: u64) -> Result<Response, Failure> {
    let request = request(store, id, Operation::ConstructFile { length: 64 });
    service
        .handle(
            &peer(),
            &request,
            &mut Cursor::new(vec![7u8; 64]),
            &mut io::sink(),
        )
        .0
}
fn read_once(service: &Service, store: u32, id: u64) -> Result<Response, Failure> {
    let request = request(
        store,
        id,
        Operation::ReadFile {
            root: [0; 32],
            start: 0,
            end: 1,
        },
    );
    service
        .handle(&peer(), &request, &mut io::empty(), &mut io::sink())
        .0
}
/// Releases every blocked writer and requires all of them to have ended.
fn release(blocked: Vec<Blocked>) {
    for held in blocked {
        let _ = held.release.send(());
        let result = held.worker.join().expect("the writer thread ended");
        // Each released writer still completes its own operation, so the wait
        // held a real permit and the release returned it.
        assert!(matches!(result, Ok(Response::Saved { .. })), "{result:?}");
    }
}
#[test]
fn four_writers_are_admitted_then_refused_and_reads_never_queue_behind_them() {
    let temp = directory("four");
    let service = service(vec![(1, store_at(&temp.0, 4))]);
    let (first_read, blocked) = block_writers(&service, 1, 4);
    await_admitted(&first_read, 4);
    // The fifth writer is refused explicitly and without waiting.
    match write_once(&service, 1, 90) {
        Err(failure) => assert_eq!(failure.code, Code::Capacity, "{failure}"),
        Ok(response) => panic!("a fifth writer must be refused: {response:?}"),
    }
    // A read takes no writer permit: it is admitted while all four are busy and
    // reaches the operation itself, which reports the absent root.
    match read_once(&service, 1, 91) {
        Err(failure) => assert_eq!(failure.code, Code::MissingObject, "{failure}"),
        Ok(response) => panic!("an unknown root cannot be read: {response:?}"),
    }
    release(blocked);
    // Every permit came back: the same route saves normally afterwards.
    let saved = write_once(&service, 1, 92).expect("a released budget admits the next writer");
    assert!(matches!(saved, Response::Saved { .. }));
}

#[test]
fn two_sandboxes_over_one_store_share_its_budget() {
    let temp = directory("shared");
    let path = temp.0.join("store.sqlite");
    let first = store_at(&temp.0, 2);
    let second = Timing::disabled("open", |s| Store::open(&path, s.child("open")))
        .0
        .unwrap();
    assert_eq!(second.max_concurrent_writes().unwrap(), 2);
    let first_service = service(vec![(1, first)]);
    let second_service = service(vec![(1, second)]);
    let (first_read, first_blocked) = block_writers(&first_service, 1, 2);
    await_admitted(&first_read, 2);
    // The other sandbox admits nothing: the Store's slots are held, and the
    // refusal is the Store's own bounded ownership refusal, not a silent write.
    match write_once(&second_service, 1, 5) {
        Err(failure) => assert_eq!(failure.code, Code::Ownership, "{failure}"),
        Ok(response) => panic!("the second sandbox must not exceed the Store budget: {response:?}"),
    }
    release(first_blocked);
    let saved = write_once(&second_service, 1, 6).expect("the released budget is shared");
    assert!(matches!(saved, Response::Saved { .. }));
}

#[test]
fn another_store_keeps_its_own_budget() {
    let temp = directory("own");
    let exhausted = store_at(&temp.0, 1);
    let other_dir = directory("other");
    let other = store_at(&other_dir.0, 4);
    let service = service(vec![(1, exhausted), (2, other)]);
    let (first_read, blocked) = block_writers(&service, 1, 1);
    await_admitted(&first_read, 1);
    match write_once(&service, 1, 8) {
        Err(failure) => assert_eq!(failure.code, Code::Capacity, "{failure}"),
        Ok(response) => panic!("Store 1 is at its budget: {response:?}"),
    }
    let saved = write_once(&service, 2, 9).expect("Store 2 has its own budget");
    assert!(matches!(saved, Response::Saved { .. }));
    release(blocked);
}

/// Output that reports its first write and then blocks until it is released.
struct GateWriter {
    opened: Sender<()>,
    release: Receiver<()>,
    waiting: bool,
}
impl Write for GateWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.waiting {
            self.waiting = false;
            let _ = self.opened.send(());
            let _ = self.release.recv();
        }
        Ok(buffer.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn a_busy_reader_bound_neither_blocks_writers_nor_exceeds_itself() {
    let temp = directory("reads");
    let service = service(vec![(1, store_at(&temp.0, 1))]);
    let saved = write_once(&service, 1, 1).expect("the fixture file is saved");
    let Response::Saved { root, length, .. } = saved else {
        panic!("saved")
    };
    // The whole read bound is held by readers blocked in their own output write.
    let (opened, first_write) = mpsc::channel();
    let readers: Vec<_> = (0..MAX_READ_OPERATIONS)
        .map(|index| {
            let service = Arc::clone(&service);
            let opened: Sender<()> = opened.clone();
            let (release, held) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                let request = request(
                    1,
                    index as u64 + 10,
                    Operation::ReadFile {
                        root,
                        start: 0,
                        end: length,
                    },
                );
                let (result, _) = service.handle(
                    &peer(),
                    &request,
                    &mut io::empty(),
                    &mut GateWriter {
                        opened,
                        release: held,
                        waiting: true,
                    },
                );
                result
            });
            (release, worker)
        })
        .collect();
    drop(opened);
    for index in 0..MAX_READ_OPERATIONS {
        first_write
            .recv_timeout(Duration::from_secs(30))
            .unwrap_or_else(|error| panic!("reader {index} did not start writing: {error}"));
    }
    // One read too many is refused at the read bound, which is a resource bound
    // and not a writer budget.
    match read_once(&service, 1, 20) {
        Err(failure) => assert_eq!(failure.code, Code::Capacity, "{failure}"),
        Ok(response) => panic!("the read bound must refuse the extra reader: {response:?}"),
    }
    // A writer does not wait for readers: the writer budget is untouched by them.
    let saved = write_once(&service, 1, 21).expect("readers hold no writer permit");
    assert!(matches!(saved, Response::Saved { .. }));
    for (initial, (release, worker)) in readers.into_iter().enumerate() {
        let _ = release.send(());
        let result = worker.join().expect("the reader thread ended");
        match result {
            Ok(Response::Read { length: read }) => assert_eq!(read, length),
            other => panic!("reader {initial} did not complete its read: {other:?}"),
        }
    }
}
