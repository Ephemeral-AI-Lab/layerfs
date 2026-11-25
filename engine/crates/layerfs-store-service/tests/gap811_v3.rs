//! #811 V3: definite answers for the PostgreSQL Store's known gaps, each
//! through the product path (`open_store_observed`, `StoreService`,
//! `BoundWorkspace::commit_captured`, the SDK method table and the
//! persistence `Handles`), against servers a driver sets up
//! (`tools/gap811_v3.py`: primary and standby, pgbouncer, two writable
//! servers). Every test reads its server URLs from the environment and
//! prints NOT_RUN without them; each writes one `GAP811 {json}` line per
//! observation (and appends it to `GAP811_OUT` when set):
//!
//! * `failover`: phases `setup` (a Store and Branch `b1`), `commit` (mount,
//!   write, wait for the driver's go file, one Commit under
//!   `GAP811_SYNC`'s `synchronous_commit`, its answer within
//!   `GAP811_BLOCK_S`); the driver kills the primary and promotes the
//!   standby and reads the head there.
//! * `multihost`: a service dialing a URL with two hosts; with
//!   `GAP811_GATE` the first host is reached through a gate that can
//!   refuse new connections, so a later dial (the lane's) goes on to the
//!   second host. Which server each connection reached is read from both.
//! * `pooled`: every writer path through `GAP811_POOLED_URL` (pgbouncer),
//!   with the live intent rows and advisory keys after each.
//! * `lane_contention`: `GAP811_WRITERS` services committing in a loop for
//!   `GAP811_SECONDS`, the server's lock waits sampled from `pg_locks`.
//! * `lost_unmount_reply`: the owner's release with its reply dropped by
//!   the fault proxy, then what refuses and what settles the rest.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;

use adversary::{commit, commit_forced, verdict, write, Service, TestStore, Verdict, CURSOR};
use layerfs_bridge::control::SdkValue;
use layerfs_history::{HistoryError, LayerId, LayerStackId};
use layerfs_persistence::{CompactRequest, Handles, StoreSelection, SynchronousCommit};
use std::io::{Read, Write};
use std::mem::ManuallyDrop;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn env_or(name: &str, default: u64) -> u64 {
    env(name).map_or(default, |v| v.parse().unwrap())
}

/// One observation: printed and appended to `GAP811_OUT`.
fn record(line: String) {
    println!("GAP811 {line}");
    if let Some(path) = env("GAP811_OUT") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        writeln!(file, "{line}").unwrap();
    }
}

