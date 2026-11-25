//! Independent #768 tester (written by the independent tester, adapted by
//! the author to the review fixes): what a user sees when a pooled
//! PostgreSQL reader's (or the writer's) backend is gone. The idle reader's
//! check (`PostgresStorage::usable`) is local, plus one empty query (5 s
//! cap) once the reader sat idle `reader_idle_check_ms` (30 s by default).
//!
//! Each scenario reaches the server through a pass-through proxy (no fault
//! armed) only to learn each connection's backend pid, terminates backends
//! with `pg_terminate_backend` from a plain admin connection, then mounts
//! and reads files no earlier call read (so the read reaches the Workspace
//! reader). Every answer is printed as one `T768_STALE_*` line and asserted
//! where the behaviour is defined:
//!
//! * an idle reader terminated between mounts: the local check sees it; the
//!   mount opens a fresh reader and every read succeeds;
//! * a mounted Workspace's reader terminated: its next read sees the closed
//!   connection before it sends anything and reopens the reader once
//!   (`session/reopen.rs`), so its reads and its unmount succeed and no row
//!   and no key stay (before readers reopened, its reads failed and its
//!   unmount reported the reader's failure, while the writer still released
//!   the mount intent, #768 review fix);
//! * an idle writer terminated: the next mount sees the closed connection
//!   before it sends anything and reopens the writer once
//!   (`session/reopen.rs`); both mounts and their reads work and no key
//!   stays (before the reopen existed both were refused);
//! * an idle reader aged past the threshold whose peer went silent: the
//!   mount's check gives up after its 5 s cap and opens a fresh reader, so
//!   the first read does not stall.
//!
//! `container_restart` restarts the private test container
//! (`LFS768_RESTART_CONTAINER` names it, `lfs-768c-pg` for the tester's
//! server); NOT_RUN without that variable, by default; it only records.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/fault_proxy.rs"]
#[allow(dead_code)]
mod fault_proxy;
#[path = "support/tester768.rs"]
#[allow(dead_code)]
mod tester768;

use adversary::{wait_until, Service, TestStore};
use fault_proxy::FaultProxy;
use layerfs_persistence::SqlWork;
use layerfs_store_service::store::BoundWorkspace;
use layerfs_store_service::write::mutation::Mutation;
use std::time::{Duration, Instant};
use tester768::*;

const FILES: usize = 12;

fn seed() -> Vec<(String, Vec<u8>)> {
    (0..FILES).map(|n| (format!("/f{n}"), content(n))).collect()
}

/// Incompressible bytes, large enough to live in their own chunk objects
/// (not inline in the inode values a mount reads).
fn content(n: usize) -> Vec<u8> {
    adversary::random_bytes(1000 + n as u64, 256 * 1024)
}

fn store(label: &str) -> Option<TestStore> {
    let seed = seed();
    let seed: Vec<(&str, &[u8])> = seed
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_slice()))
        .collect();
    TestStore::postgres(label, &seed)
}

/// Reads `/f{n}` through the Workspace; the answer, never a panic.
fn read(workspace: &BoundWorkspace, n: usize) -> Result<usize, String> {
    let view = Mutation::new(workspace).map_err(|e| format!("view: {e:?}"))?;
    let path = format!("/f{n}");
    let stat = view
        .optional(&path)
        .map_err(|e| format!("lookup: {e:?}"))?
        .ok_or_else(|| format!("{path}: absent"))?;
    let at = 128 * 1024;
    let bytes = view
        .read(stat.serial, at as u64, 8192)
        .map_err(|e| format!("read: {e:?}"))?;
    let _ = view.finish();
    let expected = &content(n)[at..at + 8192];
    if bytes != expected {
        return Err(format!("{path}: wrong bytes ({} read)", bytes.len()));
    }
    Ok(bytes.len())
}

fn short<T, E: AsRef<str>>(answer: &Result<T, E>) -> String {
    match answer {
        Ok(_) => "Ok".into(),
        Err(error) => {
            let text = error.as_ref();
            let tail: String = text
                .chars()
                .rev()
                .take(260)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("Err(...{})", tail.replace('"', "'"))
        }
    }
}

