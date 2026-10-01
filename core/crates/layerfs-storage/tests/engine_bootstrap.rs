//! Fresh real-process engine bootstrap, enforcement and original-failure custody.

#[path = "support/native_pressure.rs"]
mod pressure;
mod support;

use layerfs_content::filesystem::state::{StateRecord, StateScope, StateTable};
use layerfs_storage::construction_state::ScratchAuthority;
use layerfs_storage::engine::{
    bootstrap_exclusive, EngineBootstrapStage, EngineGuard, ENGINE_HEAP_LIMIT_BYTES,
    ENGINE_PROBE_REQUEST_BYTES,
};
use layerfs_storage::StorageError;
use rusqlite::{ffi, Connection};
use std::io::{self, Read};
use std::process::{Child, Output, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const CHILD_BOUND: Duration = Duration::from_secs(5);
const PIPE_BOUND: u64 = 16 * 1024;

struct ChildOwner {
    child: Child,
    stdout: Option<JoinHandle<io::Result<Vec<u8>>>>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
    reaped: bool,
}

fn bounded_pipe(pipe: impl Read) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(PIPE_BOUND as usize + 1);
    pipe.take(PIPE_BOUND + 1).read_to_end(&mut output)?;
    if output.len() as u64 > PIPE_BOUND {
        return Err(io::Error::other("external engine child pipe bound"));
    }
    Ok(output)
}

impl ChildOwner {
    fn spawn(case: &str) -> Self {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "engine_bootstrap_child", "--nocapture"])
            .env("LAYERFS_ENGINE_BOOTSTRAP_CASE", case)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // The process is owned before either capture thread is created; every
        // panic/error path kills and reaps this child through the same owner.
        let mut owner = Self {
            child,
            stdout: None,
            stderr: None,
            reaped: false,
        };
        let stdout = owner.child.stdout.take().unwrap();
        let stderr = owner.child.stderr.take().unwrap();
        owner.stdout = Some(std::thread::spawn(move || bounded_pipe(stdout)));
        owner.stderr = Some(std::thread::spawn(move || bounded_pipe(stderr)));
        owner
    }

    fn finish(mut self) -> Output {
        let started = Instant::now();
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(
                started.elapsed() < CHILD_BOUND,
                "external engine child bound"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        // try_wait acknowledged an exit; wait reads that cached status and makes
        // the reaping obligation explicit without another wait bound/retry.
        assert_eq!(self.child.wait().unwrap(), status);
        self.reaped = true;
        let stdout = self.stdout.take().unwrap().join().unwrap().unwrap();
        let stderr = self.stderr.take().unwrap().join().unwrap().unwrap();
        Output {
            status,
            stdout,
            stderr,
        }
    }
}

impl Drop for ChildOwner {
    fn drop(&mut self) {
        if !self.reaped {
            let exited = match self.child.try_wait() {
                Ok(Some(_)) => true,
                Ok(None) => false,
                Err(error) => {
                    eprintln!("external engine child exit inspection: {error}");
                    false
                }
            };
            if !exited {
                if let Err(error) = self.child.kill() {
                    eprintln!("external engine child kill: {error}");
                }
            }
            if let Err(error) = self.child.wait() {
                eprintln!("external engine child reap: {error}");
            }
        }
        for capture in [&mut self.stdout, &mut self.stderr] {
            if let Some(capture) = capture.take() {
                match capture.join() {
                    Ok(Ok(output)) => eprintln!(
                        "external engine child failed-path output: {}",
                        String::from_utf8_lossy(&output)
                    ),
                    Ok(Err(error)) => eprintln!("external engine child capture: {error}"),
                    Err(_) => eprintln!("external engine child capture panicked"),
                }
            }
        }
    }
}

fn bootstrap() -> Option<&'static EngineGuard> {
    // One selected fresh child, before any Store/scratch effect. Capability
    // refusal records an unrun supported-provider body; it is not enforcement.
    match unsafe { bootstrap_exclusive() } {
        Ok(guard) => Some(guard),
        Err(error) => {
            assert_eq!(error.stage(), EngineBootstrapStage::Readback);
            assert!(
                matches!(
                    error.cause(),
                    StorageError::UnsupportedPolicy {
                        field: "SQLite required hard heap limit"
                    }
                ),
                "unexpected bootstrap failure: {error}"
            );
            let custody = error.custody();
            assert_eq!(custody.observed_hard_heap_limit, Some(0));
            assert!(!custody.hard_limit_installed && !custody.probe_issued);
            println!(
                "engine supported body NOT_RUN: stage={:?} cause={} custody={custody:?}",
                error.stage(),
                error.cause()
            );
            None
        }
    }
}