fn esc(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

/// Differs between runs: each run writes new bytes.
fn nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn verdict_text(v: &Verdict) -> String {
    match v {
        Verdict::Committed(id) => format!("Committed({})", hex(id.as_slice())),
        other => format!("{other:?}"),
    }
}

/// An existing Store (made by phase `setup`) named by `GAP811_SCHEMA`,
/// `GAP811_BINDING`, `GAP811_STACK` and `GAP811_LAYER` on
/// `LAYERFS_TEST_POSTGRES_URL`; never dropped here.
fn attach() -> ManuallyDrop<TestStore> {
    let url = env("LAYERFS_TEST_POSTGRES_URL").unwrap();
    let schema = env("GAP811_SCHEMA").unwrap();
    let mut selection = layerfs_persistence::PostgresSelection::new(&url, &schema);
    selection.synchronous_commit = sync_setting();
    let dir = std::env::temp_dir().join(format!("gap811-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    ManuallyDrop::new(TestStore {
        kind: adversary::Kind::Postgres,
        binding: env("GAP811_BINDING").unwrap().into_bytes(),
        dir,
        selection: StoreSelection::Postgres(selection),
        postgres: Some((url, schema)),
        stack: LayerStackId::from_slice(&unhex(&env("GAP811_STACK").unwrap())).unwrap(),
        layer: LayerId::from_slice(&unhex(&env("GAP811_LAYER").unwrap())).unwrap(),
    })
}

fn sync_setting() -> SynchronousCommit {
    match env("GAP811_SYNC").as_deref().unwrap_or("local") {
        "off" => SynchronousCommit::Off,
        "local" => SynchronousCommit::Local,
        "remote_write" => SynchronousCommit::RemoteWrite,
        "on" => SynchronousCommit::On,
        "remote_apply" => SynchronousCommit::RemoteApply,
        other => panic!("GAP811_SYNC {other}"),
    }
}

fn with_sync(selection: &StoreSelection) -> StoreSelection {
    match selection {
        StoreSelection::Postgres(s) => {
            StoreSelection::Postgres(s.clone().with_synchronous_commit(sync_setting()))
        }
        other => other.clone(),
    }
}

/// A new Store with Branches `b1`..`b{branches}`, announced for later phases.
fn setup(label: &str, branches: usize) -> Option<ManuallyDrop<TestStore>> {
    let store = TestStore::postgres(label, &[("/seed", b"seed\n")])?;
    for n in 1..=branches {
        store.fork(&format!("b{n}"));
    }
    let (_, schema) = store.postgres.clone().unwrap();
    record(format!(
        "{{\"phase\":\"setup\",\"schema\":\"{schema}\",\"binding\":\"{}\",\"stack\":\"{}\",\"layer\":\"{}\",\"branches\":{branches}}}",
        String::from_utf8(store.binding.clone()).unwrap(),
        hex(store.stack.as_slice()),
        hex(store.layer.as_slice()),
    ));
    Some(ManuallyDrop::new(store))
}

/// Branch `name`'s head on the server `admin` reaches.
fn head(admin: &mut postgres::Client, name: &str) -> String {
    admin
        .query_opt(
            "SELECT encode(head_commit_id, 'hex') FROM branch WHERE name = $1",
            &[&name],
        )
        .unwrap()
        .and_then(|row| row.get::<_, Option<String>>(0))
        .unwrap_or_else(|| "none".into())
}

fn wait_for_file(path: &str, limit: Duration) -> bool {
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if std::path::Path::new(path).exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn failover() {
    let Some(phase) = env("GAP811_PHASE") else {
        eprintln!("NOT_RUN: GAP811_PHASE is unset");
        return;
    };
    if adversary::pg_url().is_none() {
        return;
    }
    match phase.as_str() {
        "setup" => {
            setup("fo", 2);
        }
        "commit" => {
            let store = attach();
            let service = Service::open(&store, store.selection.clone());
            let workspace = service.bind(store.branch("b1"));
            write(&workspace, "/acked", b"acknowledged on the primary\n");
            let mut admin = store.admin();
            let before = head(&mut admin, "b1");
            record(format!("{{\"phase\":\"ready\",\"head\":\"{before}\"}}"));
            let go = env("GAP811_GO").unwrap();
            assert!(wait_for_file(&go, Duration::from_secs(60)), "no go file");
            let limit = Duration::from_secs(env_or("GAP811_BLOCK_S", 20));
            let (answer, answered) = mpsc::channel();
            let started = Instant::now();
            let workspace = Arc::new(workspace);
            let committing = workspace.clone();
            std::thread::spawn(move || {
                let _ = answer.send(verdict(&commit(&committing)));
            });
            let (outcome, elapsed) = match answered.recv_timeout(limit) {
                Ok(v) => (verdict_text(&v), started.elapsed()),
                Err(_) => ("NoAnswer".to_owned(), started.elapsed()),
            };
            let after = head(&mut admin, "b1");
            record(format!(
                "{{\"phase\":\"commit\",\"sync\":\"{:?}\",\"outcome\":\"{}\",\"elapsed_ms\":{},\"limit_s\":{},\"head_before\":\"{before}\",\"head_on_primary_after\":\"{after}\"}}",
                sync_setting(),
                esc(&outcome),
                elapsed.as_millis(),
                limit.as_secs()
            ));
            // Leave without closing: a blocked Commit keeps its thread.
            std::process::exit(0);
        }
        other => panic!("phase {other}"),
    }
}

/// Client backends from this test's address on the server `admin` reaches,
/// the admin's own excluded.
fn ours(admin: &mut postgres::Client) -> i64 {
    admin
        .query_one(
            "SELECT count(*) FROM pg_stat_activity WHERE backend_type = 'client backend' \
             AND pid <> pg_backend_pid() AND client_addr IS NOT DISTINCT FROM inet_client_addr() \
             AND datname = current_database()",
            &[],
        )
        .unwrap()
        .get(0)
}

fn lane_locked(admin: &mut postgres::Client) -> i64 {
    admin
        .query_one(
            "SELECT count(*) FROM pg_locks l JOIN pg_class c ON c.oid = l.relation \
             JOIN pg_namespace n ON n.oid = c.relnamespace \
             WHERE c.relname = 'store_lane' AND n.nspname = current_schema() \
             AND l.mode = 'RowShareLock' AND l.granted AND l.pid <> pg_backend_pid()",
            &[],
        )
        .unwrap()
        .get(0)
}

/// A TCP gate in front of one server: passes connections through until
/// `refuse` is set, then closes each new one at once.
struct Gate {
    port: u16,
    refuse: Arc<AtomicBool>,
}

fn pipe(mut from: TcpStream, mut to: TcpStream) {
    let mut buffer = [0u8; 16384];
    loop {
        match from.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if to.write_all(&buffer[..n]).is_err() {
                    break;
                }
            }
        }
    }
    let _ = to.shutdown(Shutdown::Both);
    let _ = from.shutdown(Shutdown::Both);
}

