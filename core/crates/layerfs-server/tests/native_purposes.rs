//! Real composed native class saturation with persisted Saves and metadata scratch.
//! Logical Darwin/library dispatch proof; strict engine/physical qualification is separate.
#![cfg(any(target_os = "macos", target_os = "linux"))]
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
    sync::{atomic::AtomicBool, mpsc},
    time::{Duration, Instant},
};
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture(label: &str) -> (Temp, Server) {
    let path = std::env::temp_dir().join(format!(
        "layerfs-native-purpose-{label}-{}-{}",
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
        binding_key: b"native-purposes".to_vec(),
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
        deadline_ms: 5000,
        response_bytes: 0,
        operation,
    }
}
struct Gate {
    body: io::Cursor<Vec<u8>>,
    ready: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    first: bool,
}
impl Source for Gate {
    fn read(&mut self, b: &mut [u8], deadline: Instant, cancel: &AtomicBool) -> io::Result<usize> {
        if self.first {
            self.first = false;
            self.ready.send(()).unwrap();
            self.release
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| io::ErrorKind::TimedOut)?;
        }
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        Read::read(&mut self.body, b)
    }
}
fn active(path: &std::path::Path) -> u64 {
    observe::number(
        &observe::selected_library(),
        path,
        "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL",
    )
}
#[test]
fn authenticated_catalog_refill_crosses_general_saturation_and_two_actual_held_saves() {
    let (temp, server) = fixture("held-saves");
    let source = temp.0.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("a"), b"independent original").unwrap();
    let created = server
        .service()
        .init_project(&server.peer().unwrap(), 1, "main", &source)
        .unwrap();
    let Response::History(fork) = server
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
        .unwrap()
    else {
        panic!("fork")
    };
    let HistoryResult::BranchSnapshot(fork) = *fork else {
        panic!("branch")
    };
    let mut wire = vec![1];
    wire.extend_from_slice(&created.root_serial.to_be_bytes());
    wire.extend_from_slice(&1u32.to_be_bytes());
    wire.extend_from_slice(&1u16.to_be_bytes());
    wire.push(b'a');
    wire.extend_from_slice(&0u64.to_be_bytes());
    let endpoint = server.listen_with_purposes().unwrap();
    assert_eq!(
        server.listen().unwrap_err().code,
        Code::Ownership,
        "no topology adoption"
    );
    let mut catalog = server.connect_purpose(Purpose::Catalog).unwrap();
    let mut control = server.connect_purpose(Purpose::Control).unwrap();
    let mut general = Vec::new();
    for _ in 0..4 {
        general.push(server.connect_purpose(Purpose::General).unwrap());
    }
    std::thread::scope(|threads| {
        let mut releases = Vec::new();
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
            releases.push(release);
            let body = wire.clone();
            writers.push(threads.spawn(move || {
                client.call(
                    &request(
                        1,
                        Operation::HistoryCommand(HistoryCommand::StageChanges(header)),
                    ),
                    &mut Gate {
                        body: io::Cursor::new(body),
                        ready,
                        release: held,
                        first: true,
                    },
                    &mut io::sink(),
                )
            }));
            arrived.recv_timeout(Duration::from_secs(2)).unwrap();
            // Client Source readiness precedes native construction. Admit the next
            // owner only after this actual persisted Save reached its body barrier.
            let end = Instant::now() + Duration::from_secs(2);
            loop {
                let slots = active(&temp.0.join("store.sqlite"));
                if slots == i as u64 + 1 {
                    break;
                }
                assert!(
                    Instant::now() < end,
                    "native Save {i} barrier: active={slots}"
                );
                std::thread::yield_now();
            }
        }
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            if active(&temp.0.join("store.sqlite")) == 2 {
                break;
            }
            assert!(
                Instant::now() < end,
                "two actual persisted Saves not reached"
            );
            std::thread::yield_now();
        }
        let native = observe::scratch_files(&temp.0);
        assert_eq!(
            native.len(),
            2,
            "two actual pre-body metadata files in one authority directory"
        );
        assert!(
            server.connect_purpose(Purpose::General).is_err(),
            "General cannot take protected slots"
        );
        assert!(
            server.connect_purpose(Purpose::Catalog).is_err(),
            "catalog class has one owner through terminal"
        );
        let hostile = TcpStream::connect(endpoint).unwrap(); // Actual accepted preauthentication owner, no valid selector/HELLO.
        let before = active(&temp.0.join("store.sqlite"));
        let Response::History(reservation) = catalog
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
            .unwrap()
        else {
            panic!("native reservation")
        };
        assert!(
            matches!(*reservation,HistoryResult::Reservation{scope,count:2,..} if scope==fork.scope)
        );
        assert_eq!(before, 2);
        assert_eq!(
            active(&temp.0.join("store.sqlite")),
            2,
            "refill completes before either held source resumes"
        );
        assert_eq!(
            control
                .call(
                    &request(1, Operation::FileSaveCapabilities),
                    &mut &[][..],
                    &mut io::sink()
                )
                .unwrap(),
            Response::FileSaveCapabilities {
                version: SAVE_FILE_V2_VERSION
            }
        );
        assert_eq!(
            catalog
                .call(
                    &request(2, Operation::FileSaveCapabilities),
                    &mut &[][..],
                    &mut io::sink()
                )
                .unwrap_err()
                .code,
            Code::Denied
        );
        drop(hostile);
        // Refill was observed with both Saves held. Resume each distinct owner
        // through its known response before the next Store SQL-wave attempt;
        // no transaction is retried after a concurrent ownership refusal.
        for (release, writer) in releases.into_iter().zip(writers) {
            release.send(()).unwrap();
            assert!(matches!(
                writer.join().unwrap().unwrap(),
                Response::History(_)
            ));
        }
    });
    assert_eq!(active(&temp.0.join("store.sqlite")), 0);
    assert_eq!(
        observe::scratch_files(&temp.0).len(),
        2,
        "known scratch owners remaincharged idle before independentshutdown"
    );
    drop(general);
    drop(catalog);
    drop(control);
    server.shutdown();
    assert!(
        observe::scratch_files(&temp.0).is_empty(),
        "explicitidle drain closes/removes eachknownfile once"
    );
    // Two extra General sessions are persistent here, not a proof of two occupied
    // ordinary read byte owners. Native strict32MiB/read/physical gates remain open.
}
#[test]
fn selected_native_control_has_closed_surface_and_default_v1_listener_stays_unprotected() {
    let (_temp, server) = fixture("topology");
    server.listen().unwrap();
    assert!(server.connect_purpose(Purpose::Catalog).is_err());
    assert_eq!(
        server.listen_with_purposes().unwrap_err().code,
        Code::Ownership
    );
    let mut general = server.connect_purpose(Purpose::General).unwrap();
    general
        .call(
            &request(1, Operation::FileSaveCapabilities),
            &mut &[][..],
            &mut io::sink(),
        )
        .unwrap();
    drop(general);
    server.shutdown();
}