/// Connects opened by every reader session so far (Workspace readers).
fn reader_connects(service: &Service) -> u64 {
    reader_work(service).connects
}

fn reader_work(service: &Service) -> SqlWork {
    let mut total = SqlWork::default();
    for reader in &service.service.opened.diagnostics().unwrap().readers {
        total.accumulate(reader);
    }
    total
}

struct Proxied {
    proxy: FaultProxy,
    service: Service,
    writer: i32,
}

fn open_proxied(store: &TestStore) -> Proxied {
    open_proxied_with(store, layerfs_persistence::READER_IDLE_CHECK_MS)
}

/// [`open_proxied`] with readers checked by query once idle `idle_check_ms`.
fn open_proxied_with(store: &TestStore, idle_check_ms: u32) -> Proxied {
    let (url, _) = store.postgres.clone().unwrap();
    let (proxy, proxied) = FaultProxy::start(&url);
    let layerfs_persistence::StoreSelection::Postgres(mut selection) =
        store.selection_via(&proxied)
    else {
        unreachable!("a PostgreSQL Store");
    };
    selection.reader_idle_check_ms = idle_check_ms;
    let service = Service::open(
        store,
        layerfs_persistence::StoreSelection::Postgres(selection),
    );
    let writer = proxy.pids()[0].expect("the writer's pid");
    if let Some(prefilled) = service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    Proxied {
        proxy,
        service,
        writer,
    }
}

impl Proxied {
    /// Backends of this service still running, except the writer.
    fn readers(&self, admin: &mut postgres::Client) -> Vec<i32> {
        self.proxy
            .pids()
            .into_iter()
            .flatten()
            .filter(|pid| *pid != self.writer && alive(admin, *pid))
            .collect()
    }
}

fn fresh_mounts_and_reads(store: &TestStore, branch: layerfs_history::BranchId, n: usize) {
    let fresh = store.open();
    let workspace = fresh.bind(branch);
    read(&workspace, n).expect("a fresh service reads");
    fresh.unmount(workspace);
    fresh.stop();
}

#[test]
fn an_idle_reader_terminated_between_mounts() {
    let Some(store) = store("t768-stale-idle") else {
        return;
    };
    let branch = store.fork("work");
    let mut admin = store.admin();
    let p = open_proxied(&store);
    let warm = p.service.bind(branch);
    read(&warm, 0).unwrap();
    p.service.unmount(warm);
    let idle = p.readers(&mut admin);
    assert!(!idle.is_empty(), "an idle reader stays open");
    for pid in &idle {
        assert!(terminate(&mut admin, *pid));
    }
    // Let the termination's FATAL and EOF reach the client socket.
    std::thread::sleep(Duration::from_millis(200));
    let connects = reader_connects(&p.service);
    let started = Instant::now();
    let bound = p.service.try_bind(branch);
    let mount_ms = started.elapsed().as_millis();
    let opened = reader_connects(&p.service) - connects;
    let (first, second, third) = match &bound {
        Ok(workspace) => (read(workspace, 1), read(workspace, 2), read(workspace, 3)),
        Err(_) => (Err("-".into()), Err("-".into()), Err("-".into())),
    };
    println!(
        "T768_STALE_IDLE {{\"terminated\":{idle:?},\"mount\":\"{}\",\"mount_ms\":{mount_ms},\"reader_connects_at_mount\":{opened},\"read1\":\"{}\",\"read2\":\"{}\",\"read3\":\"{}\"}}",
        short(&bound.as_ref().map(|_| ())),
        short(&first),
        short(&second),
        short(&third)
    );
    let unmounted = bound.map(|workspace| p.service.try_unmount(workspace));
    println!(
        "T768_STALE_IDLE_UNMOUNT {:?}",
        unmounted.as_ref().map(|r| r.is_ok())
    );
    // The next mount on the same service, and its read.
    let again = p.service.try_bind(branch);
    let again_read = again
        .as_ref()
        .map(|w| read(w, 4))
        .unwrap_or(Err("-".into()));
    println!(
        "T768_STALE_IDLE_NEXT {{\"mount\":\"{}\",\"read\":\"{}\"}}",
        short(&again.as_ref().map(|_| ())),
        short(&again_read)
    );
    if let Ok(workspace) = again {
        let _ = p.service.try_unmount(workspace);
    }
    p.service.abandon();
    fresh_mounts_and_reads(&store, branch, 5);
    assert_eq!(opened, 1, "the mount opened a fresh reader");
    assert!(
        first.is_ok(),
        "the first read after a terminated idle reader: {first:?}"
    );
    assert!(second.is_ok() && third.is_ok(), "{second:?} {third:?}");
    assert!(matches!(unmounted, Ok(Ok(()))), "the unmount");
    assert!(again_read.is_ok(), "the next mount reads: {again_read:?}");
}