impl Gate {
    fn start(upstream: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let refuse = Arc::new(AtomicBool::new(false));
        let flag = refuse.clone();
        std::thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(client) = client else { return };
                if flag.load(Ordering::SeqCst) {
                    let _ = client.shutdown(Shutdown::Both);
                    continue;
                }
                let Ok(server) = TcpStream::connect(&upstream) else {
                    continue;
                };
                let (a, b) = (client.try_clone().unwrap(), server.try_clone().unwrap());
                std::thread::spawn(move || pipe(a, b));
                std::thread::spawn(move || pipe(server, client));
            }
        });
        Self { port, refuse }
    }
}

fn admin_on(url: &str, schema: &str) -> postgres::Client {
    let mut client = postgres::Client::connect(url, postgres::NoTls).unwrap();
    client
        .batch_execute(&format!("SET search_path TO {schema}"))
        .unwrap();
    client
}

#[test]
fn multihost() {
    let Some(template) = env("GAP811_MH_URL") else {
        eprintln!("NOT_RUN: GAP811_MH_URL is unset");
        return;
    };
    let arm = env("GAP811_ARM").unwrap_or_default();
    let store = attach();
    let schema = store.postgres.as_ref().unwrap().1.clone();
    let mut a = admin_on(&env("GAP811_ADMIN_A").unwrap(), &schema);
    let mut b = admin_on(&env("GAP811_ADMIN_B").unwrap(), &schema);
    let gate = env("GAP811_GATE").map(Gate::start);
    let url = match &gate {
        Some(gate) => template.replace("GATEPORT", &gate.port.to_string()),
        None => template,
    };
    let log = |step: &str, outcome: String, a: &mut postgres::Client, b: &mut postgres::Client| {
        record(format!(
            "{{\"arm\":\"{arm}\",\"step\":\"{step}\",\"outcome\":\"{}\",\"conns_a\":{},\"conns_b\":{},\"lane_rowlock_a\":{},\"lane_rowlock_b\":{},\"head_a\":\"{}\",\"head_b\":\"{}\"}}",
            esc(&outcome),
            ours(a),
            ours(b),
            lane_locked(a),
            lane_locked(b),
            head(a, "b1"),
            head(b, "b1"),
        ));
    };
    let selection = store.selection_via(&url);
    let service = match Service::try_open(&store, selection) {
        Ok(service) => service,
        Err(error) => {
            log("open", format!("Err {error}"), &mut a, &mut b);
            return;
        }
    };
    log("open", "Ok".into(), &mut a, &mut b);
    let workspace = match service.try_bind(store.branch("b1")) {
        Ok(w) => w,
        Err(error) => {
            log("mount", format!("Err {error}"), &mut a, &mut b);
            service.abandon();
            return;
        }
    };
    log("mount", "Ok".into(), &mut a, &mut b);
    write(&workspace, "/fused", format!("fused {arm} {}\n", nonce()).as_bytes());
    let fused = verdict(&commit(&workspace));
    log("fused_commit", verdict_text(&fused), &mut a, &mut b);
    if let Some(gate) = &gate {
        gate.refuse.store(true, Ordering::SeqCst);
        log("gate_closed", "first host refuses new connections".into(), &mut a, &mut b);
    }
    if env("GAP811_XID_ALIGN").is_some() {
        // Make the lane's transaction id on the second server equal to an
        // id that is in progress on the first: the writer's lane check
        // (`pg_xact_status` on the first server) then sees "in progress".
        let next = |c: &mut postgres::Client| -> i64 {
            c.query_one(
                "SELECT pg_snapshot_xmax(pg_current_snapshot())::text::bigint",
                &[],
            )
            .unwrap()
            .get(0)
        };
        let mut burned = (0, 0);
        loop {
            let (na, nb) = (next(&mut a), next(&mut b));
            if na == nb {
                break;
            }
            let (behind, count) = if na < nb { (&mut a, &mut burned.0) } else { (&mut b, &mut burned.1) };
            behind.batch_execute("SELECT pg_current_xact_id()").unwrap();
            *count += 1;
        }
        a.batch_execute("BEGIN").unwrap();
        let held: String = a
            .query_one("SELECT pg_current_xact_id()::text", &[])
            .unwrap()
            .get(0);
        let nb = next(&mut b);
        log(
            "xid_aligned",
            format!("in progress on A: {held}; next on B: {nb}; burned A {} B {}", burned.0, burned.1),
            &mut a,
            &mut b,
        );
    }
    write(&workspace, "/forced", format!("forced {arm} {}\n", nonce()).as_bytes());
    let forced = verdict(&commit_forced(&workspace));
    log("forced_commit", verdict_text(&forced), &mut a, &mut b);
    write(&workspace, "/after", format!("after {arm} {}\n", nonce()).as_bytes());
    let again = verdict(&commit(&workspace));
    log("fused_commit_2", verdict_text(&again), &mut a, &mut b);
    let unmounted = service.try_unmount(workspace);
    log("unmount", format!("{unmounted:?}"), &mut a, &mut b);
    service.abandon();
}

