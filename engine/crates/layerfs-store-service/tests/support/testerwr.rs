//! Shared rig of the independent writer-reopen tests (`testerwr_*`): a
//! PostgreSQL Store, a [`FaultProxy`] to its server, a counting front gate
//! in front of the proxy (every connection attempt the service makes is
//! counted, and the gate can refuse them as a stopped server would), and a
//! Store service dialing through both. Every outcome is read from an
//! independent admin connection: `pg_locks`, `pg_stat_activity`,
//! `gc_intent`, the `commit` table and the Branch chain.
//!
//! Nothing here is product code. PostgreSQL only; NOT_RUN without
//! `LAYERFS_TEST_POSTGRES_URL`.
use super::adversary::{commit, verdict, wait_until, write, Service, TestStore, Verdict, CURSOR};
use super::fault_proxy::{proxied_url, Fault, FaultProxy};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_history::{BranchId, CommitId, HistoryError};
use layerfs_persistence::{CompactRequest, Handles};
use layerfs_store_service::store::BoundWorkspace;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
/// The lane wait (and the lost-backend termination bound) of the service.
pub const LOCK_TIMEOUT_MS: u32 = 5_000;

/// A TCP gate in front of the fault proxy: counts every connection attempt
/// and, while refusing, closes each one at once.
pub struct FrontGate {
    pub port: u16,
    attempts: Arc<AtomicU64>,
    refusing: Arc<AtomicBool>,
}

fn pump(mut from: TcpStream, mut to: TcpStream) {
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        if to.write_all(&buffer[..read]).is_err() {
            break;
        }
    }
    let _ = from.shutdown(Shutdown::Both);
    let _ = to.shutdown(Shutdown::Both);
}

impl FrontGate {
    pub fn start(upstream: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let attempts: Arc<AtomicU64> = Arc::default();
        let refusing: Arc<AtomicBool> = Arc::default();
        let (counted, refused) = (attempts.clone(), refusing.clone());
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(client) = client else { return };
                counted.fetch_add(1, Ordering::SeqCst);
                if refused.load(Ordering::SeqCst) {
                    let _ = client.shutdown(Shutdown::Both);
                    continue;
                }
                let Ok(server) = TcpStream::connect(("127.0.0.1", upstream)) else {
                    let _ = client.shutdown(Shutdown::Both);
                    continue;
                };
                let _ = client.set_nodelay(true);
                let _ = server.set_nodelay(true);
                let (c2, s2) = (client.try_clone().unwrap(), server.try_clone().unwrap());
                std::thread::spawn(move || pump(client, server));
                std::thread::spawn(move || pump(s2, c2));
            }
        });
        Self {
            port,
            attempts,
            refusing,
        }
    }

    /// Connection attempts so far (refused ones included).
    pub fn attempts(&self) -> u64 {
        self.attempts.load(Ordering::SeqCst)
    }

    pub fn refuse(&self, down: bool) {
        self.refusing.store(down, Ordering::SeqCst);
    }
}

pub struct Rig {
    pub store: TestStore,
    pub proxy: FaultProxy,
    pub gate: FrontGate,
    pub service: Service,
    /// The backend of the service writer's first connection.
    pub writer: i32,
}

pub fn rig(label: &str) -> Option<Rig> {
    let store = TestStore::postgres(label, SEED)?;
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = FaultProxy::start(&url);
    let gate = FrontGate::start(proxy.port);
    let (gated, _) = proxied_url(&url, gate.port);
    let selection = store.with_lock_timeout(&store.selection_via(&gated), LOCK_TIMEOUT_MS);
    let service = Service::open(&store, selection);
    let writer = rig_writer(&proxy);
    Some(Rig {
        store,
        proxy,
        gate,
        service,
        writer,
    })
}

/// Connections the service writer opened so far (its open's included).
pub fn writer_connects(service: &Service) -> u64 {
    service
        .service
        .opened
        .session_diagnostics(None)
        .unwrap()
        .unwrap()
        .connects
}

/// The Store's generation (`catalog_incarnation`).
pub fn generation(store: &TestStore) -> i64 {
    store
        .admin()
        .query_one(
            "SELECT catalog_incarnation FROM history_meta WHERE id = 1",
            &[],
        )
        .unwrap()
        .get(0)
}

/// The service's first connection is its writer.
fn rig_writer(proxy: &FaultProxy) -> i32 {
    proxy.pids()[0].expect("the writer's backend")
}

/// The service writer's reopens so far.
pub fn reopens(service: &Service) -> u64 {
    service
        .service
        .opened
        .session_diagnostics(None)
        .unwrap()
        .unwrap()
        .reopens
}

pub fn alive(store: &TestStore, pid: i32) -> bool {
    store
        .admin()
        .query_opt("SELECT 1 FROM pg_stat_activity WHERE pid = $1", &[&pid])
        .unwrap()
        .is_some()
}

/// Terminates `pid` and waits (bounded) until it is gone.
pub fn kill(store: &TestStore, pid: i32) {
    let ended: bool = store
        .admin()
        .query_one("SELECT pg_terminate_backend($1, 10000)", &[&pid])
        .unwrap()
        .get(0);
    assert!(ended, "backend {pid} ended");
}