#[test]
fn a_mounted_workspace_reader_terminated() {
    let Some(store) = store("t768-stale-mounted") else {
        return;
    };
    let branch = store.fork("work");
    let mut admin = store.admin();
    let p = open_proxied(&store);
    let warm = p.service.bind(branch);
    p.service.unmount(warm);
    let workspace = p.service.bind(branch);
    read(&workspace, 0).unwrap();
    let readers = p.readers(&mut admin);
    for pid in &readers {
        assert!(terminate(&mut admin, *pid));
    }
    std::thread::sleep(Duration::from_millis(200));
    let reopens = reader_work(&p.service).reopens;
    let first = read(&workspace, 1);
    let second = read(&workspace, 2);
    let reopened = reader_work(&p.service).reopens - reopens;
    let connects = reader_connects(&p.service);
    let unmounted = p.service.try_unmount(workspace);
    let keys = mount_keys(&store, branch);
    let me = pid_of(&mut admin);
    let held_after_unmount = key_holders(&mut admin, &keys, &[me]).len();
    let rows_after_unmount = live_mounts(&mut admin, branch);
    println!(
        "T768_STALE_MOUNTED_CUSTODY {{\"keys\":{held_after_unmount},\"rows\":{rows_after_unmount},\"rows_detail\":{:?}}}",
        live_rows(&mut admin)
    );
    let again = p.service.try_bind(branch);
    let again_read = again
        .as_ref()
        .map(|w| read(w, 3))
        .unwrap_or(Err("-".into()));
    let opened = reader_connects(&p.service) - connects;
    println!(
        "T768_STALE_MOUNTED {{\"terminated\":{readers:?},\"read1\":\"{}\",\"read2\":\"{}\",\"unmount\":\"{}\",\"next_mount\":\"{}\",\"next_read\":\"{}\",\"reader_connects_after\":{opened}}}",
        short(&first),
        short(&second),
        short(&unmounted),
        short(&again.as_ref().map(|_| ())),
        short(&again_read)
    );
    if let Ok(workspace) = again {
        let _ = p.service.try_unmount(workspace);
    }
    p.service.abandon();
    fresh_mounts_and_reads(&store, branch, 4);
    assert!(
        first.is_ok() && second.is_ok(),
        "the reads after the reader reopened: {first:?} {second:?}"
    );
    assert_eq!(reopened, 1, "the reader reopened once");
    assert!(unmounted.is_ok(), "the unmount: {unmounted:?}");
    // The writer released the mount intent.
    assert_eq!(
        (held_after_unmount, rows_after_unmount),
        (0, 0),
        "no key and no live row after the failed unmount"
    );
    assert!(
        again_read.is_ok(),
        "the next mount reads on a fresh reader: {again_read:?}"
    );
}