/// Live intent rows and advisory keys (one-key, by any backend) of this
/// server, with the backends holding keys.
fn custody(admin: &mut postgres::Client) -> (i64, i64, String) {
    let rows: i64 = admin
        .query_one("SELECT count(*) FROM gc_intent WHERE state <> 'done'", &[])
        .unwrap()
        .get(0);
    let row = admin
        .query_one(
            "SELECT count(*), coalesce(string_agg(pid::text || ':' || mode, ',' ORDER BY pid, mode), '') \
             FROM pg_locks WHERE locktype = 'advisory' AND objsubid = 1 AND granted",
            &[],
        )
        .unwrap();
    (rows, row.get(0), row.get(1))
}

fn answer_text(answer: &Result<SdkValue, layerfs_bridge::control::SdkRefusal>) -> String {
    match answer {
        Ok(_) => "Ok".into(),
        Err(refusal) => format!("Err {}: {}", refusal.code, refusal.message),
    }
}

fn compact_text(selection: &StoreSelection, store: &TestStore, op: u8) -> String {
    let request = CompactRequest {
        op_id: [op; 16],
        ..CompactRequest::default()
    };
    match Handles::open_writable(selection, &store.binding, CURSOR) {
        Ok(Handles::Postgres(handles)) => {
            let result = handles.compact(&request);
            handles.close();
            match result {
                Ok(receipt) => format!("Ok({})", receipt.path.as_str()),
                Err(e) => format!("Err {e:?}"),
            }
        }
        Ok(_) => unreachable!(),
        Err(e) => format!("open Err {e:?}"),
    }
}

#[test]
fn pooled() {
    let Some(pooled) = env("GAP811_POOLED_URL") else {
        eprintln!("NOT_RUN: GAP811_POOLED_URL is unset");
        return;
    };
    let arm = env("GAP811_ARM").unwrap_or_default();
    let Some(store) = TestStore::postgres("pool", &[("/seed", b"seed\n")]) else {
        return;
    };
    for name in ["b1", "b2", "b3"] {
        store.fork(name);
    }
    let mut admin = store.admin();
    let selection = with_sync(&store.selection_via(&pooled));
    let log = |step: &str, outcome: &str, admin: &mut postgres::Client| {
        let (rows, keys, pids) = custody(admin);
        record(format!(
            "{{\"arm\":\"{arm}\",\"step\":\"{step}\",\"outcome\":\"{}\",\"live_rows\":{rows},\"keys\":{keys},\"key_pids\":\"{pids}\"}}",
            esc(outcome)
        ));
    };
    if env("GAP811_STEAL").is_some() {
        return steal(&store, &selection, &pooled, &arm, &mut admin);
    }
    // A second pooled client that keeps one server connection in a
    // transaction from right after the open: the next transactions of the
    // service get another server connection than the open did.
    let pin = env("GAP811_PIN").is_some();
    let service = match Service::try_open(&store, selection.clone()) {
        Ok(s) => s,
        Err(e) => {
            log("open", &format!("Err {e}"), &mut admin);
            return;
        }
    };
    log("open", "Ok", &mut admin);
    let mounted = service.try_bind(store.branch("b1"));
    log("mount_b1", &format!("{:?}", mounted.as_ref().map(|_| ())), &mut admin);
    // Pinned after the first mount: the server connection that holds its
    // keys stays in a transaction, so the session's later statements run on
    // another one.
    let _pinned = pin.then(|| {
        let mut client = postgres::Client::connect(&pooled, postgres::NoTls).unwrap();
        client.batch_execute("BEGIN").unwrap();
        let pid: i32 = client.query_one("SELECT pg_backend_pid()", &[]).unwrap().get(0);
        record(format!("{{\"arm\":\"{arm}\",\"step\":\"pin\",\"pinned_pid\":{pid}}}"));
        client
    });
    if let Ok(workspace) = &mounted {
        write(workspace, "/one", b"one\n");
        let v = verdict(&commit(workspace));
        log("fused_commit", &verdict_text(&v), &mut admin);
        write(workspace, "/two", b"two\n");
        let v = verdict(&commit_forced(workspace));
        log("forced_commit_hold_lane", &verdict_text(&v), &mut admin);
    }
    let removed_mounted = service.call("branch.remove", vec![("branch", SdkValue::text("b1"))]);
    log("branch_remove_mounted_b1", &answer_text(&removed_mounted), &mut admin);
    let compacted = compact_text(&selection, &store, 7);
    log("compaction_while_mounted", &compacted, &mut admin);
    if let Ok(workspace) = mounted {
        let released = service.try_unmount(workspace);
        log("unmount_b1", &format!("{released:?}"), &mut admin);
    }
    let removed = service.call("branch.remove", vec![("branch", SdkValue::text("b2"))]);
    log("branch_remove_b2", &answer_text(&removed), &mut admin);
    let forked = service.call(
        "branch.fork",
        vec![
            ("name", SdkValue::text("b4")),
            ("from", SdkValue::text("b3")),
        ],
    );
    log("branch_fork_b4_id_alloc", &answer_text(&forked), &mut admin);
    let compacted = compact_text(&selection, &store, 8);
    log("compaction_unmounted", &compacted, &mut admin);
    let second = service.try_bind(store.branch("b3"));
    log("mount_b3", &format!("{:?}", second.as_ref().map(|_| ())), &mut admin);
    if let Ok(workspace) = second {
        write(&workspace, "/three", b"three\n");
        let v = verdict(&commit(&workspace));
        log("fused_commit_b3", &verdict_text(&v), &mut admin);
        let released = service.try_unmount(workspace);
        log("unmount_b3", &format!("{released:?}"), &mut admin);
    }
    service.abandon();
    log("closed", "service dropped", &mut admin);
    drop(_pinned);
    std::thread::sleep(Duration::from_millis(500));
    log("pin_released", "pinned client closed", &mut admin);
    let failures = store.integrity_failures();
    record(format!(
        "{{\"arm\":\"{arm}\",\"step\":\"audit\",\"failures\":\"{}\"}}",
        esc(&format!("{failures:?}"))
    ));
}

