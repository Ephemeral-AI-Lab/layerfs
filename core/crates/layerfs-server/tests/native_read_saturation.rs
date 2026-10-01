//! Actual protected native Catalog refill with two persisted Saves and two readers.
//! Prospective functional vector: General4, Catalog1, Control1, Pending1;
//! FRAME16KiB, metadata32KiB, unchanged request/progress deadlines5000ms.
//! Six client native connections have2FD each; six authenticated Server sessions
//! have3FD each; one pending handshake has2FD, listener1FD, two hostile raw
//! client sockets2FD and at mostone transient refused accepted FD. Provider,
//! Store/Scratch/Catalog descriptors are separate; no global FD/physical claim.
//! Four external job stacks are2MiB, as are existing upload/Server workers.
//! Output readiness requires an independent third-read Capacity refusal before
//! AND after refill. A buffered completed Server read is an explicit failed proof.
//! Captured32MiB-compatible readers do not establish strict8/global176/native32.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "support/native_saturation.rs"]
mod held;
#[allow(dead_code)]
#[path = "support/draft_observation.rs"]
mod observe;
use layerfs_bridge::{adapters::native::purpose::Purpose, contract::*};
use layerfs_server::{HistoryMode, Server, ServerConfig};
use layerfs_telemetry::runtime::Runtime;
use std::{
    io::{self, Read},
    net::TcpStream,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Temp, Server) {
    let path = std::env::temp_dir().join(format!(
        "layerfs-native-read-saturation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    let server = Server::create(ServerConfig {
        store_path: path.join("store.sqlite"),
        history_path: path.join("history.sqlite"),
        binding_key: b"native-read-saturation".to_vec(),
        incarnation: 1,
        cursor_key: [7; 32],
        history: HistoryMode::Create,
        service_host: "127.0.0.1".into(),
        runtime: Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    (Temp(path), server)
}
fn request(id: u64, operation: Operation) -> Request {
    Request {
        id,
        generation: 1,
        store: 1,
        profile: if matches!(
            operation,
            Operation::HistoryCommand(_) | Operation::HistoryQuery(_)
        ) {
            HISTORY_PROFILE
        } else {
            1
        },
        deadline_ms: held::DEADLINE_MS,
        response_bytes: if let Operation::ReadFile { start, end, .. } = operation {
            end - start
        } else {
            0
        },
        operation,
    }
}
fn active(library: &std::path::Path, path: &std::path::Path) -> u64 {
    observe::number(
        library,
        path,
        "SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL",
    )
}
fn wait_active(library: &std::path::Path, path: &std::path::Path, count: u64) {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let actual = active(library, path);
        if actual == count {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "actual persisted Saves expected={count}, actual={actual}"
        );
        std::thread::yield_now();
    }
}
fn history(response: Response) -> HistoryResult {
    let Response::History(result) = response else {
        panic!("history result: {response:?}")
    };
    *result
}
fn real_reader_capacity(server: &Server, root: Root, id: u64) {
    let result = server
        .service()
        .handle(
            &server.peer().unwrap(),
            &request(
                id,
                Operation::ReadFile {
                    root,
                    start: 0,
                    end: 1,
                },
            ),
            &mut io::empty(),
            &mut io::sink(),
        )
        .0;
    assert_eq!(result.unwrap_err().code,Code::Capacity,"Output gates alone do not prove Server reader retention; both actual native ReadPermits must still be live");
}
#[test]
fn catalog_refill_completes_with_two_native_read_owners_and_two_persisted_saves_held() {
    let (temp, server) = fixture();
    let library = observe::selected_library();
    let store = temp.0.join("store.sqlite");
    let source = temp.0.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("a"), b"independent original").unwrap();
    let created = server
        .service()
        .init_project(&server.peer().unwrap(), 1, "main", &source)
        .unwrap();
    let fork = history(
        server
            .service()
            .handle(
                &server.peer().unwrap(),
                &request(
                    1,
                    Operation::HistoryCommand(HistoryCommand::Fork {
                        stack: created.stack.stack,
                        branch: [61; 16],
                        name: b"work".to_vec(),
                        source: HistoryForkSource::Layer(created.stack.head_layer),
                    }),
                ),
                &mut io::empty(),
                &mut io::sink(),
            )
            .0
            .unwrap(),
    );
    let HistoryResult::BranchSnapshot(fork) = fork else {
        panic!("fork")
    };
    // Prepare exactly once before held owners. No phase/cache/speed claim.
    let saved = server
        .service()
        .handle(
            &server.peer().unwrap(),
            &request(
                2,
                Operation::SaveFile {
                    base: None,
                    base_length: 0,
                    length: held::DATA_BYTES,
                    extents: 1,
                    replacement: held::DATA_BYTES,
                },
            ),
            &mut held::Generated::new(),
            &mut io::sink(),
        )
        .0
        .unwrap();
    let Response::Saved { root, length, .. } = saved else {
        panic!("saved generated fixture")
    };
    assert_eq!(length, held::DATA_BYTES);
    server.service().drain_construction_idle().unwrap();
    assert_eq!(active(&library, &store), 0);
    assert!(observe::scratch_files(&temp.0).is_empty());
    let mut wire = vec![1];
    wire.extend_from_slice(&created.root_serial.to_be_bytes());
    wire.extend_from_slice(&1u32.to_be_bytes());
    wire.extend_from_slice(&1u16.to_be_bytes());
    wire.push(b'a');
    wire.extend_from_slice(&0u64.to_be_bytes());
    let endpoint = server.listen_with_purposes().unwrap();
    let mut catalog = server.connect_purpose(Purpose::Catalog).unwrap();
    let control = server.connect_purpose(Purpose::Control).unwrap();
    let mut general: Vec<_> = (0..4)
        .map(|_| server.connect_purpose(Purpose::General).unwrap())
        .collect();
    let resumes = Arc::new(AtomicUsize::new(0));
    let terminals = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|threads| {
        let mut save_releases = Vec::new();
        let mut writers = Vec::new();
        for (i, mut client) in general.drain(..2).enumerate() {
            let header = PreparedChanges {
                workspace: [151 + i as u8; 32],
                branch: fork.branch.branch,
                expected_head: fork.branch.head_commit,
                expected_base: fork.branch.base_layer,
                generation: 1,
                base: fork.effective_root,
                scope: fork.scope,
                root_serial: created.root_serial,
                totals: PreparedTotals {
                    directories: 1,
                    names: 1,
                    name_bytes: 11,
                    ..PreparedTotals::default()
                },
            };
            assert_eq!(header.stream_bytes().unwrap(), wire.len() as u64);
            let (ready, arrived) = mpsc::channel();
            let (release, held) = mpsc::channel();
            save_releases.push(release);
            let body = wire.clone();
            let resumed = resumes.clone();
            let terminal = terminals.clone();
            writers.push(
                std::thread::Builder::new()
                    .name(format!("saturation-save-{i}"))
                    .stack_size(held::STACK_BYTES)
                    .spawn_scoped(threads, move || {
                        let result = client.call(
                            &request(
                                1,
                                Operation::HistoryCommand(HistoryCommand::StageChanges(header)),
                            ),
                            &mut held::SourceGate::new(body, ready, held, resumed),
                            &mut io::sink(),
                        );
                        terminal.fetch_add(1, Ordering::AcqRel);
                        (client, result)
                    })
                    .unwrap(),
            );
            arrived.recv_timeout(Duration::from_secs(2)).unwrap();
            // This actual SQL acknowledgement precedes the second Save's birth.
            wait_active(&library, &store, i as u64 + 1);
        }
        assert_eq!(
            observe::scratch_files(&temp.0).len(),
            2,
            "two pre-body native metadata owners"
        );
        let mut read_releases = Vec::new();
        let mut readers = Vec::new();
        for (i, mut client) in general.drain(..).enumerate() {
            let (ready, arrived) = mpsc::channel();
            let (release, held) = mpsc::channel();
            read_releases.push(release);
            let terminal = terminals.clone();
            readers.push(
                std::thread::Builder::new()
                    .name(format!("saturation-read-{i}"))
                    .stack_size(held::STACK_BYTES)
                    .spawn_scoped(threads, move || {
                        let deadline =
                            Instant::now() + Duration::from_millis(u64::from(held::DEADLINE_MS));
                        let mut output = held::CheckedGate::new(ready, held, deadline);
                        let result = client.call_until(
                            &request(
                                1,
                                Operation::ReadFile {
                                    root,
                                    start: 0,
                                    end: held::DATA_BYTES,
                                },
                            ),
                            &mut &[][..],
                            &mut output,
                            deadline,
                        );
                        terminal.fetch_add(1, Ordering::AcqRel);
                        (client, result, output.position)
                    })
                    .unwrap(),
            );
            let first = arrived.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!((1..=FRAME_BYTES).contains(&first));
        }
        real_reader_capacity(&server, root, 101);
        assert_eq!(resumes.load(Ordering::Acquire), 0);
        assert_eq!(terminals.load(Ordering::Acquire), 0);
        assert!(writers.iter().all(|job| !job.is_finished()));
        assert!(readers.iter().all(|job| !job.is_finished()));
        assert!(server.connect_purpose(Purpose::General).is_err());
        // Real preauthentication owner plus a second finite refusal, no valid HELLO.
        let hostile = TcpStream::connect(endpoint).unwrap();
        let mut refused = TcpStream::connect(endpoint).unwrap();
        refused
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        match refused.read(&mut [0]) {
            Ok(0) => {}
            Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
            other => panic!("pending-class refusal did not terminate: {other:?}"),
        }
        let reserved = history(
            catalog
                .call(
                    &request(
                        1,
                        Operation::HistoryCommand(HistoryCommand::ReserveInodes {
                            scope: fork.scope,
                            count: 2,
                        }),
                    ),
                    &mut &[][..],
                    &mut io::sink(),
                )
                .unwrap(),
        );
        assert!(
            matches!(reserved,HistoryResult::Reservation{scope,count:2,..} if scope==fork.scope)
        );
        real_reader_capacity(&server, root, 102);
        assert_eq!(active(&library, &store), 2);
        assert_eq!(resumes.load(Ordering::Acquire), 0);
        assert_eq!(terminals.load(Ordering::Acquire), 0);
        assert_eq!(observe::scratch_files(&temp.0).len(), 2);
        drop(refused);
        drop(hostile);
        // Finish ordinary read outputs before either distinct Save resumes.
        for (release, reader) in read_releases.into_iter().zip(readers) {
            release.send(()).unwrap();
            let (mut client, result, bytes) = reader.join().unwrap();
            assert_eq!(
                result.unwrap(),
                Response::Read {
                    length: held::DATA_BYTES
                }
            );
            assert_eq!(bytes, held::DATA_BYTES);
            // A second public call would reject a duplicated or stale native terminal.
            assert!(
                matches!(client.call(&request(2,Operation::Inspect{root,query:Inspect::File}),&mut &[][..],&mut io::sink()).unwrap(),Response::File{length,..} if length==held::DATA_BYTES)
            );
        }
        assert_eq!(terminals.load(Ordering::Acquire), 2);
        assert_eq!(resumes.load(Ordering::Acquire), 0);
        for (release, writer) in save_releases.into_iter().zip(writers) {
            release.send(()).unwrap();
            let (_client, result) = writer.join().unwrap();
            assert!(matches!(result.unwrap(), Response::History(_)));
        }
    });
    assert_eq!(terminals.load(Ordering::Acquire), 4);
    assert_eq!(resumes.load(Ordering::Acquire), 2);
    assert_eq!(active(&library, &store), 0);
    let mut one = Vec::new();
    assert_eq!(
        server
            .service()
            .handle(
                &server.peer().unwrap(),
                &request(
                    103,
                    Operation::ReadFile {
                        root,
                        start: 0,
                        end: 1
                    }
                ),
                &mut io::empty(),
                &mut one
            )
            .0
            .unwrap(),
        Response::Read { length: 1 }
    );
    assert_eq!(one, [held::expected(0)]);
    assert_eq!(
        observe::scratch_files(&temp.0).len(),
        2,
        "known successful scratch remains charged idle"
    );
    drop(catalog);
    drop(control);
    server.shutdown();
    assert!(
        observe::scratch_files(&temp.0).is_empty(),
        "explicit shutdown drains both known metadata owners"
    );
}