#[test]
fn an_idle_writer_terminated_between_mounts() {
    let Some(store) = store("t768-stale-writer") else {
        return;
    };
    let branch = store.fork("work");
    let mut admin = store.admin();
    let p = open_proxied(&store);
    let warm = p.service.bind(branch);
    p.service.unmount(warm);
    assert!(terminate(&mut admin, p.writer));
    std::thread::sleep(Duration::from_millis(200));
    let first = p.service.try_bind(branch);
    let first_read = first
        .as_ref()
        .map(|w| read(w, 1))
        .unwrap_or(Err("-".into()));
    let first_text = short(&first.as_ref().map(|_| ()));
    if let Ok(workspace) = first {
        let _ = p.service.try_unmount(workspace);
    }
    let second = p.service.try_bind(branch);
    let second_text = short(&second.as_ref().map(|_| ()));
    if let Ok(workspace) = second {
        let _ = p.service.try_unmount(workspace);
    }
    let keys = mount_keys(&store, branch);
    let me = pid_of(&mut admin);
    let held = key_holders(&mut admin, &keys, &[me]);
    println!(
        "T768_STALE_WRITER {{\"mount1\":\"{first_text}\",\"read1\":\"{}\",\"mount2\":\"{second_text}\",\"keys\":{},\"rows\":{}}}",
        short(&first_read),
        held.len(),
        live_mounts(&mut admin, branch)
    );
    assert!(
        held.is_empty(),
        "no key held after the writer's backend ended: {held:?}"
    );
    // The writer's next call finds the connection the server closed
    // before it sends anything and reopens it once: both mounts work.
    assert!(
        first_text.starts_with("Ok") && second_text.starts_with("Ok"),
        "both mounts after the reopen: {first_text}, {second_text}"
    );
    assert!(first_read.is_ok(), "the first mount reads: {first_read:?}");
    let reopens = (p.service.service.opened.session_diagnostics(None))
        .unwrap()
        .unwrap()
        .reopens;
    assert_eq!(reopens, 1, "one reopen");
    assert_eq!(live_mounts(&mut admin, branch), 0, "no row");
    p.service.abandon();
    fresh_mounts_and_reads(&store, branch, 2);
}

fn connectable(url: &str) -> bool {
    postgres::Client::connect(url, postgres::NoTls)
        .and_then(|mut c| c.simple_query("SELECT 1").map(drop))
        .is_ok()
}

#[test]
fn container_restart() {
    let Ok(container) = std::env::var("LFS768_RESTART_CONTAINER") else {
        eprintln!("NOT_RUN: LFS768_RESTART_CONTAINER is unset");
        return;
    };
    let Some(store) = store("t768-stale-restart") else {
        return;
    };
    let branch = store.fork("work");
    let (url, _) = store.postgres.clone().unwrap();
    let service = store.open();
    if let Some(prefilled) = service.service.store.prefill_reader() {
        prefilled.unwrap();
    }
    let warm = service.bind(branch);
    service.unmount(warm);
    let mounted = service.bind(branch);
    read(&mounted, 0).unwrap();
    let started = Instant::now();
    let status = std::process::Command::new("docker")
        .args(["restart", "-t", "2", &container])
        .status()
        .expect("docker");
    assert!(status.success(), "docker restart");
    assert!(
        wait_until(Duration::from_secs(60), || connectable(&url)),
        "the server accepts connections again"
    );
    let down_ms = started.elapsed().as_millis();
    let read1 = read(&mounted, 1);
    let read2 = read(&mounted, 2);
    let unmounted = service.try_unmount(mounted);
    let next = service.try_bind(branch);
    let next_read = next.as_ref().map(|w| read(w, 3)).unwrap_or(Err("-".into()));
    let next_text = short(&next.as_ref().map(|_| ()));
    if let Ok(workspace) = next {
        let _ = service.try_unmount(workspace);
    }
    let mut admin = store.admin();
    println!(
        "T768_STALE_RESTART {{\"restart_ms\":{down_ms},\"mounted_read1\":\"{}\",\"mounted_read2\":\"{}\",\"unmount\":\"{}\",\"next_mount\":\"{next_text}\",\"next_read\":\"{}\",\"live_rows\":{}}}",
        short(&read1),
        short(&read2),
        short(&unmounted),
        short(&next_read),
        live_mounts(&mut admin, branch)
    );
    service.abandon();
    fresh_mounts_and_reads(&store, branch, 4);
}