/// Two services behind a transaction pooler mount one Branch; the second's
/// unmount runs on the server connection that holds the first's keys.
fn steal(
    store: &TestStore,
    selection: &StoreSelection,
    pooled: &str,
    arm: &str,
    admin: &mut postgres::Client,
) {
    let log = |step: &str, outcome: &str, admin: &mut postgres::Client| {
        let (rows, keys, pids) = custody(admin);
        record(format!(
            "{{\"arm\":\"{arm}\",\"step\":\"{step}\",\"outcome\":\"{}\",\"live_rows\":{rows},\"keys\":{keys},\"key_pids\":\"{pids}\"}}",
            esc(outcome)
        ));
    };
    let s = Service::open(store, selection.clone());
    let t = Service::open(store, selection.clone());
    log("open_s_t", "Ok", admin);
    let ws = s.bind(store.branch("b1"));
    log("s_mount_b1", "Ok", admin);
    // The pooler hands out the server connection released last: the one
    // that just ran S's admission, which now holds S's keys.
    let mut pin = postgres::Client::connect(pooled, postgres::NoTls).unwrap();
    pin.batch_execute("BEGIN").unwrap();
    let pinned: i32 = pin.query_one("SELECT pg_backend_pid()", &[]).unwrap().get(0);
    log("pin", &format!("pinned server pid {pinned}"), admin);
    let wt = t.try_bind(store.branch("b1"));
    log("t_mount_b1_while_pinned", &format!("{:?}", wt.as_ref().map(|_| ())), admin);
    pin.batch_execute("COMMIT").unwrap();
    drop(pin);
    std::thread::sleep(Duration::from_millis(200));
    log("pin_released", "Ok", admin);
    if let Ok(wt) = wt {
        let released = t.try_unmount(wt);
        log("t_unmount", &format!("{released:?}"), admin);
    }
    let compacted = compact_text(&store.selection, store, 9);
    log("direct_compaction_while_s_mounted", &compacted, admin);
    let released = s.try_unmount(ws);
    log("s_unmount", &format!("{released:?}"), admin);
    s.abandon();
    t.abandon();
    std::thread::sleep(Duration::from_millis(300));
    log("services_closed", "Ok", admin);
    let compacted = compact_text(&store.selection, store, 10);
    log("direct_compaction_after_all_unmounted", &compacted, admin);
    let removed = store.open().call("branch.remove", vec![("branch", SdkValue::text("b1"))]);
    log("direct_branch_remove_b1", &answer_text(&removed), admin);
}

