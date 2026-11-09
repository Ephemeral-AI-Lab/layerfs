//! #547 regression coverage for the real daemon's control-slot custody.
//!
//! #543 deleted the inline tests that proved #540: connections whose received
//! record the daemon cannot decode must not remove admission capacity, no
//! vacant slot must answer with the advisory capacity notice instead of a
//! bare EOF, and a failure storm must neither deadlock nor leak a slot.
//!
//! Each test starts the package's own `layerfs-daemon` executable with a
//! protected configuration record and talks to it over the authenticated
//! Noise channel exactly as a client does. No product `Service` is composed
//! inside the test process, and no test-only product API is used.

#![cfg(any(target_os = "linux", target_os = "macos"))]

#[allow(dead_code)]
#[path = "support/host_install.rs"]
mod host;

use layerfs_bridge::{
    control::{
        Answer, Call, CapacityModel, ControlCode, DaemonPhase, HelloRequest, Reply, Request,
        SdkCall, SdkValue, OPERATION, RECLAIM,
    },
    daemon_setup::{DaemonLimits, DaemonSetup},
    duplex::{self, ChannelError, Connection},
    DaemonStore, KernelRoute,
};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    net::{SocketAddr, TcpStream},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const CLIENT_PRIVATE: [u8; 32] = [71; 32];
const SERVER_PRIVATE: [u8; 32] = [72; 32];
const CONNECTIONS: u16 = 8;
/// Deaths past every slot (the pool plus the reserved one), so a daemon that
/// retains a failed slot runs out of admission capacity.
const ROUNDS: usize = CONNECTIONS as usize + 4;
/// A record that is received but fails `InitialRecord::decode`, so the worker
/// retains the exact failure. It is not a decoded request: the daemon executes
/// nothing for it.
const DIE: &[u8] = b"client-died-before-reading-reply";

/// One real daemon executable and the directory holding its protected files.
struct Daemon {
    child: Child,
    address: SocketAddr,
    stderr: PathBuf,
    directory: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn private_directory(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

/// Starts the actual executable and returns it once it reports its listener.
fn start(label: &str, handshake_ms: u32, idle_ttl_ms: u32, request_deadline_ms: u32) -> Daemon {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "layerfs-547-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&directory);
    private_directory(&directory);
    let mounts = directory.join("mounts");
    private_directory(&mounts);
    let identity = fs::metadata(&directory).unwrap();
    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: SERVER_PRIVATE,
        control_peer: duplex::public_key(&CLIENT_PRIVATE).unwrap(),
        // The native NFS route needs no FUSE device, on either platform.
        route: KernelRoute::Nfs,
        overlay: directory.join("overlay.sqlite").display().to_string(),
        mounts: mounts.display().to_string(),
        command_uid: identity.uid(),
        command_gid: identity.gid(),
        limits: DaemonLimits {
            connections: CONNECTIONS,
            handshake_ms,
            idle_ttl_ms,
            request_deadline_ms,
            cache_bytes: 1024 * 1024,
            namespaces: 16,
            pager_kib: 1024,
            serial_low_water: 256,
        },
        // No installed Store: the daemon serves startup control only, which
        // is enough to prove admission custody and needs no Store service.
        store: DaemonStore::Sqlite {
            locator: directory.join("store.sqlite").display().to_string(),
            install: None,
        },
    };
    let config = directory.join("daemon.config");
    fs::write(&config, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let stderr = directory.join("daemon.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"))
        .arg("--config")
        .arg(&config)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(File::create(&stderr).unwrap()))
        .spawn()
        .expect("spawn layerfs-daemon");
    let mut child = Daemon {
        child,
        address: "127.0.0.1:0".parse().unwrap(),
        stderr,
        directory,
    };
    let stdout = child.child.stdout.take().unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = send.send(result);
    });
    let line = match receive.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(line)) => line,
        _ => {
            let output = fs::read_to_string(&child.stderr).unwrap_or_default();
            panic!("daemon never reported a listener; stderr:\n{output}");
        }
    };
    reader.join().unwrap();
    child.address = line
        .strip_prefix("LAYERFS_DAEMON_LISTEN ")
        .unwrap_or_else(|| panic!("unexpected daemon startup line: {line:?}"))
        .trim()
        .parse()
        .unwrap();
    child
}