/// An idle reader whose peer goes silent (no FIN, no reply: a half-open
/// connection, as a NAT timeout or a lost server host leaves it), emulated
/// by the proxy holding that connection's replies for `HOLD` before it
/// delivers them. Readers are checked by query once idle `IDLE_MS`, and the
/// reader is aged past it before each mount: the clean run finds where the
/// check's reply falls, the faulted run holds it there. The mount's check
/// gives up after its 5 s cap and opens a fresh reader, so the first read
/// answers at once instead of waiting for the held reply.
#[test]
fn an_idle_reader_whose_peer_went_silent() {
    use fault_proxy::Fault;
    use layerfs_persistence::READER_CHECK_TIMEOUT;
    const HOLD: Duration = Duration::from_secs(8);
    const IDLE_MS: u32 = 200;
    let age = || std::thread::sleep(Duration::from_millis(u64::from(IDLE_MS) + 100));
    let Some(store) = store("t768-stale-silent") else {
        return;
    };
    let branch = store.fork("work");
    let mut admin = store.admin();
    let p = open_proxied_with(&store, IDLE_MS);
    let warm = p.service.bind(branch);
    p.service.unmount(warm);
    let readers = p.readers(&mut admin);
    assert_eq!(readers.len(), 1, "one idle reader: {readers:?}");
    let pids = p.proxy.pids();
    let reader = pids
        .iter()
        .position(|pid| *pid == Some(readers[0]))
        .unwrap();
    // A clean aged mount: where the idle reader's first reply (the check's)
    // falls.
    age();
    p.proxy.arm(None);
    let clean = p.service.bind(branch);
    let window = p.proxy.disarm();
    read(&clean, 0).unwrap();
    p.service.unmount(clean);
    let target = window
        .log
        .iter()
        .position(|(c, _)| *c == reader)
        .expect("the aged reader is checked at the mount") as u64;
    age();
    let connects = reader_connects(&p.service);
    p.proxy.arm(Some((target, Fault::Pause)));
    let (bound, mount_ms, first, read_ms) = std::thread::scope(|scope| {
        // The peer speaks again after HOLD (the proxy delivers what it held).
        scope.spawn(|| {
            std::thread::sleep(HOLD);
            p.proxy.release();
        });
        let started = Instant::now();
        let bound = p.service.try_bind(branch);
        let mount_ms = started.elapsed().as_millis();
        let started = Instant::now();
        let first = bound
            .as_ref()
            .map(|w| read(w, 1))
            .unwrap_or(Err("-".into()));
        let read_ms = started.elapsed().as_millis();
        (bound, mount_ms, first, read_ms)
    });
    let fired = p.proxy.disarm().fired;
    let opened = reader_connects(&p.service) - connects;
    let second = bound
        .as_ref()
        .map(|w| read(w, 2))
        .unwrap_or(Err("-".into()));
    println!(
        "T768_STALE_SILENT {{\"target\":{target},\"fired\":{:?},\"mount\":\"{}\",\"mount_ms\":{mount_ms},\"read1\":\"{}\",\"read1_ms\":{read_ms},\"read2\":\"{}\",\"reader_connects_at_mount\":{opened}}}",
        fired.as_ref().map(|f| (f.at, f.connection)),
        short(&bound.as_ref().map(|_| ())),
        short(&first),
        short(&second)
    );
    if let Ok(workspace) = bound {
        let _ = p.service.try_unmount(workspace);
    }
    p.service.abandon();
    assert_eq!(
        fired.map(|f| f.connection),
        Some(reader),
        "the check's reply was held"
    );
    let cap = READER_CHECK_TIMEOUT.as_millis();
    assert!(
        mount_ms >= cap && mount_ms < HOLD.as_millis(),
        "the mount waited the check's cap, not the silent peer: {mount_ms} ms"
    );
    assert_eq!(opened, 1, "a fresh reader serves the mount");
    assert!(first.is_ok() && second.is_ok(), "{first:?} {second:?}");
    assert!(
        read_ms < 2_000,
        "the first read does not stall: {read_ms} ms"
    );
}