fn percentiles(mut values: Vec<f64>) -> String {
    if values.is_empty() {
        return "{\"n\":0}".into();
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| values[((values.len() as f64 - 1.0) * q).round() as usize];
    format!(
        "{{\"n\":{},\"p50\":{:.1},\"p90\":{:.1},\"p99\":{:.1},\"max\":{:.1}}}",
        values.len(),
        at(0.5),
        at(0.9),
        at(0.99),
        values[values.len() - 1]
    )
}

#[test]
fn lane_contention() {
    let Some(writers) = env("GAP811_WRITERS") else {
        eprintln!("NOT_RUN: GAP811_WRITERS is unset");
        return;
    };
    let writers: usize = writers.parse().unwrap();
    let seconds = env_or("GAP811_SECONDS", 60);
    let lock_ms = env_or("GAP811_LOCK_TIMEOUT_MS", 30_000) as u32;
    let shared = env("GAP811_SHARED").is_some();
    let arm = env("GAP811_ARM").unwrap_or_default();
    let Some(store) = TestStore::postgres("lane", &[("/seed", b"seed\n")]) else {
        return;
    };
    let names: Vec<String> = (0..writers)
        .map(|n| if shared { "shared".into() } else { format!("w{n}") })
        .collect();
    let mut forked = std::collections::BTreeSet::new();
    for name in &names {
        if forked.insert(name.clone()) {
            store.fork(name);
        }
    }
    let selection = store.with_lock_timeout(&with_sync(&store.selection), lock_ms);
    let services: Vec<Service> = (0..writers)
        .map(|_| Service::open(&store, selection.clone()))
        .collect();
    let stop = Arc::new(AtomicBool::new(false));
    // Server lock waits: (pid, waitstart) -> longest age seen, ms.
    let waits: Arc<Mutex<std::collections::HashMap<(i32, String), (f64, String)>>> =
        Arc::default();
    let samples = Arc::new(AtomicU64::new(0));
    let sampler = {
        let (stop, waits, samples) = (stop.clone(), waits.clone(), samples.clone());
        let mut admin = store.admin();
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let rows = admin
                    .query(
                        "SELECT pid, waitstart::text, locktype, \
                         extract(epoch FROM clock_timestamp() - waitstart) * 1000 \
                         FROM pg_locks WHERE NOT granted AND waitstart IS NOT NULL",
                        &[],
                    )
                    .unwrap();
                samples.fetch_add(1, Ordering::Relaxed);
                let mut waits = waits.lock().unwrap();
                for row in rows {
                    let age: f64 = row.get::<_, rust_decimal_free::Num>(3).0;
                    let entry = waits
                        .entry((row.get(0), row.get(1)))
                        .or_insert((0.0, row.get(2)));
                    entry.0 = entry.0.max(age);
                }
                drop(waits);
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    };
    let started = Instant::now();
    let until = started + Duration::from_secs(seconds);
    let results: Vec<Vec<(String, f64)>> = std::thread::scope(|scope| {
        let handles: Vec<_> = services
            .iter()
            .zip(&names)
            .enumerate()
            .map(|(i, (service, name))| {
                let branch = store.branch(name);
                scope.spawn(move || {
                    let workspace = service.bind(branch);
                    let mut out = Vec::new();
                    let mut n = 0u64;
                    while Instant::now() < until {
                        n += 1;
                        let t = Instant::now();
                        // A write may allocate ids (a lane write): it can be
                        // refused Busy under contention like a Commit.
                        let op = adversary::Op::Write(
                            format!("/w{i}-{}", n % 4),
                            format!("writer {i} commit {n}\n").into_bytes(),
                        );
                        if let Err(error) = adversary::apply(&workspace, &op) {
                            let ms = t.elapsed().as_secs_f64() * 1000.0;
                            out.push((format!("WriteRefused: {error}"), ms));
                            continue;
                        }
                        let t = Instant::now();
                        let v = verdict(&commit(&workspace));
                        let ms = t.elapsed().as_secs_f64() * 1000.0;
                        let class = match &v {
                            Verdict::Committed(_) => "Committed".to_owned(),
                            Verdict::Refused(text) | Verdict::Unknown(text) => {
                                let kind = if matches!(v, Verdict::Unknown(_)) {
                                    "Unknown"
                                } else {
                                    "Refused"
                                };
                                format!("{kind}: {}", text.chars().take(160).collect::<String>())
                            }
                            other => format!("{other:?}"),
                        };
                        out.push((class, ms));
                    }
                    let _ = service.try_unmount(workspace);
                    out
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let wall = started.elapsed().as_secs_f64();
    stop.store(true, Ordering::SeqCst);
    sampler.join().unwrap();
    for service in services {
        service.abandon();
    }
    let mut classes = std::collections::BTreeMap::<String, u64>::new();
    let mut committed_ms = Vec::new();
    let mut busy_ms = Vec::new();
    let mut per_writer = Vec::new();
    for out in &results {
        let mut ok = 0;
        for (class, ms) in out {
            let key = if class.starts_with("Refused") && class.contains("Busy") {
                "Refused Busy".to_owned()
            } else if class.starts_with("WriteRefused") && class.contains("Busy") {
                "WriteRefused Busy".to_owned()
            } else {
                class.clone()
            };
            *classes.entry(key).or_default() += 1;
            if class == "Committed" {
                ok += 1;
                committed_ms.push(*ms);
            } else if class.contains("Busy") {
                busy_ms.push(*ms);
            }
        }
        per_writer.push(ok);
    }
    let waits = waits.lock().unwrap();
    let lock_waits: Vec<f64> = waits.values().map(|(ms, _)| *ms).collect();
    let mut kinds = std::collections::BTreeMap::<String, u64>::new();
    for (_, kind) in waits.values() {
        *kinds.entry(kind.clone()).or_default() += 1;
    }
    let deadlocks = classes.keys().filter(|k| k.contains("40P01") || k.to_lowercase().contains("deadlock")).count();
    let busy_min = busy_ms.iter().cloned().fold(f64::INFINITY, f64::min);
    record(format!(
        "{{\"arm\":\"{arm}\",\"writers\":{writers},\"shared\":{shared},\"lock_timeout_ms\":{lock_ms},\"seconds\":{wall:.1},\"sync\":\"{:?}\",\"per_writer_committed\":{per_writer:?},\"classes\":\"{}\",\"commit_ms\":{},\"busy_ms\":{},\"server_lock_waits_ms\":{},\"wait_kinds\":\"{}\",\"samples\":{},\"deadlocks\":{deadlocks}}}",
        sync_setting(),
        esc(&format!("{classes:?}")),
        percentiles(committed_ms.clone()),
        percentiles(busy_ms.clone()),
        percentiles(lock_waits),
        esc(&format!("{kinds:?}")),
        samples.load(Ordering::Relaxed),
    ));
    assert_eq!(deadlocks, 0, "no deadlock");
    assert!(
        !classes.keys().any(|k| k.starts_with("Unknown")),
        "no unknown outcome: {classes:?}"
    );
    assert!(
        busy_ms.is_empty() || busy_min >= f64::from(lock_ms),
        "a Busy before lock_timeout: {busy_min} ms < {lock_ms}"
    );
    assert!(per_writer.iter().all(|n| *n > 0), "a starved writer: {per_writer:?}");
}

/// `numeric` read as f64 without a decimal crate: the text form parsed.
mod rust_decimal_free {
    use postgres::types::{FromSql, Type};
    pub struct Num(pub f64);
    impl<'a> FromSql<'a> for Num {
        fn from_sql(
            _: &Type,
            raw: &'a [u8],
        ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            // numeric binary: ndigits, weight, sign, dscale, then base-10000 digits
            let int = |i: usize| i16::from_be_bytes([raw[i], raw[i + 1]]);
            let (ndigits, weight, sign) = (int(0) as usize, int(2) as i32, int(4) as u16);
            let mut value = 0f64;
            for d in 0..ndigits {
                let digit = int(8 + 2 * d) as f64;
                value += digit * 10000f64.powi(weight - d as i32);
            }
            Ok(Num(if sign == 0x4000 { -value } else { value }))
        }
        fn accepts(ty: &Type) -> bool {
            *ty == Type::NUMERIC
        }
    }
}

#[test]
fn lost_unmount_reply() {
    let Some(store) = TestStore::postgres("lostrel", &[("/seed", b"seed\n")]) else {
        return;
    };
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, _) = fault_proxy::FaultProxy::start(&url);
    let via = fault_proxy::proxied_url(&url, proxy.port).0;
    let mut admin = store.admin();
    let rows_of = |admin: &mut postgres::Client, name: &str| -> i64 {
        admin
            .query_one(
                "SELECT count(*) FROM gc_intent WHERE scope_kind = 'branch' AND scope_id = $1 \
                 AND state <> 'done'",
                &[&store.branch(name).as_slice()],
            )
            .unwrap()
            .get(0)
    };
    // Which message of the release to drop: the first reply (the delete's)
    // and the last (the unlock's `ReadyForQuery`).
    let clean_messages = {
        let (x, y) = ("c-x", "c-y");
        store.fork(x);
        store.fork(y);
        let service = Service::open(&store, store.selection_via(&via));
        let w = service.bind(store.branch(x));
        proxy.arm(None);
        service.unmount(w);
        let window = proxy.disarm();
        service.stop();
        let kinds: String = window
            .log
            .iter()
            .map(|(c, k)| format!("{c}{k} "))
            .collect();
        record(format!(
            "{{\"step\":\"clean_release\",\"server_messages\":{},\"connection_and_kind\":\"{kinds}\"}}",
            window.seen
        ));
        // Bind replies ('2') start the delete's and the unlock's replies.
        let binds: Vec<u64> = (0..window.log.len() as u64)
            .filter(|i| window.log[*i as usize].1 == '2')
            .collect();
        (window.seen, binds)
    };
    let (seen, binds) = clean_messages;
    let delete_reply = binds[binds.len() - 2];
    let unlock_reply = binds[binds.len() - 1];
    for (run, at) in [
        ("before-delete", 0),
        ("delete-reply", delete_reply),
        ("unlock-reply", unlock_reply),
        ("last", seen - 1),
    ] {
        let (x, y) = (format!("{run}-x"), format!("{run}-y"));
        store.fork(&x);
        store.fork(&y);
        let before_pids = proxy.pids().len();
        let service = Service::open(&store, store.selection_via(&via));
        let writer = proxy.pids()[before_pids].unwrap();
        let wx = service.bind(store.branch(&x));
        let wy = service.bind(store.branch(&y));
        let keys_before = custody(&mut admin).1;
        proxy.arm(Some((at, fault_proxy::Fault::Drop)));
        let released = service.try_unmount(wx);
        let window = proxy.disarm();
        let gone = adversary::wait_until(Duration::from_secs(10), || {
            admin
                .query_one("SELECT count(*) FROM pg_stat_activity WHERE pid = $1", &[&writer])
                .unwrap()
                .get::<_, i64>(0)
                == 0
        });
        let (_, keys_after, key_pids) = custody(&mut admin);
        let (rx, ry) = (rows_of(&mut admin, &x), rows_of(&mut admin, &y));
        let next_mount = service.try_bind(store.branch(&x)).map(|_| ());
        // Another daemon: the GC calls the leftovers decide.
        let other = store.open();
        let remove_x = other.call("branch.remove", vec![("branch", SdkValue::text(&x))]);
        let remove_y = other.call("branch.remove", vec![("branch", SdkValue::text(&y))]);
        let compaction = compact_text(&store.selection, &store, 0x40 + at as u8);
        record(format!(
            "{{\"run\":\"{run}\",\"dropped_at\":{at},\"fired\":{},\"fired_on\":\"{}\",\"unmount\":\"{}\",\"writer_backend_ended\":{gone},\"keys_before\":{keys_before},\"keys_after\":{keys_after},\"key_pids\":\"{key_pids}\",\"row_released\":{rx},\"row_other_mount\":{ry},\"same_service_next_mount\":\"{}\",\"branch_remove_released\":\"{}\",\"branch_remove_other_mounted\":\"{}\",\"compaction\":\"{}\"}}",
            window.fired.is_some(),
            esc(&format!("{:?}", window.fired)),
            esc(&format!("{released:?}")),
            esc(&format!("{next_mount:?}")),
            esc(&answer_text(&remove_x)),
            esc(&answer_text(&remove_y)),
            esc(&compaction),
        ));
        assert!(window.fired.is_some());
        match &released {
            Ok(()) => assert_eq!(rx, 0, "an Ok release deleted its row"),
            Err(e) => assert!(e.contains("UnknownOutcome"), "lost reply: {e}"),
        }
        assert!(gone, "the quarantined writer's backend ends");
        assert_eq!(keys_after, 0, "no key outlives the closed connection");
        assert!(rx <= 1 && ry == 1, "at most the released row, the other mount's row kept");
        assert!(next_mount.is_err(), "the quarantined writer admits nothing more");
        assert!(remove_x.is_ok() == (rx == 0), "removal decided by the row alone");
        assert!(answer_text(&remove_y).contains("MountsActive"));
        assert!(compaction.contains("MountsActive"), "compaction refused: {compaction}");
        // The daemon exits: nothing it held is released by its exit.
        drop(wy);
        service.abandon();
        let ry_after_exit = rows_of(&mut admin, &y);
        // The operator's release settles the orphan.
        let ops: Vec<Vec<u8>> = admin
            .query(
                "SELECT op_id FROM gc_intent WHERE scope_kind = 'branch' \
                 AND scope_id IN ($1, $2) AND state <> 'done'",
                &[&store.branch(&x).as_slice(), &store.branch(&y).as_slice()],
            )
            .unwrap()
            .into_iter()
            .map(|r| r.get(0))
            .collect();
        let handles = store.handles();
        let settled: Vec<String> = ops
            .iter()
            .map(|op| {
                let op: [u8; 16] = op.as_slice().try_into().unwrap();
                format!(
                    "{:?}",
                    adversary::history(&handles).gc_release_done(&op, "gap811 operator release")
                )
            })
            .collect();
        handles.close();
        let compaction_after = compact_text(&store.selection, &store, 0x60 + at as u8);
        record(format!(
            "{{\"run\":\"{run}\",\"row_other_after_daemon_exit\":{ry_after_exit},\"operator_release\":\"{}\",\"compaction_after_release\":\"{}\"}}",
            esc(&format!("{settled:?}")),
            esc(&compaction_after)
        ));
        assert_eq!(ry_after_exit, 1, "the daemon's exit releases nothing");
        assert!(compaction_after.starts_with("Ok"), "{compaction_after}");
        other.stop();
    }
    assert!(store.no_open_transaction());
    let _ = HistoryError::Busy;
}