fn connect(address: SocketAddr) -> Connection {
    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    duplex::initiate(
        stream,
        &CLIENT_PRIVATE,
        duplex::public_key(&SERVER_PRIVATE).unwrap(),
    )
    .unwrap()
}

/// One authenticated client that reaches its Hello reply and then holds the
/// connection idle; the worker stays blocked in `receive()` and keeps its slot.
fn hello(address: SocketAddr) -> Connection {
    let mut connection = connect(address);
    let call = Call {
        id: 1,
        request: Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: false,
        }),
    };
    connection.send.send(&call.encode().unwrap()).unwrap();
    match Answer::decode(connection.receive.receive().unwrap())
        .unwrap()
        .reply
    {
        Reply::Hello(status) => assert_eq!(status.phase, DaemonPhase::InstallPending),
        other => panic!("unexpected Hello reply: {other:?}"),
    }
    connection
}

/// The daemon's own control-capacity read model.
fn capacity(address: SocketAddr) -> CapacityModel {
    let mut connection = connect(address);
    let call = Call {
        id: 7,
        request: Request::Sdk(SdkCall {
            method: OPERATION.to_owned(),
            args: SdkValue::Null,
        }),
    };
    connection.send.send(&call.encode().unwrap()).unwrap();
    match Answer::decode(connection.receive.receive().unwrap())
        .unwrap()
        .reply
    {
        Reply::Sdk(value) => CapacityModel::parse(&value),
        other => panic!("capacity query was not answered: {other:?}"),
    }
}