/// Live proxied backends.
pub fn live_proxied(rig: &Rig) -> Vec<i32> {
    let pids: Vec<i32> = rig.proxy.pids().into_iter().flatten().collect();
    rig.store
        .admin()
        .query(
            "SELECT pid FROM pg_stat_activity WHERE pid = ANY($1) ORDER BY pid",
            &[&pids],
        )
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

/// Session advisory keys per proxied backend, right now.
pub fn key_holders_now(rig: &Rig) -> Vec<(i32, i64)> {
    let pids: Vec<i32> = rig.proxy.pids().into_iter().flatten().collect();
    rig.store
        .admin()
        .query(
            "SELECT pid, count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 1 \
             AND granted AND pid = ANY($1) GROUP BY pid ORDER BY pid",
            &[&pids],
        )
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect()
}

/// The session advisory keys backend `pid` holds, as `bigint` keys.
pub fn keys_of(store: &TestStore, pid: i32) -> Vec<i64> {
    store
        .admin()
        .query(
            "SELECT ((classid::bigint << 32) | objid::bigint) FROM pg_locks \
              WHERE locktype = 'advisory' AND objsubid = 1 AND granted AND pid = $1 \
              ORDER BY 1",
            &[&pid],
        )
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect()
}

/// Per backend holding any of `keys` (whoever it is, proxied or not: a
/// connection whose startup a `Pause` held is never parsed by the proxy),
/// how many of `keys` it holds.
pub fn holders_of(store: &TestStore, keys: &[i64]) -> Vec<(i32, i64)> {
    store
        .admin()
        .query(
            "SELECT pid, count(*) FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 1 \
             AND granted AND ((classid::bigint << 32) | objid::bigint) = ANY($1) \
             GROUP BY pid ORDER BY pid",
            &[&keys],
        )
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect()
}

/// Key holders once at most one proxied backend holds keys (waiting up to
/// 10 s for closed connections' backends to exit).
pub fn key_holders(rig: &Rig) -> Vec<(i32, i64)> {
    let mut last = Vec::new();
    wait_until(Duration::from_secs(10), || {
        last = key_holders_now(rig);
        last.len() <= 1
    });
    last
}

/// The writer's backend: the one proxied backend holding session keys.
pub fn writer_pid(rig: &Rig) -> i32 {
    let holders = key_holders(rig);
    assert_eq!(holders.len(), 1, "one key holder: {holders:?}");
    holders[0].0
}

/// (live, uncertain) intent rows of Branch `branch`.
pub fn rows(store: &TestStore, branch: BranchId) -> (i64, i64) {
    let row = store
        .admin()
        .query_one(
            "SELECT count(*) FILTER (WHERE state = 'live'), \
                    count(*) FILTER (WHERE state = 'uncertain') \
               FROM gc_intent WHERE scope_kind = 'branch' AND scope_id = $1",
            &[&branch.as_slice()],
        )
        .unwrap();
    (row.get(0), row.get(1))
}

/// Every non-done intent row of the Store.
pub fn all_rows(store: &TestStore) -> i64 {
    store
        .admin()
        .query_one("SELECT count(*) FROM gc_intent WHERE state <> 'done'", &[])
        .unwrap()
        .get(0)
}

static COMPACT_OP: AtomicU64 = AtomicU64::new(1);

/// One compaction on a session of its own: "Ok" or the refusal's class.
pub fn compact(store: &TestStore) -> String {
    let n = COMPACT_OP.fetch_add(1, Ordering::SeqCst);
    let mut op = [0xc0u8; 16];
    op[..8].copy_from_slice(&n.to_le_bytes());
    let request = CompactRequest {
        op_id: op,
        ..CompactRequest::default()
    };
    let Handles::Postgres(handles) =
        Handles::open_writable(&store.selection, &store.binding, CURSOR).unwrap()
    else {
        unreachable!()
    };
    let result = handles.compact(&request);
    handles.close();
    match result {
        Ok(_) => "Ok".into(),
        Err(HistoryError::MountsActive(_)) => "MountsActive".into(),
        Err(other) => format!("{other:?}"),
    }
}

/// `branch.remove` of `name` through a fresh direct service: "Ok" or the
/// refusal's code.
pub fn remove(store: &TestStore, name: &str) -> String {
    let service = store.open();
    let answer = service.call("branch.remove", vec![("branch", SdkValue::text(name))]);
    service.stop();
    match answer {
        Ok(_) => "Ok".into(),
        Err(refusal) => refusal.code,
    }
}

pub fn committed(workspace: &BoundWorkspace, what: &str) -> CommitId {
    match verdict(&commit(workspace)) {
        Verdict::Committed(id) => id,
        other => panic!("{what}: {other:?}"),
    }
}

/// Commit rows the Store holds.
pub fn commit_rows(store: &TestStore) -> i64 {
    store
        .admin()
        .query_one("SELECT count(*) FROM \"commit\"", &[])
        .unwrap()
        .get(0)
}

/// The server messages of a repeated mount on the writer (its snapshot
/// read and admission message), counted on a twin Branch after the
/// service prefilled its reader and mounted the twin once.
pub fn mount_flight(rig: &Rig, twin: BranchId) -> u64 {
    if let Some(prefilled) = rig.service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    let warm = rig.service.bind(twin);
    rig.service.unmount(warm);
    if let Some(prefilled) = rig.service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    rig.proxy.arm(None);
    let counted = rig.service.bind(twin);
    let window = rig.proxy.disarm();
    rig.service.unmount(counted);
    let connections: std::collections::BTreeSet<usize> =
        window.log.iter().map(|(c, _)| *c).collect();
    assert_eq!(
        connections.len(),
        1,
        "only the writer answers a repeated mount"
    );
    window.seen
}

/// A mount of `branch` whose reply is lost after its intent's `COMMIT`
/// (the last server message of the repeated mount's flight `k`): the row
/// committed, the writer is quarantined, the mount never returned.
/// A mount and unmount of `twin` first leave an idle reader in the pool, so
/// the lost mount's flight is the writer's alone.
pub fn lose_mount(rig: &Rig, twin: BranchId, branch: BranchId, k: u64) {
    let warm = rig.service.bind(twin);
    rig.service.unmount(warm);
    rig.proxy.arm(Some((k - 1, Fault::Drop)));
    let lost = rig.service.try_bind(branch);
    let window = rig.proxy.disarm();
    let connections: std::collections::BTreeSet<usize> =
        window.log.iter().map(|(c, _)| *c).collect();
    assert_eq!(
        connections.len(),
        1,
        "only the writer answers the lost mount"
    );
    assert!(window.fired.is_some(), "the mount fault fired");
    let error = lost.err().expect("the mount's reply was lost");
    assert!(error.contains("UnknownOutcome"), "unknown: {error}");
}

/// The server messages of one warm Commit on a twin Branch (the twin's
/// third Commit, once its statements are prepared on the writer).
pub fn commit_flight(rig: &Rig, twin: BranchId) -> u64 {
    let workspace = rig.service.bind(twin);
    write(&workspace, "/warm", b"warm\n");
    committed(&workspace, "twin warm-up");
    write(&workspace, "/one", b"one\n");
    committed(&workspace, "twin first warm Commit");
    write(&workspace, "/fault", b"fault\n");
    rig.proxy.arm(None);
    committed(&workspace, "twin Commit");
    let window = rig.proxy.disarm();
    rig.service.unmount(workspace);
    let kinds: String = window.log.iter().map(|(_, kind)| *kind).collect();
    assert!(kinds.ends_with("CZ"), "the flight: {kinds}");
    window.seen
}

/// Loses the reply of a warm Commit of `path` after the server committed:
/// a `Drop` at the last message of the flight two unarmed Commits before
/// it measured. A Commit whose flight turns out shorter completes (no fault
/// fires) and the next path is tried, at most five times. Returns the head
/// before the lost Commit and the flight length that fired.
pub fn lose_commit(
    rig: &Rig,
    workspace: &BoundWorkspace,
    branch: layerfs_history::BranchId,
    path: &str,
) -> (CommitId, u64) {
    for attempt in 0..5 {
        // The flights of two more unarmed Commits of the same shape, on the
        // same connections: steady Commits alternate between two flights
        // (an ordinal reservation every other one), so the first of the two
        // has the lost one's.
        let mut flights = Vec::new();
        for n in 0..2 {
            let counted = format!("{path}-count{attempt}{n}");
            write(workspace, &counted, counted.as_bytes());
            rig.proxy.arm(None);
            committed(workspace, &counted);
            flights.push(rig.proxy.disarm().seen);
        }
        let k = flights[0];
        let old = rig.store.chain(branch)[0].id;
        let path = format!("{path}-{attempt}");
        write(workspace, &path, path.as_bytes());
        rig.proxy.arm(Some((k - 1, Fault::Drop)));
        let lost = verdict(&commit(workspace));
        let window = rig.proxy.disarm();
        if window.fired.is_none() {
            assert!(matches!(lost, Verdict::Committed(_)), "{path}: {lost:?}");
            println!(
                "TESTERWR_LOSE_MISSED {path} counted {flights:?}, ran {}",
                window.seen
            );
            continue;
        }
        assert!(
            matches!(lost, Verdict::Unknown(_)),
            "{path}: {lost:?} at {:?}: {:?}",
            window.fired,
            window.log
        );
        assert!(workspace.has_retained());
        return (old, k);
    }
    panic!("{path}: no Commit reply was lost");
}

/// Exits the test process when not disarmed within `limit`: a deadlock
/// fails the test at a bound instead of hanging it.
pub struct Watchdog(Arc<AtomicBool>);

impl Watchdog {
    pub fn arm(label: &'static str, limit: Duration) -> Self {
        let done: Arc<AtomicBool> = Arc::default();
        let flag = done.clone();
        std::thread::spawn(move || {
            let until = std::time::Instant::now() + limit;
            while std::time::Instant::now() < until {
                if flag.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            eprintln!("WATCHDOG: {label} did not finish within {limit:?}: deadlock");
            std::process::exit(101);
        });
        Self(done)
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
