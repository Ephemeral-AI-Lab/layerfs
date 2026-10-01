//! Actual native closed/cancelled Source and finite slow-output terminal vectors.
//! One generated32MiB fixture, four General clients opened once, no retries.
//! Two temporary source socketpair FDs; public Pipe enforces deadline/cancellation.
//! Native endpoint vector: four clients2FD each, four Server sessions3FD each,
//! listener1FD plus source2FD; Store/Scratch/Catalog/observer FDs are separate.
//! External jobs use2MiB stacks; at mosttwo run together here. Existing native
//! upload/Server stacks and FRAME16KiB/metadata32KiB limits are unchanged.
//! Source deadline5000ms; slow-output negative deadline250ms, fixed prospectively.
//! Public shutdown joins native workers before the two-read-credit refund oracle.
//! No public intermediate before-join barrier exists: closing-slot retention is
//! NOT_RUN, and no timing-based class-refund assumption or protected-flag PASS.
//! This logical compatible-reader proof is not strict8/global176/native32/physical.
#![cfg(any(target_os = "macos", target_os = "linux"))]
// The shared module also contains gates exercised by native_read_saturation.
#[allow(dead_code)]
#[path = "support/native_saturation.rs"]
mod held;
#[allow(dead_code)]
#[path = "support/draft_observation.rs"]
mod observe;
use layerfs_bridge::{adapters::native::purpose::Purpose, contract::*};
use layerfs_server::{HistoryMode, Server, ServerConfig};
use layerfs_telemetry::runtime::Runtime;
use std::{
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
const SLOW_MS: u32 = 250;
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
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
fn call(server: &Server, id: u64, operation: Operation) -> Result<Response, Failure> {
    server
        .service()
        .handle(
            &server.peer().unwrap(),
            &request(id, operation),
            &mut io::empty(),
            &mut io::sink(),
        )
        .0
}
fn history(response: Response) -> HistoryResult {
    let Response::History(value) = response else {
        panic!("history")
    };
    *value
}
fn active(library: &std::path::Path, store: &std::path::Path) -> u64 {
    observe::number(
        library,
        store,
        "SELECT COUNT(*) FROM saves WHERE active_slot IS NOT NULL",
    )
}
fn wait_active(library: &std::path::Path, store: &std::path::Path, count: u64) {
    let end = Instant::now() + Duration::from_secs(2);
    loop {
        let actual = active(library, store);
        if actual == count {
            return;
        }
        assert!(
            Instant::now() < end,
            "persisted Save count expected={count}, actual={actual}"
        );
        std::thread::yield_now();
    }
}
fn no_stage(server: &Server, id: u64, workspace: [u8; 32]) {
    assert_eq!(
        call(
            server,
            id,
            Operation::HistoryQuery(HistoryQuery::GetStage { workspace })
        )
        .unwrap_err()
        .code,
        Code::NotFound
    );
}
fn capacity(server: &Server, root: Root, id: u64) {
    assert_eq!(
        call(
            server,
            id,
            Operation::ReadFile {
                root,
                start: 0,
                end: 1
            }
        )
        .unwrap_err()
        .code,
        Code::Capacity
    );
}
fn terminal_source_count(end: &held::SourceEnd) {
    assert_eq!(end.reads.load(Ordering::Acquire), 1);
    assert_eq!(end.returned.load(Ordering::Acquire), 1);
    assert_eq!(end.deadline.load(Ordering::Acquire), 0);
}
#[test]
fn native_source_closure_cancel_and_slow_output_end_once_then_joined_read_credits_return() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-native-terminal-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    let temp = Temp(path.clone());
    let server = Server::create(ServerConfig {
        store_path: path.join("store.sqlite"),
        history_path: path.join("history.sqlite"),
        binding_key: b"native-terminal-custody".to_vec(),
        incarnation: 1,
        cursor_key: [7; 32],
        history: HistoryMode::Create,
        service_host: "127.0.0.1".into(),
        runtime: Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    let library = observe::selected_library();
    let store = path.join("store.sqlite");
    let source = path.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("a"), b"original").unwrap();
    let created = server
        .service()
        .init_project(&server.peer().unwrap(), 1, "main", &source)
        .unwrap();
    let fork = history(
        call(
            &server,
            1,
            Operation::HistoryCommand(HistoryCommand::Fork {
                stack: created.stack.stack,
                branch: [71; 16],
                name: b"work".to_vec(),
                source: HistoryForkSource::Layer(created.stack.head_layer),
            }),
        )
        .unwrap(),
    );
    let HistoryResult::BranchSnapshot(fork) = fork else {
        panic!("fork")
    };
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
        panic!("fixture")
    };
    assert_eq!(length, held::DATA_BYTES);
    server.service().drain_construction_idle().unwrap();
    assert_eq!(active(&library, &store), 0);
    assert!(observe::scratch_files(&temp.0).is_empty());
    let header = |workspace| PreparedChanges {
        workspace,
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
    server.listen_with_purposes().unwrap();
    let mut clients: Vec<_> = (0..4)
        .map(|_| server.connect_purpose(Purpose::General).unwrap())
        .collect();
    let mut cancel_client = clients.pop().unwrap();
    let spare = clients.pop().unwrap();
    let mut slow_client = clients.pop().unwrap();
    let mut closed_client = clients.pop().unwrap();
    let terminals = Arc::new(AtomicUsize::new(0));
    // A real persisted Save reaches its body read before the actual source writer descriptor closes.
    let closed_end = Arc::new(held::SourceEnd::default());
    std::thread::scope(|threads| {
        let (ready, arrived) = mpsc::channel();
        let (held, release) = std::os::unix::net::UnixStream::pair().unwrap();
        let end = closed_end.clone();
        let terminal = terminals.clone();
        let changes = header([161; 32]);
        let job = std::thread::Builder::new()
            .name("closed-native-source".into())
            .stack_size(held::STACK_BYTES)
            .spawn_scoped(threads, move || {
                let result = closed_client.call(
                    &request(
                        1,
                        Operation::HistoryCommand(HistoryCommand::StageChanges(changes)),
                    ),
                    &mut held::TerminalSource::new(ready, held, end),
                    &mut io::sink(),
                );
                terminal.fetch_add(1, Ordering::AcqRel);
                result
            })
            .unwrap();
        arrived.recv_timeout(Duration::from_secs(2)).unwrap();
        wait_active(&library, &store, 1);
        assert_eq!(observe::scratch_files(&temp.0).len(), 1);
        drop(release);
        let failure = job.join().unwrap().unwrap_err();
        assert!(matches!(
            failure.code,
            Code::Io | Code::InvalidInput | Code::Unknown
        ));
        if failure.code == Code::Unknown {
            assert!(failure.unknown);
        }
        eprintln!("DIAGNOSTIC one closed native Source terminal: {failure}");
    });
    terminal_source_count(&closed_end);
    assert_eq!(closed_end.closed.load(Ordering::Acquire), 1);
    assert_eq!(closed_end.cancelled.load(Ordering::Acquire), 0);
    assert_eq!(terminals.load(Ordering::Acquire), 1);
    wait_active(&library, &store, 0);
    no_stage(&server, 10, [161; 32]);
    // Prove the native read owner is live during the finite decoded-output barrier.
    let output_end = Arc::new(held::OutputEnd::default());
    std::thread::scope(|threads| {
        let (ready, arrived) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let end = output_end.clone();
        let terminal = terminals.clone();
        let job = std::thread::Builder::new()
            .name("slow-native-output".into())
            .stack_size(held::STACK_BYTES)
            .spawn_scoped(threads, move || {
                let deadline = Instant::now() + Duration::from_millis(u64::from(SLOW_MS));
                let mut output = held::DeadlineOutput::new(ready, held, deadline, end);
                let mut r = request(
                    1,
                    Operation::ReadFile {
                        root,
                        start: 0,
                        end: held::DATA_BYTES,
                    },
                );
                r.deadline_ms = SLOW_MS;
                let result = slow_client.call_until(&r, &mut &[][..], &mut output, deadline);
                terminal.fetch_add(1, Ordering::AcqRel);
                result
            })
            .unwrap();
        let bytes = arrived
            .recv_timeout(Duration::from_millis(u64::from(SLOW_MS)))
            .unwrap();
        assert!((1..=FRAME_BYTES).contains(&bytes));
        let (ready, arrived) = mpsc::channel();
        let (resume, held) = mpsc::channel();
        let service = server.service();
        let peer = server.peer().unwrap();
        let tiny = std::thread::Builder::new()
            .name("slow-proof-second-reader".into())
            .stack_size(held::STACK_BYTES)
            .spawn_scoped(threads, move || {
                let mut output = held::TinyOutput::new(ready, held);
                let result = service
                    .handle(
                        &peer,
                        &request(
                            11,
                            Operation::ReadFile {
                                root,
                                start: 0,
                                end: 1,
                            },
                        ),
                        &mut io::empty(),
                        &mut output,
                    )
                    .0;
                (result, output.bytes)
            })
            .unwrap();
        arrived
            .recv_timeout(Duration::from_millis(u64::from(SLOW_MS)))
            .unwrap();
        capacity(&server, root, 12);
        resume.send(()).unwrap();
        let (result, bytes) = tiny.join().unwrap();
        assert_eq!(result.unwrap(), Response::Read { length: 1 });
        assert_eq!(bytes, 1);
        let failure = job.join().unwrap().unwrap_err();
        assert!(matches!(failure.code, Code::Io | Code::Deadline));
        assert!(!failure.unknown);
        assert_eq!(output_end.writes.load(Ordering::Acquire), 1);
        assert_eq!(output_end.deadline.load(Ordering::Acquire), 1);
        drop(release);
        eprintln!("DIAGNOSTIC one fixed250ms slow-output terminal: {failure}");
    });
    assert_eq!(terminals.load(Ordering::Acquire), 2);
    // Keep the source writer FD live: only actual native transport cancellation ends the read.
    let cancel_end = Arc::new(held::SourceEnd::default());
    std::thread::scope(|threads| {
        let (ready, arrived) = mpsc::channel();
        let (held, release) = std::os::unix::net::UnixStream::pair().unwrap();
        let end = cancel_end.clone();
        let terminal = terminals.clone();
        let changes = header([162; 32]);
        let job = std::thread::Builder::new()
            .name("cancel-native-source".into())
            .stack_size(held::STACK_BYTES)
            .spawn_scoped(threads, move || {
                let result = cancel_client.call(
                    &request(
                        1,
                        Operation::HistoryCommand(HistoryCommand::StageChanges(changes)),
                    ),
                    &mut held::TerminalSource::new(ready, held, end),
                    &mut io::sink(),
                );
                terminal.fetch_add(1, Ordering::AcqRel);
                result
            })
            .unwrap();
        arrived.recv_timeout(Duration::from_secs(2)).unwrap();
        wait_active(&library, &store, 1);
        assert_eq!(observe::scratch_files(&temp.0).len(), 1);
        server.shutdown();
        let failure = job.join().unwrap().unwrap_err();
        assert_eq!(failure.code, Code::Unknown);
        assert!(failure.unknown);
        drop(release);
        eprintln!("DIAGNOSTIC one cancelled native Source terminal: {failure}");
    });
    terminal_source_count(&cancel_end);
    assert_eq!(cancel_end.cancelled.load(Ordering::Acquire), 1);
    assert_eq!(cancel_end.closed.load(Ordering::Acquire), 0);
    assert_eq!(terminals.load(Ordering::Acquire), 3);
    assert_eq!(active(&library, &store), 0);
    no_stage(&server, 13, [162; 32]);
    assert!(observe::scratch_files(&temp.0).is_empty());
    drop(spare);
    // Shutdown is the public joined-worker boundary, not a guessed settling delay.
    // Both complete read allowances must now admit before the third is refused.
    std::thread::scope(|threads| {
        let mut releases = Vec::new();
        let mut readers = Vec::new();
        for id in [14, 15] {
            let (ready, arrived) = mpsc::channel();
            let (release, held) = mpsc::channel();
            releases.push(release);
            let service = server.service();
            let peer = server.peer().unwrap();
            readers.push(
                std::thread::Builder::new()
                    .name(format!("refunded-reader-{id}"))
                    .stack_size(held::STACK_BYTES)
                    .spawn_scoped(threads, move || {
                        let mut output = held::TinyOutput::new(ready, held);
                        let result = service
                            .handle(
                                &peer,
                                &request(
                                    id,
                                    Operation::ReadFile {
                                        root,
                                        start: 0,
                                        end: 1,
                                    },
                                ),
                                &mut io::empty(),
                                &mut output,
                            )
                            .0;
                        (result, output.bytes)
                    })
                    .unwrap(),
            );
            arrived.recv_timeout(Duration::from_secs(1)).unwrap();
        }
        capacity(&server, root, 16);
        for (release, reader) in releases.into_iter().zip(readers) {
            release.send(()).unwrap();
            let (result, bytes) = reader.join().unwrap();
            assert_eq!(result.unwrap(), Response::Read { length: 1 });
            assert_eq!(bytes, 1);
        }
    });
    assert_eq!(active(&library, &store), 0);
    assert!(observe::scratch_files(&temp.0).is_empty());
    eprintln!("NOT_RUN intermediate closing-before-join native slot-retention observation: public Server/Client APIs expose no barrier for that stage; actual joined shutdown/drain and complete read-credit refund are checked separately");
}