fn wait_for_log(path: &Path, needle: &str, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = fs::read_to_string(path).unwrap_or_default();
        let found = text.lines().filter(|line| line.starts_with(needle)).count();
        if found >= count {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {count} {needle:?} line(s) in {}; saw {found}\n{text}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

/// Clients send a record the daemon cannot decode and then disappear. Every
/// retained failure must free its slot: a fresh authenticated client is
/// admitted, the pool is whole, and one log line names each failure (#540).
#[test]
fn undecodable_records_do_not_consume_control_slots() {
    let daemon = start("slots", 2000, 60_000, 60_000);
    for round in 0..ROUNDS {
        let mut connection = connect(daemon.address);
        connection.send.send(DIE).unwrap();
        // The daemon records the failure and closes its end; wait for that
        // close so the failure is retained rather than only seen as EOF.
        assert!(
            connection.receive.receive().is_err(),
            "round {round}: the daemon kept the connection after a recorded failure"
        );
        drop(connection);
    }

    // A fresh authenticated client is admitted and reaches its Hello reply
    // after every failed worker released its slot.
    drop(hello(daemon.address));

    let model = capacity(daemon.address);
    // The query connection is the only worker running at this observation.
    assert_eq!(model.admission.pool_running, 1);
    assert_eq!(model.admission.pool_vacant, i64::from(CONNECTIONS) - 1);
    assert_eq!(
        model.admission.pool_vacant + model.admission.pool_running,
        i64::from(CONNECTIONS)
    );
    assert_eq!(model.retained.limit, 64);
    assert_eq!(model.retained.recorded, ROUNDS as i64);
    assert_eq!(model.retained.count, ROUNDS as i64);
    assert_eq!(model.retained.dropped, 0);
    for record in &model.retained.failures {
        assert_eq!(record.class, "pool");
        assert!(record.generation > 0);
        assert!(
            record.owner.contains("layerfs-control-"),
            "{}",
            record.owner
        );
        assert_eq!(record.received_bytes, Some(DIE.len() as i64));
        assert_eq!(record.request, None);
        assert!(!record.reply_sent);
        assert!(record.cause.contains("kernel request"), "{}", record.cause);
    }

    wait_for_log(&daemon.stderr, "ConnectionFailure {", ROUNDS);
}

/// With every pool slot held and the reserved slot occupied by a connection
/// that never completes its handshake, the next caller gets one 24-byte
/// `LAYERFS-CAPACITY` body instead of a bare EOF (#540).
#[test]
fn no_vacant_slot_writes_the_capacity_notice() {
    let daemon = start("notice", 10_000, 60_000, 60_000);
    let mut holders = Vec::new();
    for _ in 0..CONNECTIONS {
        holders.push(hello(daemon.address));
    }
    // Take the reserved slot with a raw connection that never speaks: its
    // worker blocks in the Noise handshake and keeps the last slot.
    let reserved = TcpStream::connect_timeout(&daemon.address, Duration::from_secs(2)).unwrap();
    reserved
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    thread::sleep(Duration::from_millis(300));
    // This one must be refused before any handshake.
    let mut probe = TcpStream::connect_timeout(&daemon.address, Duration::from_secs(2)).unwrap();
    probe
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();

    let mut framed = [0u8; 26];
    probe.read_exact(&mut framed).unwrap();
    assert_eq!(
        u16::from_be_bytes([framed[0], framed[1]]) as usize,
        layerfs_bridge::duplex::admission::BODY_LEN
    );
    assert_eq!(&framed[2..2 + 16], layerfs_bridge::duplex::admission::MAGIC);
    let notice = layerfs_bridge::duplex::admission::classify(&framed[2..])
        .expect("the body is not a capacity notice");
    // No Store is installed, so the daemon is still starting.
    assert_eq!(
        notice.reason,
        layerfs_bridge::duplex::admission::AdmissionReason::Starting
    );
    assert_eq!(notice.pool_vacant, 0);
    assert_eq!(notice.reserved_vacant, 0);

    drop(probe);
    drop(reserved);
    holders.clear();
}

/// Sixteen threads each die repeatedly. The accept loop must not deadlock,
/// every worker must finish, the ring stays bounded and exact, and a fresh
/// authenticated client is still admitted afterwards.
#[test]
fn concurrent_failures_do_not_deadlock_or_leak() {
    let daemon = start("concurrent", 3000, 60_000, 60_000);
    let address = daemon.address;
    let mut threads = Vec::new();
    for _ in 0..16 {
        threads.push(thread::spawn(move || {
            for _ in 0..4 {
                loop {
                    let Ok(stream) = TcpStream::connect_timeout(&address, Duration::from_secs(2))
                    else {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    match duplex::initiate(
                        stream,
                        &CLIENT_PRIVATE,
                        duplex::public_key(&SERVER_PRIVATE).unwrap(),
                    ) {
                        Ok(mut connection) => {
                            connection.send.send(DIE).unwrap();
                            let _ = connection.receive.receive();
                            break;
                        }
                        Err(ChannelError::Admission(_)) | Err(ChannelError::Io(_)) => {
                            // Full pool or a connection lost in the storm:
                            // neither attempted anything, so retry is safe.
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("unexpected open failure: {error:?}"),
                    }
                }
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }

    let model = capacity(address);
    assert_eq!(model.admission.pool_running, 1);
    assert_eq!(model.admission.pool_vacant, i64::from(CONNECTIONS) - 1);
    assert!(model.retained.count >= 1, "{model:?}");
    assert!(model.retained.count <= model.retained.limit);
    assert_eq!(
        model.retained.count + model.retained.dropped,
        model.retained.recorded
    );

    // A fresh authenticated client is admitted after the storm.
    drop(hello(address));
    wait_for_log(&daemon.stderr, "ConnectionFailure {", 1);
}

/// Eight idle clients hold every pool slot. Once they have been parked in
/// `receive()` past `idle_ttl`, a ninth client is admitted by idle
/// reclamation instead of being refused (#547 slice 4). The ninth reaches its
/// Hello reply, which it could not do on the installer-only reserved slot.
#[test]
fn idle_clients_are_reclaimed_for_a_new_connection() {
    const IDLE_TTL_MS: u32 = 300;
    let daemon = start("idle", 60_000, IDLE_TTL_MS, 60_000);
    let holders: Vec<Connection> = (0..CONNECTIONS).map(|_| hello(daemon.address)).collect();
    // Let every idle worker pass the TTL so the next admission reclaims one.
    thread::sleep(Duration::from_millis(u64::from(IDLE_TTL_MS) + 400));
    // A ninth client is admitted and reaches its Hello reply: the accept loop
    // reclaimed an idle pool slot under pressure.
    drop(hello(daemon.address));

    // The reclamation freed the whole idle pool: only the capacity query's
    // own connection is running at this observation.
    let model = capacity(daemon.address);
    assert_eq!(model.admission.pool_running, 1);
    assert_eq!(model.admission.pool_vacant, i64::from(CONNECTIONS) - 1);

    drop(holders);
}

/// An authenticated client that reaches its Hello reply on a ControlReady
/// daemon and then holds the connection idle.
fn hello_ready(address: SocketAddr) -> Connection {
    let mut connection = connect(address);
    let call = Call {
        id: 1,
        request: Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: false,
        }),
    };
    connection.send.send(&call.encode().unwrap()).unwrap();
    match Answer::decode(connection.receive.receive().unwrap())
        .unwrap()
        .reply
    {
        Reply::Hello(status) => assert_eq!(status.phase, DaemonPhase::ControlReady),
        other => panic!("unexpected Hello reply: {other:?}"),
    }
    connection
}

/// Starts the real executable with an installed Store, so it reports
/// ControlReady: the phase where the reserved slot is the recovery class.
fn start_ready(label: &str) -> Daemon {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "layerfs-547-ready-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&directory);
    private_directory(&directory);
    let mounts = directory.join("mounts");
    private_directory(&mounts);
    let identity = fs::metadata(&directory).unwrap();

    // Build an installed Store at this daemon's own path. Init only seals;
    // the install step is a copy to the manifest's locator. Then set the
    // post-install metadata the daemon's protected-config gate expects.
    let store_path = directory.join("store.sqlite");
    let fixture = host::Fixture::new(label, Some(store_path.to_str().unwrap()));
    fs::copy(&fixture.project.store.path, &store_path).unwrap();
    let mut manifest = fixture.project.manifest.clone();
    manifest.daemon_sqlite = Some("3.50.4".to_owned());
    fixture.cleanup();
    fs::set_permissions(&store_path, fs::Permissions::from_mode(0o600)).unwrap();

    let setup = DaemonSetup {
        listen: "127.0.0.1:0".into(),
        private_key: SERVER_PRIVATE,
        control_peer: duplex::public_key(&CLIENT_PRIVATE).unwrap(),
        route: KernelRoute::Nfs,
        overlay: directory.join("overlay.sqlite").display().to_string(),
        mounts: mounts.display().to_string(),
        command_uid: identity.uid(),
        command_gid: identity.gid(),
        limits: DaemonLimits {
            connections: CONNECTIONS,
            handshake_ms: 60_000,
            idle_ttl_ms: 60_000,
            request_deadline_ms: 60_000,
            cache_bytes: 1024 * 1024,
            namespaces: 16,
            pager_kib: 1024,
            serial_low_water: 256,
        },
        store: DaemonStore::Sqlite {
            locator: manifest.locator.clone(),
            install: Some(manifest),
        },
    };
    let config = directory.join("daemon.config");
    fs::write(&config, setup.encode().unwrap()).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
    let stderr = directory.join("daemon.stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_layerfs-daemon"))
        .arg("--config")
        .arg(&config)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(File::create(&stderr).unwrap()))
        .spawn()
        .expect("spawn layerfs-daemon");
    let mut child = Daemon {
        child,
        address: "127.0.0.1:0".parse().unwrap(),
        stderr,
        directory,
    };
    let stdout = child.child.stdout.take().unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = send.send(result);
    });
    let line = match receive.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(line)) => line,
        _ => {
            let output = fs::read_to_string(&child.stderr).unwrap_or_default();
            panic!("daemon never reported a listener; stderr:\n{output}");
        }
    };
    reader.join().unwrap();
    child.address = line
        .strip_prefix("LAYERFS_DAEMON_LISTEN ")
        .unwrap_or_else(|| panic!("unexpected daemon startup line: {line:?}"))
        .trim()
        .parse()
        .unwrap();
    child
}