fn run_child(case: &str) {
    let output = ChildOwner::spawn(case).finish();
    assert!(
        output.status.success(),
        "case={case}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(&format!("engine case complete: {case}"))
    );
    if String::from_utf8_lossy(&output.stdout).contains("engine supported body NOT_RUN") {
        eprintln!(
            "case={case} supported-provider qualification NOT_RUN: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn fresh_owned_bootstrap_tracks_real_store_and_scratch() {
    run_child("owned");
}

#[test]
fn negative_limit_query_first_refuses_without_reconfiguration() {
    run_child("query-first");
}

#[test]
fn initialized_foreign_sqlite_is_preserved_and_failed_bootstrap_is_cached() {
    run_child("foreign");
}

#[test]
fn native_hard_cap_refuses_and_known_owners_free_once() {
    run_child("native-cap");
}

#[test]
fn real_sql_nomem_reports_transaction_and_cleanup_custody() {
    run_child("sql-nomem");
}

#[test]
fn foreign_limit_change_terminally_quarantines_without_repair() {
    run_child("foreign-limit");
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_selected_system_provider_refuses_unavailable_hard_limit() {
    run_child("unavailable-provider");
}

#[test]
fn engine_bootstrap_child() {
    let Ok(case) = std::env::var("LAYERFS_ENGINE_BOOTSTRAP_CASE") else {
        return;
    };
    match case.as_str() {
        "owned" => owned(),
        "query-first" => query_first(),
        "foreign" => foreign(),
        "native-cap" => native_cap(),
        "sql-nomem" => sql_nomem(),
        "foreign-limit" => foreign_limit(),
        #[cfg(target_os = "macos")]
        "unavailable-provider" => unavailable_provider(),
        other => panic!("unknown external case {other}"),
    }
    println!("engine case complete: {case}");
}

#[cfg(target_os = "macos")]
fn unavailable_provider() {
    // This distinct proof owns the observed Apple system provider's refusal;
    // all eligible bootstrap/Store/NOMEM proofs above remain strict.
    let failure = unsafe { bootstrap_exclusive() }.unwrap_err();
    assert_eq!(failure.stage(), EngineBootstrapStage::Readback);
    assert!(matches!(
        failure.cause(),
        StorageError::UnsupportedPolicy {
            field: "SQLite required hard heap limit"
        }
    ));
    let custody = *failure.custody();
    assert!(custody.memstatus_acknowledged);
    assert!(custody.initialize_attempted && custody.initialized_by_bootstrap);
    assert_eq!(custody.previous_hard_heap_limit, Some(0));
    assert!(custody.hard_limit_set_attempted);
    assert_eq!(custody.setter_previous_hard_heap_limit, Some(0));
    assert!(!custody.hard_limit_installed && !failure.limit_installed());
    assert_eq!(custody.observed_hard_heap_limit, Some(0));
    assert!(!custody.probe_issued && !custody.probe_free_attempted);
    assert_eq!(custody.probe_allocated_bytes, None);
    assert_eq!(unsafe { ffi::sqlite3_hard_heap_limit64(-1) }, 0);
    assert!(std::ptr::eq(
        failure,
        unsafe { bootstrap_exclusive() }.unwrap_err()
    ));
    assert_eq!(*failure.custody(), custody);
    assert_eq!(unsafe { ffi::sqlite3_hard_heap_limit64(-1) }, 0);
    let version = rusqlite::version();
    let source_pointer = unsafe { ffi::sqlite3_sourceid() };
    assert!(!source_pointer.is_null());
    let source = unsafe { std::ffi::CStr::from_ptr(source_pointer) }
        .to_str()
        .unwrap();
    assert_eq!(version, "3.51.0");
    assert_eq!(
        source,
        "2025-06-12 13:14:41 f0ca7bba1c5e232e5d279fad6338121ab55af0c8c68c84cdfb18ba5114dcaapl"
    );
    eprintln!("actual selected Darwin provider version={version} source={source}; {failure}; setter attempted, required limit unavailable, no EngineGuard/owned probe/Store/scratch; exact original refusal cached");
}

fn owned() {
    let Some(guard) = bootstrap() else {
        return;
    };
    let profile = guard.profile();
    let boot = guard.bootstrap_observation();
    assert_eq!(profile.hard_heap_limit_bytes, ENGINE_HEAP_LIMIT_BYTES);
    assert_eq!(profile.probe_request_bytes, ENGINE_PROBE_REQUEST_BYTES);
    assert_eq!(profile.provider_version, rusqlite::version());
    assert!(!profile.provider_source_id.is_empty());
    assert!(boot.probe_allocated_bytes >= ENGINE_PROBE_REQUEST_BYTES);
    assert_eq!(
        boot.during_probe.used_bytes - boot.before_probe.used_bytes,
        boot.probe_allocated_bytes
    );
    assert_eq!(
        boot.during_probe.allocations,
        boot.before_probe.allocations + 1
    );
    assert_eq!(boot.after_probe.used_bytes, boot.before_probe.used_bytes);
    assert_eq!(boot.after_probe.allocations, boot.before_probe.allocations);
    assert!(std::ptr::eq(
        guard,
        bootstrap().expect("same established guard")
    ));
    eprintln!("actual linked engine profile={profile:?}; exclusive startup probe={boot:?}; highwaters are lifetime, no physical/progress claim");

    let directory = support::TempDir::new("owned_engine");
    let store = support::create_store(&directory.store_path("engine"));
    let (objects, root, _) = support::construct_file(b"native-guarded-real-store-bytes");
    support::save_all(&store, &objects).unwrap();
    assert_eq!(
        support::read_logical(&store, root),
        b"native-guarded-real-store-bytes"
    );
    let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
    let mut scratch = authority.begin([0x31; 32], 1).unwrap();
    let scope =
        StateScope::new(scratch.selection().clone(), 1, StateTable::DirectoryRoots).unwrap();
    scratch
        .append(
            &scope,
            &[StateRecord::directory_root(&scope, 1, root).unwrap()],
        )
        .unwrap();
    let seal = scratch.seal(&scope).unwrap();
    assert_eq!(seal.records(), 1);
    let live = guard.validate().unwrap();
    assert_eq!(live.hard_heap_limit_bytes, ENGINE_HEAP_LIMIT_BYTES);
    assert!(live.memory.used_bytes > boot.after_probe.used_bytes);
    eprintln!("real live Store+scratch native observation={live:?}; no complete-shape/protected-catalog admission");
    scratch.complete_phase(&scope).unwrap();
    scratch.release().unwrap();
    assert_eq!(authority.reserved_bytes().unwrap(), 0);
    drop(scratch);
    drop(authority);
    drop(store);
    let actual = Connection::open_in_memory().unwrap();
    let source: String = actual
        .query_row("SELECT sqlite_source_id()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(source, profile.provider_source_id);
    let mut statement = actual.prepare("PRAGMA compile_options").unwrap();
    let options = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap();
    for (count, option) in options.enumerate() {
        let option = option.unwrap();
        assert!(option.len() <= 1024 && count < 128);
        eprintln!("actual linked SQLite compile option: {option}");
    }
}

fn query_first() {
    // One deliberately prior automatic initializer under exclusive ownership.
    let prior = unsafe { ffi::sqlite3_hard_heap_limit64(-1) };
    let failure = unsafe { bootstrap_exclusive() }.unwrap_err();
    assert_eq!(failure.stage(), EngineBootstrapStage::MemStatus);
    assert!(
        matches!(failure.cause(), StorageError::Engine(rusqlite::Error::SqliteFailure(code, _)) if code.extended_code == ffi::SQLITE_MISUSE)
    );
    assert!(!failure.custody().memstatus_acknowledged);
    assert!(
        !failure.initialized_by_bootstrap()
            && !failure.limit_installed()
            && !failure.custody().probe_issued
    );
    assert_eq!(unsafe { ffi::sqlite3_hard_heap_limit64(-1) }, prior);
    assert!(std::ptr::eq(
        failure,
        unsafe { bootstrap_exclusive() }.unwrap_err()
    ));
    eprintln!("real negative-query auto-init trap: prior_limit={prior}; {failure}");
}

fn foreign() {
    let foreign = Connection::open_in_memory().unwrap();
    foreign
        .execute_batch("CREATE TABLE original(value INTEGER); INSERT INTO original VALUES(41);")
        .unwrap();
    let prior = unsafe { ffi::sqlite3_hard_heap_limit64(-1) };
    let failure = unsafe { bootstrap_exclusive() }.unwrap_err();
    assert_eq!(failure.stage(), EngineBootstrapStage::MemStatus);
    assert!(
        matches!(failure.cause(), StorageError::Engine(rusqlite::Error::SqliteFailure(code, _)) if code.extended_code == ffi::SQLITE_MISUSE)
    );
    assert_eq!(
        foreign
            .query_row::<i64, _, _>("SELECT value FROM original", [], |row| row.get(0))
            .unwrap(),
        41
    );
    assert_eq!(unsafe { ffi::sqlite3_hard_heap_limit64(-1) }, prior);
    assert!(
        !failure.initialized_by_bootstrap()
            && !failure.limit_installed()
            && !failure.custody().probe_issued
    );
    assert!(std::ptr::eq(
        failure,
        unsafe { bootstrap_exclusive() }.unwrap_err()
    ));
    eprintln!("real initialized foreign provider preserved: {failure}; no owned Store/catalog/scratch created");
}

fn native_cap() {
    let Some(guard) = bootstrap() else {
        return;
    };
    let before = guard.validate().unwrap().memory;
    let mut owners = pressure::until_refusal();
    assert!(!owners.is_empty() && owners.len() < 32);
    let bytes: u64 = owners.iter().map(|owner| owner.bytes).sum();
    let during = guard.validate().unwrap().memory;
    assert_eq!(during.used_bytes, before.used_bytes + bytes);
    assert_eq!(during.allocations, before.allocations + owners.len() as u64);
    assert!(during.used_bytes <= ENGINE_HEAP_LIMIT_BYTES);
    eprintln!("actual native cap: requested1MiB each, owners={}, actual_msize_sum={bytes}, before={before:?}, refused_current={during:?}; stop at first refusal", owners.len());
    owners.clear();
    let after = guard.validate().unwrap().memory;
    assert_eq!(after.used_bytes, before.used_bytes);
    assert_eq!(after.allocations, before.allocations);
    assert_eq!(
        unsafe { ffi::sqlite3_hard_heap_limit64(-1) },
        ENGINE_HEAP_LIMIT_BYTES as i64
    );
    eprintln!(
        "known native owners freed once: {after:?}; no free-and-retry, no phase-highwater claim"
    );
}

fn sql_nomem() {
    let Some(guard) = bootstrap() else {
        return;
    };
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF; PRAGMA temp_store=MEMORY; CREATE TABLE bytes(value BLOB);").unwrap();
    connection.busy_timeout(std::time::Duration::ZERO).unwrap();
    layerfs_storage::sqlite::write::begin_immediate(&connection).unwrap();
    let mut owners = pressure::until_refusal();
    let error = connection
        .execute("INSERT INTO bytes VALUES(zeroblob(1048576))", [])
        .unwrap_err();
    assert!(
        matches!(error, rusqlite::Error::SqliteFailure(code, _) if code.code == rusqlite::ErrorCode::OutOfMemory),
        "actual SQL error={error:?}"
    );
    let autocommit = connection.is_autocommit();
    let cleanup = if autocommit {
        None
    } else {
        Some(layerfs_storage::sqlite::write::rollback(&connection))
    };
    eprintln!("real SQL NOMEM original={error:?}; engine_autocommit_after_failure={autocommit}; exact explicit C2 rollback={cleanup:?}; no COMMIT/resend/reset/guessed adoption");
    if let Some(Err(cleanup)) = &cleanup {
        // The original typed C2 cleanup failure is already recorded. Keep this
        // exact unacknowledged connection until the isolated process exits;
        // neither Drop nor a query can repair/adopt an unknown SQL outcome.
        std::mem::forget(connection);
        panic!("actual cleanup failure retained above: {cleanup:?}");
    }
    let before_free = guard.validate().unwrap().memory;
    let bytes: u64 = owners.iter().map(|owner| owner.bytes).sum();
    let count = owners.len() as u64;
    owners.clear();
    let after_free = guard.validate().unwrap().memory;
    assert_eq!(after_free.used_bytes, before_free.used_bytes - bytes);
    assert_eq!(after_free.allocations, before_free.allocations - count);
    assert!(connection.is_autocommit());
    assert_eq!(
        connection
            .query_row::<i64, _, _>("SELECT count(*) FROM bytes", [], |row| row.get(0))
            .unwrap(),
        0
    );
    connection.close().unwrap();
    eprintln!("real SQL fixture cleanup complete; native pressure owners freed once; observation={after_free:?}; largest legal C2/C5 shapes remain separate/unrun");
}

fn foreign_limit() {
    let Some(guard) = bootstrap() else {
        return;
    };
    let foreign = ENGINE_HEAP_LIMIT_BYTES as i64 / 2;
    assert_eq!(
        unsafe { ffi::sqlite3_hard_heap_limit64(foreign) },
        ENGINE_HEAP_LIMIT_BYTES as i64
    );
    assert!(matches!(
        guard.validate(),
        Err(StorageError::Integrity("SQLite engine hard limit changed"))
    ));
    assert!(guard.is_quarantined());
    assert_eq!(unsafe { ffi::sqlite3_hard_heap_limit64(-1) }, foreign);
    assert!(matches!(
        guard.validate(),
        Err(StorageError::Integrity("SQLite engine guard quarantined"))
    ));
    assert!(std::ptr::eq(
        guard,
        bootstrap().expect("same established guard")
    ));
    assert!(guard.is_quarantined());
    assert_eq!(unsafe { ffi::sqlite3_hard_heap_limit64(-1) }, foreign);
    eprintln!("foreign lower limit={foreign} remains untouched; owner terminally quarantined, no repair/rebootstrap/reset");
}