/// Once the daemon is ControlReady, a full pool admits one connection into
/// the reserved slot (recovery class): it serves the recovery set and refuses
/// anything else with `Capacity` and a close (#547 slice 3).
#[test]
fn reserved_slot_serves_recovery_and_refuses_everything_else() {
    let daemon = start_ready("recovery");
    // The daemon opened its Store and reports ControlReady.
    drop(hello_ready(daemon.address));

    // Hold every pool slot idle; `idle_ttl` is large, so no reclamation runs.
    let holders: Vec<Connection> = (0..CONNECTIONS)
        .map(|_| hello_ready(daemon.address))
        .collect();

    // A ninth connection is admitted to the reserved slot and serves one
    // recovery operation: the operator read model.
    {
        let mut recovery = connect(daemon.address);
        recovery
            .send
            .send(
                &Call {
                    id: 1,
                    request: Request::Hello(HelloRequest {
                        expected_instance: None,
                        wait_for_store: false,
                    }),
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
        assert!(matches!(
            Answer::decode(recovery.receive.receive().unwrap())
                .unwrap()
                .reply,
            Reply::Hello(_)
        ));
        recovery
            .send
            .send(
                &Call {
                    id: 2,
                    request: Request::Sdk(SdkCall {
                        method: OPERATION.to_owned(),
                        args: SdkValue::Null,
                    }),
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
        match Answer::decode(recovery.receive.receive().unwrap())
            .unwrap()
            .reply
        {
            Reply::Sdk(value) => {
                let model = CapacityModel::parse(&value);
                assert_eq!(model.admission.pool_running, i64::from(CONNECTIONS));
            }
            other => panic!("recovery operation was not served: {other:?}"),
        }
    }

    // A tenth connection is also admitted to the reserved slot, but a
    // non-recovery request is refused `Capacity` and the connection closes.
    {
        let mut refused = connect(daemon.address);
        refused
            .send
            .send(
                &Call {
                    id: 1,
                    request: Request::Hello(HelloRequest {
                        expected_instance: None,
                        wait_for_store: false,
                    }),
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
        assert!(matches!(
            Answer::decode(refused.receive.receive().unwrap())
                .unwrap()
                .reply,
            Reply::Hello(_)
        ));
        refused
            .send
            .send(
                &Call {
                    id: 2,
                    request: Request::Sdk(SdkCall {
                        method: "sandbox.list_workspaces".to_owned(),
                        args: SdkValue::Null,
                    }),
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
        match Answer::decode(refused.receive.receive().unwrap())
            .unwrap()
            .reply
        {
            Reply::Refused(refusal) => {
                assert_eq!(refusal.code, layerfs_bridge::control::ControlCode::Capacity)
            }
            other => panic!("non-recovery request was not refused: {other:?}"),
        }
    }

    drop(holders);
}

/// A worker whose decoded request passes `request_deadline` is expired: its
/// slot is freed, its socket is closed (so the caller observes the loss), and
/// one `Expired` retained failure is recorded (#547 slice 5). The interrupted
/// operation's effect is unknown, so it is never released as clean.
#[test]
fn in_flight_expiry_records_expired_and_closes_the_socket() {
    const DEADLINE_MS: u32 = 300;
    let daemon = start("expiry", 60_000, 60_000, DEADLINE_MS);
    // Park one worker in-flight: a Hello that waits for a Store that never
    // installs blocks its worker in `hello()` past the request deadline.
    let mut parked = connect(daemon.address);
    parked
        .send
        .send(
            &Call {
                id: 1,
                request: Request::Hello(HelloRequest {
                    expected_instance: None,
                    wait_for_store: true,
                }),
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
    thread::sleep(Duration::from_millis(u64::from(DEADLINE_MS) + 400));

    // The capacity query's admission sweeps the expired slot first, so the
    // read model already carries the Expired record.
    let model = capacity(daemon.address);

    // The parked worker's socket was closed by expiry: the caller observes
    // EOF, never a silent hang.
    assert!(parked.receive.receive().is_err());

    let expired = model
        .retained
        .failures
        .iter()
        .find(|record| record.cause == "Expired")
        .expect("no Expired retained failure");
    assert_eq!(expired.class, "pool");
    assert!(!expired.reply_sent);
    assert!(expired.generation > 0);
    // The slot was freed: only the query's own connection is running now.
    assert_eq!(model.admission.pool_running, 1);
    assert_eq!(model.admission.pool_vacant, i64::from(CONNECTIONS) - 1);
}

/// `backend.daemon.reclaim { slot, generation }` closes the named worker's
/// socket, records `Reclaimed` and is one-shot: a second reclaim of the same
/// generation answers `Invalid` (#547 slice 5).
#[test]
fn reclaim_closes_the_named_worker_and_answers_invalid_on_mismatch() {
    let daemon = start("reclaim", 60_000, 60_000, 60_000);
    // The first admitted worker: slot 0, generation 1.
    let mut parked = connect(daemon.address);
    parked
        .send
        .send(
            &Call {
                id: 1,
                request: Request::Hello(HelloRequest {
                    expected_instance: None,
                    wait_for_store: true,
                }),
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
    thread::sleep(Duration::from_millis(200));

    let reclaim = |slot: i64, generation: i64| {
        let mut operator = connect(daemon.address);
        operator
            .send
            .send(
                &Call {
                    id: 9,
                    request: Request::Sdk(SdkCall {
                        method: RECLAIM.to_owned(),
                        args: SdkValue::map([
                            ("slot", SdkValue::Int(slot)),
                            ("generation", SdkValue::Int(generation)),
                        ]),
                    }),
                }
                .encode()
                .unwrap(),
            )
            .unwrap();
        Answer::decode(operator.receive.receive().unwrap())
            .unwrap()
            .reply
    };

    // The exact generation succeeds and frees the slot.
    match reclaim(0, 1) {
        Reply::Sdk(value) => {
            assert_eq!(value.get_text("outcome"), Some("reclaimed"));
            assert_eq!(value.get_int("slot"), Some(0));
            assert_eq!(value.get_int("generation"), Some(1));
        }
        other => panic!("reclaim was not answered: {other:?}"),
    }
    // The parked worker's socket was closed by the reclaim.
    assert!(parked.receive.receive().is_err());

    // One-shot: the same generation now answers Invalid.
    match reclaim(0, 1) {
        Reply::Refused(refusal) => assert_eq!(refusal.code, ControlCode::Invalid),
        other => panic!("stale reclaim was not refused: {other:?}"),
    }
    // A wrong generation on the now-reoccupied slot answers Invalid too.
    match reclaim(0, 999) {
        Reply::Refused(refusal) => assert_eq!(refusal.code, ControlCode::Invalid),
        other => panic!("mismatched reclaim was not refused: {other:?}"),
    }
}
